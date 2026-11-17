use std::path::Path;

use editor::{
    DocumentFactory, DocumentOpenCompletion as EditorDocumentOpenCompletion, DocumentOpenRequest,
    DocumentPosition,
};

use crate::{
    buffer,
    data_object::{Context, Model, OpenLocation, StatusMessage},
    manager::effect::Effect,
};

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
        model.context_stack.last().copied(),
    );
    Some(Effect::OpenDocument(OpenFileRequest {
        id,
        request: DocumentOpenRequest::new(documents.clone(), path),
    }))
}

/// Apply the latest document-open completion. Picker/focus reconciliation stays
/// in the shell until the picker command migration.
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
    buffer::reveal_buffer(model, id);
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
