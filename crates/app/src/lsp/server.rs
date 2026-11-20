use std::path::Path;

use serde_json::Value;
use url::Url;

use super::{error_response_message, response_message};

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
