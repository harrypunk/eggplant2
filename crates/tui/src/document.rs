use std::path::Path;

use editor::{
    BufferId, DocumentFactory, DocumentOpenCompletion, DocumentOpenRequest, DocumentPosition,
};

use crate::{
    buffer,
    data_object::{Model, OpenLocation, StatusMessage},
    manager::effect::Effect,
};

/// Start opening a document through the shared text-only document capability.
pub(crate) fn open_file(
    documents: &DocumentFactory,
    model: &mut Model,
    path: &Path,
) -> Option<Effect> {
    open_file_at(documents, model, path, None)
}

/// Open a document and optionally move its cursor after the normal safe insertion flow.
pub(crate) fn open_file_at(
    documents: &DocumentFactory,
    model: &mut Model,
    path: &Path,
    position: Option<DocumentPosition>,
) -> Option<Effect> {
    model.opening_file = Some(OpenLocation {
        path: path.to_path_buf(),
        position,
    });
    Some(Effect::OpenDocument(DocumentOpenRequest::new(
        documents.clone(),
        path,
    )))
}

/// Apply the latest document-open completion. Picker/focus reconciliation stays
/// in the shell until the picker command migration.
pub(crate) fn finish_open_file(
    model: &mut Model,
    completion: DocumentOpenCompletion,
) -> Option<BufferId> {
    if model
        .opening_file
        .as_ref()
        .map(|location| location.path.as_path())
        != Some(completion.path())
    {
        return None;
    }
    let path = completion.path().to_path_buf();
    let position = model
        .opening_file
        .as_ref()
        .and_then(|location| location.position);
    model.opening_file = None;
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
    if let Some(position) = position
        && let Err(error) = model.editor.move_cursor_to(position)
    {
        model.status = Some(StatusMessage::error(error.to_string()));
    }
    Some(id)
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
            model.opening_file,
            Some(OpenLocation {
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
}
