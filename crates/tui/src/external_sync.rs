use std::path::{Path, PathBuf};

use editor::{DocumentFactory, DocumentOpenRequest, EditorError, OpenedDocument};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

/// Schedule a background read for a changed open file without touching local edits/saves.
pub fn file_changed(model: &mut Model, path: &Path) -> Option<Effect> {
    let id = model.editor.buffer_by_path(path)?;
    if model.editor.is_saving(id) {
        return None;
    }
    if model.editor.is_modified(id) == Some(true) {
        report_conflict(model, path);
        return None;
    }
    let expected_version = model.editor.buffer_version(id)?;
    Some(Effect::feature(ExternalReloadRequest {
        id,
        expected_version,
        path: path.to_path_buf(),
        request: DocumentOpenRequest::new(DocumentFactory::new(), path),
    }))
}

struct ExternalReloadRequest {
    id: editor::BufferId,
    expected_version: i32,
    path: PathBuf,
    request: DocumentOpenRequest,
}

impl FeatureEffect for ExternalReloadRequest {
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(ExternalReloadCompletion {
            id: self.id,
            expected_version: self.expected_version,
            path: self.path,
            result: self.request.perform().into_result(),
        }))
    }
}

struct ExternalReloadCompletion {
    id: editor::BufferId,
    expected_version: i32,
    path: PathBuf,
    result: Result<OpenedDocument, String>,
}

impl Completion for ExternalReloadCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match model
            .editor
            .complete_external_reload(self.id, self.expected_version, self.result)
        {
            Ok(_) => {}
            Err(EditorError::BufferModified(_)) => report_conflict(model, &self.path),
            Err(EditorError::SaveInProgress(_)) => {}
            Err(error) => model.status = Some(StatusMessage::error(error.to_string())),
        }
        None
    }
}

fn report_conflict(model: &mut Model, path: &Path) {
    model.status = Some(StatusMessage::error(format!(
        "{} changed on disk; local changes preserved",
        path.display()
    )));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{Context, StatusKind},
        manager::message_manager::MessageManager,
        test_utils::open_file,
    };

    fn change_file(model: &mut Model, path: &Path) {
        let Some(Effect::Feature(request)) = file_changed(model, path) else {
            return;
        };
        MessageManager::default().update(model, request.perform());
    }

    #[test]
    fn clean_buffers_reload_and_dirty_buffers_report_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "original").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        let id = open_file(&mut model, &path);
        model.editor.resize_active_view(80, 10);
        std::fs::write(&path, "external").unwrap();

        change_file(&mut model, &path);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "external"
        );

        model.editor.enter_insert_mode();
        model
            .editor
            .move_cursor_to(editor::DocumentPosition {
                line: 0,
                character: 8,
            })
            .unwrap();
        model.editor.insert_text(" local").unwrap();
        model.editor.enter_normal_mode();
        std::fs::write(&path, "new external").unwrap();
        change_file(&mut model, &path);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "external local"
        );
        assert_eq!(model.status.unwrap().kind, StatusKind::Error);
        assert_eq!(model.editor.buffer_by_path(&path), Some(id));
    }
}
