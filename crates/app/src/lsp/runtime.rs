use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::JoinHandle,
};

use serde_json::Value;
use url::Url;

use super::{
    ServerMessage, decode_server_message,
    diagnostics::decode_diagnostics as decode_batch,
    navigation::{decode_locations, navigation_method, navigation_params},
    notification_message,
    process::{LspChild, LspProcess, diagnostics_params},
    request_message,
    transport::LspTransport,
};
use crate::language::{
    DiagnosticBatch, DocumentEvent, LanguageError, LanguageService, NavigationRequest,
};

type Documents = Arc<Mutex<HashMap<PathBuf, String>>>;
type DiagnosticsHandler = dyn Fn(Result<DiagnosticBatch, LanguageError>) + Send + 'static;

/// Split process ownership for concurrent diagnostics reads and navigation writes.
pub struct LspRuntime {
    child: LspChild,
    writer: LspTransport<std::io::Empty, std::process::ChildStdin>,
    documents: Documents,
    pending: VecDeque<ServerMessage>,
    incoming: mpsc::Receiver<ServerMessage>,
    next_request_id: u64,
    reader: Option<JoinHandle<()>>,
}

impl LspRuntime {
    pub fn start(
        process: LspProcess,
        on_diagnostics: impl Fn(Result<DiagnosticBatch, LanguageError>) + Send + 'static,
    ) -> Self {
        let parts = process.into_parts();
        let (reader, writer, buffer) = parts.transport.into_parts();
        let documents = Arc::new(Mutex::new(parts.documents));
        let (sender, receiver) = mpsc::channel();
        for message in parts.pending {
            route_message(message, &documents, &on_diagnostics, &sender);
        }
        let reader_documents = documents.clone();
        let mut transport = LspTransport::from_parts(reader, std::io::sink(), buffer);
        let reader = std::thread::spawn(move || {
            loop {
                match transport.receive() {
                    Ok(message) => match decode_server_message(message) {
                        Ok(message) => {
                            route_message(message, &reader_documents, &on_diagnostics, &sender)
                        }
                        Err(error) => {
                            on_diagnostics(Err(error));
                            break;
                        }
                    },
                    Err(error) => {
                        on_diagnostics(Err(error));
                        break;
                    }
                }
            }
        });
        Self {
            child: parts.child,
            writer: LspTransport::new(std::io::empty(), writer),
            documents,
            pending: VecDeque::new(),
            incoming: receiver,
            next_request_id: parts.next_request_id,
            reader: Some(reader),
        }
    }

    fn receive(&mut self) -> Result<ServerMessage, LanguageError> {
        match self.pending.pop_front() {
            Some(message) => Ok(message),
            None => Ok(self
                .incoming
                .recv()
                .map_err(|_| LanguageError("language server reader stopped".to_string()))?),
        }
    }

    fn write(&mut self, message: Value) -> Result<(), LanguageError> {
        self.writer.send(&message)
    }

    fn contains_document(&self, path: &Path) -> bool {
        self.documents
            .lock()
            .map(|documents| documents.contains_key(path))
            .unwrap_or(false)
    }

    fn set_document(&self, path: PathBuf, text: String) -> Result<(), LanguageError> {
        self.documents
            .lock()
            .map_err(|_| LanguageError("document map lock is poisoned".to_string()))?
            .insert(path, text);
        Ok(())
    }

    fn remove_document(&self, path: &Path) -> Result<(), LanguageError> {
        self.documents
            .lock()
            .map_err(|_| LanguageError("document map lock is poisoned".to_string()))?
            .remove(path);
        Ok(())
    }
}

impl LanguageService for LspRuntime {
    fn synchronize(&mut self, event: DocumentEvent) -> Result<(), LanguageError> {
        match event {
            DocumentEvent::Open {
                path,
                language,
                version,
                text,
            } => {
                let uri = document_uri(&path)?;
                let existing = self.contains_document(&path);
                self.set_document(path.clone(), text.clone())?;
                if existing {
                    self.write(notification_message(
                        "textDocument/didChange",
                        serde_json::json!({
                            "textDocument": {"uri": uri.to_string(), "version": version},
                            "contentChanges": [{"text": text}],
                        }),
                    ))?;
                } else {
                    self.write(notification_message(
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
            }
            DocumentEvent::Change {
                path,
                version,
                text,
            } => {
                let uri = document_uri(&path)?;
                self.set_document(path, text.clone())?;
                self.write(notification_message(
                    "textDocument/didChange",
                    serde_json::json!({
                        "textDocument": {"uri": uri.to_string(), "version": version},
                        "contentChanges": [{"text": text}],
                    }),
                ))?;
            }
            DocumentEvent::Close { path } => {
                let uri = document_uri(&path)?;
                self.remove_document(&path)?;
                self.write(notification_message(
                    "textDocument/didClose",
                    serde_json::json!({"textDocument": {"uri": uri.to_string()}}),
                ))?;
            }
        }
        Ok(())
    }

    fn navigate(
        &mut self,
        request: NavigationRequest,
    ) -> Result<Vec<crate::language::SourceLocation>, LanguageError> {
        let text = document_text(&self.documents, &request.path)?;
        let params = navigation_params(&request, &text)?;
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1);
        self.write(request_message(id, navigation_method(request.kind), params))?;

        loop {
            match self.receive()? {
                ServerMessage::Response {
                    id: response_id,
                    result,
                } if response_id == id => {
                    let value = result.map_err(|error| LanguageError(error.message))?;
                    return decode_locations(&value, |path| document_text(&self.documents, path));
                }
                other => self.pending.push_back(other),
            }
        }
    }
}

impl Drop for LspRuntime {
    fn drop(&mut self) {
        self.child.terminate();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn document_uri(path: &Path) -> Result<Url, LanguageError> {
    Url::from_file_path(path)
        .map_err(|_| LanguageError("document path must be absolute".to_string()))
}

fn document_text(documents: &Documents, path: &Path) -> Result<String, LanguageError> {
    let cached = documents
        .lock()
        .map_err(|_| LanguageError("document map lock is poisoned".to_string()))?
        .get(path)
        .cloned();
    match cached {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path).map_err(|error| LanguageError(error.to_string())),
    }
}

fn route_message(
    message: ServerMessage,
    documents: &Documents,
    on_diagnostics: &DiagnosticsHandler,
    incoming: &mpsc::Sender<ServerMessage>,
) {
    if let Some(params) = diagnostics_params(&message) {
        let result = decode_batch(params, |path| document_text(documents, path));
        on_diagnostics(result);
    } else {
        let _ = incoming.send(message);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use editor::DocumentPosition;

    use super::*;
    use crate::language::{LanguageServerSpec, NavigationKind, NavigationRequestId};

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
  dd if=/dev/stdin bs=1 count="$length" of=/dev/null status=none
}
read_frame
body='{"jsonrpc":"2.0","id":1,"result":{"capabilities":{}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
read_frame
read_frame
notification="{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"file://$PWD/note.txt\",\"diagnostics\":[]}}"
printf 'Content-Length: %s\r\n\r\n%s' "${#notification}" "$notification"
read_frame
body='{"jsonrpc":"2.0","id":2,"result":[]}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
"#;

    #[test]
    fn runtime_routes_diagnostics_and_navigation_independently() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", &["-c", SERVER]);
        let process = LspProcess::spawn(&spec, workspace.path()).unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut runtime = LspRuntime::start(process, move |result| {
            let _ = sender.send(result);
        });
        let path = workspace.path().join("note.txt");
        runtime
            .synchronize(DocumentEvent::Open {
                path: path.clone(),
                language: "rust".to_string(),
                version: 1,
                text: "fn main() {}".to_string(),
            })
            .unwrap();

        let locations = runtime
            .navigate(NavigationRequest {
                id: NavigationRequestId(1),
                kind: NavigationKind::Definition,
                buffer: editor::EditorSession::new()
                    .insert_document(editor::DocumentFactory::new().create("")),
                path: path.clone(),
                revision: 1,
                position: DocumentPosition {
                    line: 0,
                    character: 0,
                },
            })
            .unwrap();
        assert!(locations.is_empty());
        let diagnostics = receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap();
        assert_eq!(diagnostics.path, path);
        assert!(diagnostics.diagnostics.is_empty());
    }
}
