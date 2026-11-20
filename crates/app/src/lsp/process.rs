use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::Value;
use url::Url;

use super::{
    ServerMessage, decode_server_message,
    diagnostics::decode_diagnostics as decode_batch,
    navigation::{decode_locations, navigation_method, navigation_params},
    notification_message, request_message,
    transport::LspTransport,
};
use crate::language::{
    DiagnosticBatch, DocumentEvent, LanguageError, LanguageServerSpec, LanguageService,
    NavigationRequest,
};

/// A live language-server child process and its stdio transport.
pub struct LspProcess {
    child: Child,
    transport: LspTransport<ChildStdout, ChildStdin>,
    documents: HashMap<PathBuf, String>,
    pending: VecDeque<ServerMessage>,
    next_request_id: u64,
}

impl LspProcess {
    pub fn spawn(
        spec: &LanguageServerSpec,
        workspace_root: &std::path::Path,
    ) -> Result<Self, LanguageError> {
        let mut child = Command::new(spec.command())
            .args(spec.args())
            .current_dir(workspace_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| LanguageError(error.to_string()))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LanguageError("language server stdin is unavailable".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LanguageError("language server stdout is unavailable".to_string()))?;
        let transport = LspTransport::new(stdout, stdin);
        let mut process = Self {
            child,
            transport,
            documents: HashMap::new(),
            pending: VecDeque::new(),
            next_request_id: 2,
        };
        process.initialize(workspace_root)?;
        Ok(process)
    }

    fn initialize(&mut self, workspace_root: &std::path::Path) -> Result<(), LanguageError> {
        let root_uri = Url::from_file_path(workspace_root)
            .map_err(|_| LanguageError("workspace root must be an absolute path".to_string()))?;
        self.transport.send(&request_message(
            1,
            "initialize",
            serde_json::json!({
                "processId": std::process::id(),
                "rootUri": root_uri.to_string(),
                "capabilities": {"textDocumentSync": 1},
            }),
        ))?;

        loop {
            let message = self.receive()?;
            if let ServerMessage::Response { id: 1, result } = message {
                result.map_err(|error| LanguageError(error.message))?;
                break;
            } else {
                self.pending.push_back(message);
            }
        }
        self.transport
            .send(&notification_message("initialized", Value::Null))
    }

    /// Receive the next queued or transport-level server message.
    pub fn receive(&mut self) -> Result<ServerMessage, LanguageError> {
        match self.pending.pop_front() {
            Some(message) => Ok(message),
            None => decode_server_message(self.transport.receive()?),
        }
    }

    /// Wait for the next diagnostics notification without discarding other messages.
    pub fn receive_diagnostics(&mut self) -> Result<Option<DiagnosticBatch>, LanguageError> {
        let pending = self
            .pending
            .iter()
            .position(|message| diagnostics_params(message).is_some());
        let message = match pending {
            Some(index) => self
                .pending
                .remove(index)
                .expect("pending message must exist"),
            None => decode_server_message(self.transport.receive()?)?,
        };
        if let Some(params) = diagnostics_params(&message) {
            return decode_batch(params, |path| self.document_text(path)).map(Some);
        }
        self.pending.push_back(message);
        Ok(None)
    }

    fn document_text(&self, path: &Path) -> Result<String, LanguageError> {
        if let Some(text) = self.documents.get(path) {
            return Ok(text.clone());
        }
        std::fs::read_to_string(path).map_err(|error| LanguageError(error.to_string()))
    }
}

impl LanguageService for LspProcess {
    fn synchronize(&mut self, event: DocumentEvent) -> Result<(), LanguageError> {
        match event {
            DocumentEvent::Open {
                path,
                language,
                version,
                text,
            } => {
                let uri = Url::from_file_path(&path)
                    .map_err(|_| LanguageError("document path must be absolute".to_string()))?;
                if self.documents.contains_key(&path) {
                    self.transport.send(&notification_message(
                        "textDocument/didChange",
                        serde_json::json!({
                            "textDocument": {"uri": uri.to_string(), "version": version},
                            "contentChanges": [{"text": text}],
                        }),
                    ))?;
                } else {
                    self.transport.send(&notification_message(
                        "textDocument/didOpen",
                        serde_json::json!({
                            "textDocument": {
                                "uri": uri.to_string(),
                                "languageId": language,
                                "version": version,
                                "text": text,
                            }
                        }),
                    ))?;
                }
                self.documents.insert(path, text);
            }
            DocumentEvent::Change {
                path,
                version,
                text,
            } => {
                let uri = Url::from_file_path(&path)
                    .map_err(|_| LanguageError("document path must be absolute".to_string()))?;
                self.transport.send(&notification_message(
                    "textDocument/didChange",
                    serde_json::json!({
                        "textDocument": {"uri": uri.to_string(), "version": version},
                        "contentChanges": [{"text": text}],
                    }),
                ))?;
                self.documents.insert(path, text);
            }
            DocumentEvent::Close { path } => {
                let uri = Url::from_file_path(&path)
                    .map_err(|_| LanguageError("document path must be absolute".to_string()))?;
                self.transport.send(&notification_message(
                    "textDocument/didClose",
                    serde_json::json!({"textDocument": {"uri": uri.to_string()}}),
                ))?;
                self.documents.remove(&path);
            }
        }
        Ok(())
    }

    fn navigate(
        &mut self,
        request: NavigationRequest,
    ) -> Result<Vec<crate::language::SourceLocation>, LanguageError> {
        let text = self.document_text(&request.path)?;
        let params = navigation_params(&request, &text)?;
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.transport.send(&request_message(
            id,
            navigation_method(request.kind),
            params,
        ))?;

        loop {
            let message = decode_server_message(self.transport.receive()?)?;
            match message {
                ServerMessage::Response {
                    id: response_id,
                    result,
                } if response_id == id => {
                    let value = result.map_err(|error| LanguageError(error.message))?;
                    return decode_locations(&value, |path| self.document_text(path));
                }
                other => self.pending.push_back(other),
            }
        }
    }
}

fn diagnostics_params(message: &ServerMessage) -> Option<&Value> {
    let ServerMessage::Notification { method, params } = message else {
        return None;
    };
    (method == "textDocument/publishDiagnostics").then_some(params)
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAKE_SERVER: &str = r#"
read_frame() {
  local length=0 line
  while IFS= read -r line; do
    line=${line%$'\r'}
    if [[ -z $line ]]; then
      break
    fi
    if [[ $line == Content-Length:* ]]; then
      length=${line#Content-Length: }
      length=${length//[[:space:]]/}
    fi
  done
  dd if=/dev/stdin bs=1 count="$length" of=/dev/null status=none
}
read_frame
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
read_frame
"#;

    #[test]
    fn launches_and_initializes_a_real_stdio_process() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", FAKE_SERVER]);

        let process = LspProcess::spawn(&spec, workspace.path()).unwrap();
        drop(process);
    }

    #[test]
    fn failed_handshake_cleans_up_the_process() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", "exit 0"]);

        assert!(LspProcess::spawn(&spec, workspace.path()).is_err());
    }

    #[test]
    fn synchronizes_and_navigates_without_losing_notifications() {
        const SERVER: &str = r#"
read_frame() {
  local length=0 line
  while IFS= read -r line; do
    line=${line%$'\r'}
    if [[ -z $line ]]; then
      break
    fi
    if [[ $line == Content-Length:* ]]; then
      length=${line#Content-Length: }
      length=${length//[[:space:]]/}
    fi
  done
  frame=$(dd if=/dev/stdin bs=1 count="$length" status=none)
}
read_frame
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
read_frame
read_frame
[[ $frame == *'"text":"fn main() {}"'* ]] || exit 2
read_frame
notification="{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"file://$PWD/note.txt\",\"diagnostics\":[]}}"
printf 'Content-Length: %s\r\n\r\n%s' "${#notification}" "$notification"
body="{\"jsonrpc\":\"2.0\",\"id\":2,\"result\":[{\"uri\":\"file://$PWD/note.txt\",\"range\":{\"start\":{\"line\":0,\"character\":0},\"end\":{\"line\":0,\"character\":1}}}]}"
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
read_frame
"#;
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", SERVER]);
        let mut process = LspProcess::spawn(&spec, workspace.path()).unwrap();
        let path = workspace.path().join("note.txt");
        process
            .synchronize(DocumentEvent::Open {
                path: path.clone(),
                language: "rust".to_string(),
                version: 1,
                text: "fn main() {}".to_string(),
            })
            .unwrap();

        let locations = process
            .navigate(NavigationRequest {
                id: crate::language::NavigationRequestId(1),
                kind: crate::language::NavigationKind::Definition,
                buffer: editor::EditorSession::new()
                    .insert_document(editor::DocumentFactory::new().create("")),
                path: path.clone(),
                revision: 1,
                position: editor::DocumentPosition {
                    line: 0,
                    character: 0,
                },
            })
            .unwrap();
        assert_eq!(locations.len(), 1);
        assert_eq!(locations[0].path, path);
        assert_eq!(locations[0].end.character, 1);

        let batch = process.receive_diagnostics().unwrap().unwrap();
        assert_eq!(batch.path, path);
        assert!(batch.diagnostics.is_empty());

        process.synchronize(DocumentEvent::Close { path }).unwrap();
    }
}
