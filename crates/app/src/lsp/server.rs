use std::{collections::HashMap, path::Path};

use serde_json::Value;
use url::Url;

use super::{ServerMessage, error_response_message, response_message};
use crate::language::{
    LanguageError, LanguageMessage, LanguageMessageKind, LanguageProgress, LanguageProgressStage,
};

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

pub fn progress_params(message: &ServerMessage) -> Option<&Value> {
    let ServerMessage::Notification { method, params } = message else {
        return None;
    };
    (method == "$/progress").then_some(params)
}

/// Decode work progress and retain begin titles for subsequent report/end events.
pub fn decode_progress(
    params: &Value,
    titles: &mut HashMap<String, String>,
) -> Result<Option<LanguageProgress>, LanguageError> {
    let token = match params.get("token") {
        Some(Value::String(token)) => token.clone(),
        Some(Value::Number(token)) if token.as_i64().is_some() => token.to_string(),
        _ => {
            return Err(LanguageError(
                "$/progress requires a string or integer token".to_string(),
            ));
        }
    };
    let value = params
        .get("value")
        .ok_or_else(|| LanguageError("$/progress requires a progress value".to_string()))?;
    let stage = match value.get("kind").and_then(Value::as_str) {
        Some("begin") => LanguageProgressStage::Begin,
        Some("report") => LanguageProgressStage::Report,
        Some("end") => LanguageProgressStage::End,
        _ => {
            return Err(LanguageError(
                "$/progress requires begin, report, or end".to_string(),
            ));
        }
    };
    let message = match value.get("message") {
        None | Some(Value::Null) => None,
        Some(Value::String(message)) => Some(message.clone()),
        _ => {
            return Err(LanguageError(
                "$/progress message must be a string".to_string(),
            ));
        }
    };
    let percentage = match value.get("percentage") {
        None | Some(Value::Null) => None,
        Some(Value::Number(percentage)) => {
            let Some(percentage) = percentage.as_u64().filter(|percentage| *percentage <= 100)
            else {
                return Err(LanguageError(
                    "$/progress percentage must be between 0 and 100".to_string(),
                ));
            };
            Some(percentage as u32)
        }
        _ => {
            return Err(LanguageError(
                "$/progress percentage must be between 0 and 100".to_string(),
            ));
        }
    };
    let title = if stage == LanguageProgressStage::Begin {
        let title = value
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.is_empty())
            .ok_or_else(|| {
                LanguageError("$/progress begin requires a non-empty title".to_string())
            })?
            .to_string();
        titles.insert(token, title.clone());
        title
    } else {
        let Some(title) = titles.get(&token).cloned() else {
            return Ok(None);
        };
        if stage == LanguageProgressStage::End {
            titles.remove(&token);
        }
        title
    };
    Ok(Some(LanguageProgress {
        stage,
        title,
        message,
        percentage,
    }))
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
    fn tracks_progress_titles_through_begin_report_and_end() {
        let mut titles = HashMap::new();
        let begin = decode_progress(
            &json!({
                "token": "indexing",
                "value": {"kind": "begin", "title": "Indexing", "percentage": 0}
            }),
            &mut titles,
        )
        .unwrap()
        .unwrap();
        assert_eq!(begin.stage, LanguageProgressStage::Begin);
        assert_eq!(begin.title, "Indexing");
        assert_eq!(begin.percentage, Some(0));

        let report = decode_progress(
            &json!({
                "token": "indexing",
                "value": {"kind": "report", "message": "workspace", "percentage": 50}
            }),
            &mut titles,
        )
        .unwrap()
        .unwrap();
        assert_eq!(report.stage, LanguageProgressStage::Report);
        assert_eq!(report.title, "Indexing");
        assert_eq!(report.message.as_deref(), Some("workspace"));

        let end = decode_progress(
            &json!({"token": "indexing", "value": {"kind": "end"}}),
            &mut titles,
        )
        .unwrap()
        .unwrap();
        assert_eq!(end.stage, LanguageProgressStage::End);
        assert_eq!(end.title, "Indexing");
        assert!(titles.is_empty());
        assert!(
            decode_progress(
                &json!({"token": "indexing", "value": {"kind": "report"}}),
                &mut titles,
            )
            .unwrap()
            .is_none()
        );
        assert!(
            decode_progress(
                &json!({
                    "token": "other",
                    "value": {"kind": "begin", "title": "Other", "percentage": 101}
                }),
                &mut titles,
            )
            .is_err()
        );
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
