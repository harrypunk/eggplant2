use std::path::Path;

use serde_json::Value;
use url::Url;

use super::{ServerMessage, error_response_message, response_message};
use crate::language::{LanguageError, LanguageMessage, LanguageMessageKind};

pub fn show_message_params(message: &ServerMessage) -> Option<&Value> {
    let ServerMessage::Notification { method, params } = message else {
        return None;
    };
    (method == "window/showMessage").then_some(params)
}

pub fn decode_show_message(params: &Value) -> Result<LanguageMessage, LanguageError> {
    let kind = match params.get("type").and_then(Value::as_u64) {
        Some(1) => LanguageMessageKind::Error,
        Some(2) => LanguageMessageKind::Warning,
        Some(3) => LanguageMessageKind::Information,
        Some(4) => LanguageMessageKind::Log,
        _ => {
            return Err(LanguageError(
                "window/showMessage requires a valid message type".to_string(),
            ));
        }
    };
    let message = params
        .get("message")
        .and_then(Value::as_str)
        .filter(|message| !message.is_empty())
        .ok_or_else(|| {
            LanguageError("window/showMessage requires a non-empty message".to_string())
        })?;
    Ok(LanguageMessage {
        kind,
        message: message.to_string(),
    })
}

/// Answer the small set of server requests needed by the lightweight integration.
pub fn respond_to_server_request(
    id: u64,
    method: &str,
    params: &Value,
    workspace_root: &Path,
) -> Value {
    match method {
        "workspace/configuration" => {
            let count = params
                .get("items")
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            response_message(id, Value::Array(vec![Value::Null; count]))
        }
        "workspace/workspaceFolders" => {
            let uri = Url::from_file_path(workspace_root).ok();
            let folder = uri.map(|uri| {
                serde_json::json!({
                    "uri": uri.to_string(),
                    "name": workspace_root
                        .file_name()
                        .unwrap_or(workspace_root.as_os_str())
                        .to_string_lossy(),
                })
            });
            response_message(id, folder.into_iter().collect())
        }
        "workspace/applyEdit" => response_message(
            id,
            serde_json::json!({
                "applied": false,
                "failureReason": "workspace edits are not supported",
            }),
        ),
        "client/registerCapability"
        | "client/unregisterCapability"
        | "window/workDoneProgress/create"
        | "window/showMessageRequest"
        | "workspace/semanticTokens/refresh"
        | "workspace/diagnostic/refresh"
        | "workspace/inlayHint/refresh"
        | "workspace/codeLens/refresh" => response_message(id, Value::Null),
        _ => error_response_message(
            id,
            -32601,
            &format!("unsupported server request `{method}`"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn decodes_show_message_severity_and_text() {
        let message =
            decode_show_message(&json!({"type": 2, "message": "workspace loaded"})).unwrap();
        assert_eq!(message.kind, LanguageMessageKind::Warning);
        assert_eq!(message.message, "workspace loaded");

        assert!(decode_show_message(&json!({"type": 9, "message": "bad"})).is_err());
        assert!(decode_show_message(&json!({"type": 3, "message": ""})).is_err());
    }

    #[test]
    fn supports_configuration_and_workspace_folder_requests() {
        let configuration = respond_to_server_request(
            1,
            "workspace/configuration",
            &json!({"items": [{"section": "rust-analyzer"}, {}]}),
            Path::new("/workspace"),
        );
        assert_eq!(configuration["result"], json!([null, null]));

        let folders = respond_to_server_request(
            2,
            "workspace/workspaceFolders",
            &Value::Null,
            Path::new("/workspace"),
        );
        assert_eq!(folders["result"][0]["uri"], "file:///workspace");
        assert_eq!(folders["result"][0]["name"], "workspace");
    }

    #[test]
    fn acknowledges_lifecycle_requests_and_rejects_unknown_methods() {
        let registered = respond_to_server_request(
            3,
            "client/registerCapability",
            &Value::Null,
            Path::new("/workspace"),
        );
        assert_eq!(registered["result"], Value::Null);

        let unknown =
            respond_to_server_request(4, "custom/unknown", &Value::Null, Path::new("/workspace"));
        assert_eq!(unknown["error"]["code"], -32601);
    }
}
