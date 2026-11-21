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
    server::{
        decode_progress, decode_show_message, progress_params, respond_to_server_request,
        show_message_params,
    },
    transport::LspTransport,
};
use crate::language::{
    DocumentEvent, LanguageError, LanguageEvent, LanguageService, NavigationRequest,
};

type Documents = Arc<Mutex<HashMap<PathBuf, String>>>;
type EventHandler = dyn Fn(Result<LanguageEvent, LanguageError>) + Send + 'static;

/// Split process ownership for concurrent diagnostics reads and navigation writes.
pub struct LspRuntime {
    child: LspChild,
    outgoing: Option<mpsc::Sender<Value>>,
    documents: Documents,
    pending: VecDeque<ServerMessage>,
    incoming: mpsc::Receiver<ServerMessage>,
    next_request_id: u64,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
}

impl LspRuntime {
    pub fn start(
        process: LspProcess,
        on_event: impl Fn(Result<LanguageEvent, LanguageError>) + Send + 'static,
    ) -> Self {
        let parts = process.into_parts();
        let (reader, writer, buffer) = parts.transport.into_parts();
        let documents = Arc::new(Mutex::new(parts.documents));
        let (sender, receiver) = mpsc::channel();
        let (outgoing, outgoing_receiver) = mpsc::channel();
        let mut progress = HashMap::new();
        for message in parts.pending {
            route_message(
                message,
                &documents,
                &on_event,
                &mut progress,
                &sender,
                &parts.workspace_root,
                &outgoing,
            );
        }
        let mut writer_transport = LspTransport::new(std::io::empty(), writer);
        let writer = std::thread::spawn(move || {
            while let Ok(message) = outgoing_receiver.recv() {
                if writer_transport.send(&message).is_err() {
                    break;
                }
            }
        });
        let reader_documents = documents.clone();
        let reader_outgoing = outgoing.clone();
        let workspace_root = parts.workspace_root.clone();
        let mut transport = LspTransport::from_parts(reader, std::io::sink(), buffer);
        let reader = std::thread::spawn(move || {
            loop {
                match transport.receive() {
                    Ok(message) => match decode_server_message(message) {
                        Ok(message) => route_message(
                            message,
                            &reader_documents,
                            &on_event,
                            &mut progress,
                            &sender,
                            &workspace_root,
                            &reader_outgoing,
                        ),
                        Err(error) => {
                            on_event(Ok(LanguageEvent::Failure(error)));
                            break;
                        }
                    },
                    Err(error) => {
                        on_event(Ok(LanguageEvent::Failure(error)));
                        break;
                    }
                }
            }
        });
        Self {
            child: parts.child,
            outgoing: Some(outgoing),
            documents,
            pending: VecDeque::new(),
            incoming: receiver,
            next_request_id: parts.next_request_id,
            reader: Some(reader),
            writer: Some(writer),
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
        self.outgoing
            .as_ref()
            .ok_or_else(|| LanguageError("language server writer stopped".to_string()))?
            .send(message)
            .map_err(|_| LanguageError("language server writer stopped".to_string()))
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
        self.outgoing.take();
        self.child.terminate();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        if let Some(writer) = self.writer.take() {
            let _ = writer.join();
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
    on_event: &EventHandler,
    progress: &mut HashMap<String, String>,
    incoming: &mpsc::Sender<ServerMessage>,
    workspace_root: &Path,
    outgoing: &mpsc::Sender<Value>,
) {
    match message {
        ServerMessage::Request { id, method, params } => {
            let response = respond_to_server_request(id, &method, &params, workspace_root);
            let _ = outgoing.send(response);
        }
        message => {
            if let Some(params) = diagnostics_params(&message) {
                let result = decode_batch(params, |path| document_text(documents, path))
                    .map(LanguageEvent::Diagnostics);
                on_event(result);
            } else if let Some(params) = show_message_params(&message) {
                on_event(decode_show_message(params).map(LanguageEvent::Message));
            } else if let Some(params) = progress_params(&message) {
                match decode_progress(params, progress) {
                    Ok(Some(progress)) => on_event(Ok(LanguageEvent::Progress(progress))),
                    Ok(None) => {}
                    Err(error) => on_event(Err(error)),
                }
            } else {
                let _ = incoming.send(message);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use editor::DocumentPosition;

    use super::*;
    use crate::language::{
        LanguageMessageKind, LanguageServerSpec, NavigationKind, NavigationRequestId,
    };

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
request='{"jsonrpc":"2.0","id":99,"method":"client/registerCapability","params":{"registrations":[]}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#request}" "$request"
read_frame
[[ $frame == *'"id":99'* && $frame == *'"result":null'* ]] || exit 3
show_message='{"jsonrpc":"2.0","method":"window/showMessage","params":{"type":2,"message":"workspace loaded"}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#show_message}" "$show_message"
progress='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"indexing","value":{"kind":"begin","title":"Indexing"}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#progress}" "$progress"
progress='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"indexing","value":{"kind":"report","message":"workspace","percentage":50}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#progress}" "$progress"
progress='{"jsonrpc":"2.0","method":"$/progress","params":{"token":"indexing","value":{"kind":"end","message":"done"}}}'
printf 'Content-Length: %s\r\n\r\n%s' "${#progress}" "$progress"
notification="{\"jsonrpc\":\"2.0\",\"method\":\"textDocument/publishDiagnostics\",\"params\":{\"uri\":\"file://$PWD/note.txt\",\"diagnostics\":[]}}"
printf 'Content-Length: %s\r\n\r\n%s' "${#notification}" "$notification"
read_frame
body='{"jsonrpc":"2.0","id":2,"result":[]}'
printf 'Content-Length: %s\r\n\r\n%s' "${#body}" "$body"
"#;

    fn receive_event(
        receiver: &mpsc::Receiver<Result<LanguageEvent, LanguageError>>,
    ) -> LanguageEvent {
        receiver
            .recv_timeout(Duration::from_secs(2))
            .unwrap()
            .unwrap()
    }

    #[test]
    fn runtime_routes_diagnostics_and_navigation_independently() {
        let workspace = tempfile::tempdir().unwrap();
        let spec = LanguageServerSpec::new("test", "bash", ["-c", SERVER]);
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
        let message = receive_event(&receiver);
        let LanguageEvent::Message(message) = message else {
            panic!("expected server message");
        };
        assert_eq!(message.kind, LanguageMessageKind::Warning);
        assert_eq!(message.message, "workspace loaded");

        let begin = receive_event(&receiver);
        let LanguageEvent::Progress(begin) = begin else {
            panic!("expected progress begin");
        };
        assert_eq!(begin.title, "Indexing");

        let report = receive_event(&receiver);
        let LanguageEvent::Progress(report) = report else {
            panic!("expected progress report");
        };
        assert_eq!(report.title, "Indexing");
        assert_eq!(report.message.as_deref(), Some("workspace"));
        assert_eq!(report.percentage, Some(50));

        let end = receive_event(&receiver);
        let LanguageEvent::Progress(end) = end else {
            panic!("expected progress end");
        };
        assert_eq!(end.title, "Indexing");
        assert_eq!(end.message.as_deref(), Some("done"));

        let diagnostics = receive_event(&receiver);
        let LanguageEvent::Diagnostics(diagnostics) = diagnostics else {
            panic!("expected diagnostics");
        };
        assert_eq!(diagnostics.path, path);
        assert!(diagnostics.diagnostics.is_empty());

        let failure = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(matches!(failure, Ok(LanguageEvent::Failure(_))));
    }
}
