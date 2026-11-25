use std::path::{Path, PathBuf};

use editor::{
    DocumentFactory, DocumentOpenCompletion as EditorDocumentOpenCompletion, DocumentOpenRequest,
    DocumentPosition,
};

use crate::{
    buffer,
    data_object::{Context, Model, OpenLocation, StatusMessage, Visibility},
    file_picker, focus,
    manager::effect::{Completion, Effect},
};

pub type DocumentCloseHandler = fn(&mut Model, PathBuf) -> Option<Effect>;

/// Provider-neutral document lifecycle hooks injected at composition.
#[derive(Debug, Default, Clone, Copy)]
pub struct DocumentLifecycle {
    on_close: Option<DocumentCloseHandler>,
}

impl DocumentLifecycle {
    pub fn set_on_close(&mut self, handler: DocumentCloseHandler) {
        self.on_close = Some(handler);
    }
}

#[derive(Debug, Default)]
pub struct DocumentOpening {
    next_id: u64,
    pending: Option<PendingOpen>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenOrigin {
    FilePicker { session: u64 },
    Other,
}

#[derive(Debug)]
struct PendingOpen {
    id: OpenRequestId,
    location: OpenLocation,
    origin: OpenOrigin,
    context: Option<Context>,
}

pub(crate) struct OpenedFile {
    pub origin: OpenOrigin,
    pub context: Option<Context>,
    pub effect: Option<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpenRequestId(u64);

pub struct OpenFileRequest {
    id: OpenRequestId,
    request: DocumentOpenRequest,
}

#[derive(Debug)]
pub struct OpenFileCompletion {
    id: OpenRequestId,
    completion: EditorDocumentOpenCompletion,
}

impl Completion for OpenFileCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        complete_open(model, *self)
    }
}

impl OpenFileRequest {
    pub fn perform(self) -> OpenFileCompletion {
        OpenFileCompletion {
            id: self.id,
            completion: self.request.perform(),
        }
    }
}

impl DocumentOpening {
    fn begin(
        &mut self,
        location: OpenLocation,
        origin: OpenOrigin,
        context: Option<Context>,
    ) -> OpenRequestId {
        self.next_id = self.next_id.wrapping_add(1);
        let id = OpenRequestId(self.next_id);
        self.pending = Some(PendingOpen {
            id,
            location,
            origin,
            context,
        });
        id
    }

    fn complete(&mut self, id: OpenRequestId, path: &Path) -> Option<PendingOpen> {
        let pending = self.pending.as_ref()?;
        if pending.id != id || pending.location.path.as_path() != path {
            return None;
        }
        self.pending.take()
    }

    #[cfg(test)]
    fn pending_location(&self) -> Option<&OpenLocation> {
        self.pending.as_ref().map(|pending| &pending.location)
    }
}

/// Notify the configured lifecycle adapter after the editor closes a file.
pub(crate) fn close_file(model: &mut Model, path: PathBuf) -> Option<Effect> {
    let handler = model.document_lifecycle.on_close;
    handler.and_then(|handler| handler(model, path))
}

/// Start opening a document through the shared text-only document capability.
pub(crate) fn open_file(
    documents: &DocumentFactory,
    model: &mut Model,
    path: &Path,
) -> Option<Effect> {
    open_file_at_with_origin(documents, model, path, None, OpenOrigin::Other)
}

/// Open a document and optionally move its cursor after the normal safe insertion flow.
pub(crate) fn open_file_at(
    documents: &DocumentFactory,
    model: &mut Model,
    path: &Path,
    position: Option<DocumentPosition>,
) -> Option<Effect> {
    open_file_at_with_origin(documents, model, path, position, OpenOrigin::Other)
}

pub(crate) fn open_file_at_with_origin(
    documents: &DocumentFactory,
    model: &mut Model,
    path: &Path,
    position: Option<DocumentPosition>,
    origin: OpenOrigin,
) -> Option<Effect> {
    let id = model.document_open.begin(
        OpenLocation {
            path: path.to_path_buf(),
            position,
        },
        origin,
        crate::composition::input::input_context(model),
    );
    Some(Effect::OpenDocument(OpenFileRequest {
        id,
        request: DocumentOpenRequest::new(documents.clone(), path),
    }))
}

/// Apply the latest document-open result and reconcile the workflow that started it.
fn complete_open(model: &mut Model, completion: OpenFileCompletion) -> Option<Effect> {
    let opened = finish_open_file(model, completion)?;
    let reconcile = crate::composition::input::input_context(model) == opened.context
        && match opened.origin {
            OpenOrigin::FilePicker { session } => {
                model.file_picker.visibility == Visibility::Displayed
                    && model.file_picker.session_revision() == session
            }
            OpenOrigin::Other => true,
        };
    if reconcile {
        if let OpenOrigin::FilePicker { .. } = opened.origin {
            file_picker::close(model);
        }
        focus::focus_editor(model);
    }
    let follow_up = Effect::combine(opened.effect, crate::vcs::sync_active_buffer(model));
    Effect::combine(follow_up, crate::code_navigation::synchronize(model))
}

/// Insert an opened document and return its workflow metadata and follow-up effect.
pub(crate) fn finish_open_file(
    model: &mut Model,
    completion: OpenFileCompletion,
) -> Option<OpenedFile> {
    let OpenFileCompletion { id, completion } = completion;
    let pending = model.document_open.complete(id, completion.path())?;
    let path = completion.path().to_path_buf();
    let position = pending.location.position;
    let opened = match completion.into_result() {
        Ok(opened) => opened,
        Err(error) => {
            report_error(model, "failed to open file", &path, error);
            return None;
        }
    };
    let id = match model.editor.insert_opened_document(opened) {
        Ok(id) => id,
        Err(error) => {
            report_error(model, "failed to open file", &path, error);
            return None;
        }
    };
    model.status = None;
    let effect = buffer::reveal_buffer(model, id);
    if let Some(position) = position {
        let result = model
            .editor
            .active_editor()
            .and_then(|mut editor| editor.move_cursor_to(position));
        if let Err(error) = result {
            model.status = Some(StatusMessage::error(error.to_string()));
        }
    }
    Some(OpenedFile {
        origin: pending.origin,
        context: pending.context,
        effect,
    })
}

fn report_error(model: &mut Model, action: &str, path: &Path, error: impl std::fmt::Display) {
    model.status = Some(StatusMessage::error(format!(
        "{action} `{}`: {error}",
        path.display()
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startup::initial_model;

    fn complete_open(model: &mut Model, documents: &DocumentFactory, path: &Path) {
        let effect = open_file(documents, model, path);
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        let _ = finish_open_file(model, request.perform());
    }

    fn record_close(model: &mut Model, path: PathBuf) -> Option<Effect> {
        model.status = Some(StatusMessage::info(format!("closed {}", path.display())));
        None
    }

    #[test]
    fn close_uses_only_the_injected_lifecycle_handler() {
        let mut model = Model::default();
        let path = PathBuf::from("/workspace/main.rs");
        assert!(close_file(&mut model, path.clone()).is_none());
        assert!(model.status.is_none());

        model.document_lifecycle.set_on_close(record_close);
        assert!(close_file(&mut model, path).is_none());
        assert_eq!(model.status.unwrap().text, "closed /workspace/main.rs");
    }

    #[test]
    fn non_text_open_reports_error_without_changing_state() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("text.txt");
        let binary = directory.path().join("binary.bin");
        let link = directory.path().join("link.txt");
        std::fs::write(&text, "text").unwrap();
        std::fs::write(&binary, [0xff, 0x00]).unwrap();
        std::os::unix::fs::symlink(&text, &link).unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let documents = DocumentFactory::new();
        let original_entries = model.explorer.entries.clone();

        for path in [binary, link] {
            complete_open(&mut model, &documents, &path);
            assert!(model.editor.is_empty());
            assert_eq!(model.explorer.entries, original_entries);
            assert!(
                model
                    .status
                    .as_ref()
                    .unwrap()
                    .text
                    .contains("not a text file")
            );
        }
    }

    #[test]
    fn open_at_position_moves_the_cursor_after_safe_insertion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "zero\none").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.editor.resize_active_view(80, 10);

        let effect = open_file_at(
            &DocumentFactory::new(),
            &mut model,
            &path,
            Some(DocumentPosition {
                line: 1,
                character: 0,
            }),
        );
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        assert!(finish_open_file(&mut model, request.perform()).is_some());

        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.cursor.unwrap().row, 1);
        assert_eq!(snapshot.cursor.unwrap().column, 0);
    }

    #[test]
    fn open_completion_composes_nested_explorer_reveal() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let path = nested.join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let documents = DocumentFactory::new();

        let effect = open_file(&documents, &mut model, &path);
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        let opened = finish_open_file(&mut model, request.perform()).unwrap();
        let Some(Effect::Feature(request)) = opened.effect else {
            panic!("expected an explorer reveal effect");
        };
        crate::manager::message_manager::MessageManager::default()
            .update(&mut model, request.perform());

        assert_eq!(
            model
                .explorer
                .selected_entry()
                .map(|entry| entry.path.as_path()),
            Some(std::fs::canonicalize(path).unwrap().as_path())
        );
    }

    #[test]
    fn stale_open_completion_does_not_replace_the_latest_request() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let documents = DocumentFactory::new();
        let first_completion = match open_file(&documents, &mut model, &first) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };
        let second_completion = match open_file(&documents, &mut model, &second) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };

        assert!(finish_open_file(&mut model, first_completion).is_none());
        assert!(model.editor.is_empty());
        assert_eq!(
            model.document_open.pending_location(),
            Some(&OpenLocation {
                path: second.clone(),
                position: None,
            })
        );
        assert!(finish_open_file(&mut model, second_completion).is_some());

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(
            model
                .editor
                .buffer_path(model.editor.active_buffer().unwrap()),
            Some(std::fs::canonicalize(second).unwrap().as_path())
        );
    }

    #[test]
    fn same_path_completion_requires_the_latest_request_id() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "zero\none").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.editor.resize_active_view(80, 10);
        let documents = DocumentFactory::new();
        let first = match open_file_at(
            &documents,
            &mut model,
            &path,
            Some(DocumentPosition {
                line: 0,
                character: 0,
            }),
        ) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };
        let second = match open_file_at(
            &documents,
            &mut model,
            &path,
            Some(DocumentPosition {
                line: 1,
                character: 0,
            }),
        ) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };

        assert!(finish_open_file(&mut model, first).is_none());
        assert!(model.editor.is_empty());
        assert_eq!(
            model
                .document_open
                .pending_location()
                .and_then(|location| location.position),
            Some(DocumentPosition {
                line: 1,
                character: 0,
            })
        );
        assert!(finish_open_file(&mut model, second).is_some());
        assert_eq!(
            model.editor.active_snapshot().unwrap().cursor.unwrap().row,
            1
        );
    }
}
