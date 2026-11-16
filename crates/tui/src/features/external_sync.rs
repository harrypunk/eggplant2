use std::path::{Path, PathBuf};

use editor::{DocumentFactory, DocumentOpenRequest, EditorError, OpenedDocument};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalChangeKind {
    Content,
    Structure,
}

/// Coordinate explorer structure updates and clean-buffer reloads for one event.
pub fn file_changed(model: &mut Model, path: &Path, kind: ExternalChangeKind) -> Option<Effect> {
    if is_repository_head_change(path) {
        return crate::vcs::repository_changed(model);
    }
    match kind {
        ExternalChangeKind::Structure => crate::explorer::refresh_changed(model, path),
        ExternalChangeKind::Content => {}
    }
    if !path.exists() {
        return None;
    }
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

fn is_repository_head_change(path: &Path) -> bool {
    let in_git = path
        .components()
        .any(|component| component.as_os_str() == ".git");
    let reference = path
        .file_name()
        .is_some_and(|name| name == "HEAD" || name == "packed-refs")
        || path
            .components()
            .any(|component| component.as_os_str() == "refs" || component.as_os_str() == "logs");
    in_git && reference
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
            Ok(true) => return crate::vcs::refresh_buffer_base(model, self.id),
            Ok(false) => {}
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
        let Some(Effect::Feature(request)) = file_changed(model, path, ExternalChangeKind::Content)
        else {
            return;
        };
        MessageManager::default().update(model, request.perform());
    }

    #[test]
    fn git_head_and_reference_events_are_detected_without_treating_index_as_head() {
        for path in [
            "/repo/.git/HEAD",
            "/repo/.git/refs/heads/main",
            "/repo/.git/logs/HEAD",
        ] {
            assert!(is_repository_head_change(Path::new(path)));
        }
        assert!(!is_repository_head_change(Path::new("/repo/.git/index")));
        assert!(!is_repository_head_change(Path::new("/repo/note.txt")));
    }

    #[test]
    fn structural_events_refresh_explorer_and_preserve_selection() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("a.txt");
        let selected = directory.path().join("b.txt");
        std::fs::write(&first, "a").unwrap();
        std::fs::write(&selected, "b").unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(crate::data_object::CommandId("explorer_next")),
        );

        let created = directory.path().join("c.txt");
        std::fs::write(&created, "c").unwrap();
        manager.update(
            &mut model,
            Message::ExternalFileChanged {
                path: created.clone(),
                kind: ExternalChangeKind::Structure,
            },
        );
        assert!(
            model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.path == created)
        );
        assert_eq!(model.explorer.selected_entry().unwrap().path, selected);

        std::fs::remove_file(&first).unwrap();
        manager.update(
            &mut model,
            Message::ExternalFileChanged {
                path: first.clone(),
                kind: ExternalChangeKind::Structure,
            },
        );
        assert!(
            !model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.path == first)
        );
        assert_eq!(model.explorer.selected_entry().unwrap().path, selected);

        let renamed = directory.path().join("d.txt");
        std::fs::rename(&created, &renamed).unwrap();
        manager.update(
            &mut model,
            Message::ExternalFileChanged {
                path: renamed.clone(),
                kind: ExternalChangeKind::Structure,
            },
        );
        assert!(
            model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.path == renamed)
        );
        assert!(
            !model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.path == created)
        );
        assert_eq!(model.explorer.selected_entry().unwrap().path, selected);
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
            .active_editor()
            .unwrap()
            .move_cursor_to(editor::DocumentPosition {
                line: 0,
                character: 8,
            })
            .unwrap();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text(" local")
            .unwrap();
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
