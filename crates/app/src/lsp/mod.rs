pub mod diagnostics;
pub mod navigation;

use serde_json::Value;

use crate::language::LanguageError;

const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
const HEADER_END: &[u8] = b"\r\n\r\n";

pub fn encode_message(message: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(message).expect("a JSON value must serialize");
    format!("Content-Length: {}\r\n\r\n", body.len())
        .into_bytes()
        .into_iter()
        .chain(body)
        .collect()
}

/// Decode one complete stdio frame, retaining partial or subsequent frames.
pub fn decode_message(buffer: &mut Vec<u8>) -> Result<Option<Value>, LanguageError> {
    let Some(header_end) = buffer
        .windows(HEADER_END.len())
        .position(|window| window == HEADER_END)
    else {
        return Ok(None);
    };
    let headers = std::str::from_utf8(&buffer[..header_end])
        .map_err(|error| LanguageError(error.to_string()))?;
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("Content-Length")
                .then_some(value.trim())
        })
        .ok_or_else(|| LanguageError("missing Content-Length header".to_string()))?
        .parse::<usize>()
        .map_err(|error| LanguageError(format!("invalid Content-Length: {error}")))?;
    if length > MAX_MESSAGE_SIZE {
        return Err(LanguageError(format!(
            "LSP message exceeds {MAX_MESSAGE_SIZE} bytes"
        )));
    }

    let body_start = header_end + HEADER_END.len();
    let frame_end = body_start
        .checked_add(length)
        .ok_or_else(|| LanguageError("LSP frame length overflow".to_string()))?;
    if buffer.len() < frame_end {
        return Ok(None);
    }
    let message = serde_json::from_slice(&buffer[body_start..frame_end])
        .map_err(|error| LanguageError(error.to_string()))?;
    buffer.drain(..frame_end);
    Ok(Some(message))
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Response {
        id: u64,
        result: Result<Value, JsonRpcError>,
    },
    Notification {
        method: String,
        params: Value,
    },
}

pub fn request_message(id: u64, method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params,
    })
}

pub fn notification_message(method: &str, params: Value) -> Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
    })
}

pub fn decode_server_message(message: Value) -> Result<ServerMessage, LanguageError> {
    let object = message
        .as_object()
        .ok_or_else(|| LanguageError("JSON-RPC message must be an object".to_string()))?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(LanguageError("expected JSON-RPC 2.0".to_string()));
    }
    if let Some(method) = object.get("method").and_then(Value::as_str) {
        if object.contains_key("id") {
            return Err(LanguageError(
                "server-initiated requests are not supported".to_string(),
            ));
        }
        return Ok(ServerMessage::Notification {
            method: method.to_string(),
            params: object.get("params").cloned().unwrap_or(Value::Null),
        });
    }

    let id = object
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| LanguageError("response requires a numeric ID".to_string()))?;
    match (object.get("result"), object.get("error")) {
        (Some(result), None) => Ok(ServerMessage::Response {
            id,
            result: Ok(result.clone()),
        }),
        (None, Some(error)) => {
            let error = error
                .as_object()
                .ok_or_else(|| LanguageError("JSON-RPC error must be an object".to_string()))?;
            let code = error
                .get("code")
                .and_then(Value::as_i64)
                .ok_or_else(|| LanguageError("JSON-RPC error requires a code".to_string()))?;
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .ok_or_else(|| LanguageError("JSON-RPC error requires a message".to_string()))?;
            Ok(ServerMessage::Response {
                id,
                result: Err(JsonRpcError {
                    code,
                    message: message.to_string(),
                    data: error.get("data").cloned(),
                }),
            })
        }
        _ => Err(LanguageError(
            "response requires exactly one of result or error".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn decodes_partial_and_sequential_frames() {
        let first = json!({"jsonrpc": "2.0", "id": 1, "method": "initialize"});
        let second = json!({"jsonrpc": "2.0", "id": 1, "result": {}});
        let encoded = encode_message(&first);
        let second_frame = encode_message(&second);
        let partial = encoded.len() - 2;
        let mut buffer = encoded[..partial].to_vec();

        assert_eq!(decode_message(&mut buffer).unwrap(), None);
        buffer.extend_from_slice(&encoded[partial..]);
        buffer.extend_from_slice(&second_frame);
        assert_eq!(decode_message(&mut buffer).unwrap(), Some(first));
        assert_eq!(decode_message(&mut buffer).unwrap(), Some(second));
        assert_eq!(decode_message(&mut buffer).unwrap(), None);
    }

    #[test]
    fn rejects_missing_invalid_and_oversized_frames() {
        for bytes in [
            b"Content-Type: application/json\r\n\r\n{}".as_slice(),
            b"Content-Length: no\r\n\r\n{}".as_slice(),
            b"Content-Length: 999999999999999999\r\n\r\n".as_slice(),
        ] {
            assert!(decode_message(&mut bytes.to_vec()).is_err());
        }
    }

    #[test]
    fn builds_requests_and_parses_results() {
        let request = request_message(7, "textDocument/definition", json!({"line": 2}));
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 7);
        assert_eq!(request["method"], "textDocument/definition");

        let response = decode_server_message(json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": {"ok": true}
        }))
        .unwrap();
        assert_eq!(
            response,
            ServerMessage::Response {
                id: 7,
                result: Ok(json!({"ok": true})),
            }
        );
    }

    #[test]
    fn parses_notifications_and_protocol_errors() {
        let notification = decode_server_message(json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": {"diagnostics": []}
        }))
        .unwrap();
        assert_eq!(
            notification,
            ServerMessage::Notification {
                method: "textDocument/publishDiagnostics".to_string(),
                params: json!({"diagnostics": []}),
            }
        );

        let response = decode_server_message(json!({
            "jsonrpc": "2.0",
            "id": 9,
            "error": {"code": -32601, "message": "method not found"}
        }))
        .unwrap();
        assert_eq!(
            response,
            ServerMessage::Response {
                id: 9,
                result: Err(JsonRpcError {
                    code: -32601,
                    message: "method not found".to_string(),
                    data: None,
                }),
            }
        );
    }

    #[test]
    fn rejects_malformed_or_unsupported_server_messages() {
        for message in [
            json!({"jsonrpc": "1.0", "id": 1, "result": null}),
            json!({"jsonrpc": "2.0", "result": null}),
            json!({"jsonrpc": "2.0", "id": 1}),
            json!({"jsonrpc": "2.0", "id": 1, "method": "custom/request"}),
        ] {
            assert!(decode_server_message(message).is_err());
        }
    }
}
