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
}
