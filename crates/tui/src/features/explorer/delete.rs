use std::path::{Path, PathBuf};

use ratatui::{Frame, layout::Size, text::Line, widgets::Paragraph};

use crate::{
    component::popup::Popup,
    data_object::{Context, Message, Model, StatusMessage},
    filesystem::{self, FileKind as DeleteKind},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{ExplorerEntry, ExplorerEntryKind, buffers};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum DeletePhase {
    #[default]
    Closed,
    Confirming,
    Deleting,
}

#[derive(Debug, Default)]
pub(crate) struct DeleteState {
    phase: DeletePhase,
    target: Option<DeleteTarget>,
    revision: u64,
}

impl DeleteState {
    fn is_open(&self) -> bool {
        self.phase != DeletePhase::Closed
    }

    fn is_confirming(&self) -> bool {
        self.phase == DeletePhase::Confirming
    }

    fn is_deleting(&self) -> bool {
        self.phase == DeletePhase::Deleting
    }

    fn target(&self) -> Option<&DeleteTarget> {
        self.target.as_ref()
    }

    fn open(&mut self, target: DeleteTarget) {
        self.phase = DeletePhase::Confirming;
        self.target = Some(target);
    }

    fn close(&mut self) {
        self.phase = DeletePhase::Closed;
        self.target = None;
        self.revision = self.revision.wrapping_add(1);
    }

    fn begin_delete(&mut self) -> Option<u64> {
        if !self.is_confirming() {
            return None;
        }
        self.phase = DeletePhase::Deleting;
        self.revision = self.revision.wrapping_add(1);
        Some(self.revision)
    }

    fn accepts_completion(&self, revision: u64) -> bool {
        self.is_deleting() && self.revision == revision
    }

    fn fail(&mut self, revision: u64) -> bool {
        if !self.accepts_completion(revision) {
            return false;
        }
        self.phase = DeletePhase::Confirming;
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DeleteTarget {
    path: PathBuf,
    relative: PathBuf,
    kind: DeleteKind,
}

impl DeleteTarget {
    fn new(root: &Path, entry: &ExplorerEntry) -> Result<Self, String> {
        let relative = entry
            .path
            .strip_prefix(root)
            .map_err(|_| "The selected path is outside the workspace".to_string())?
            .to_path_buf();
        if relative.as_os_str().is_empty() {
            return Err("The workspace root cannot be deleted".to_string());
        }
        let kind = match entry.kind {
            ExplorerEntryKind::File => DeleteKind::File,
            ExplorerEntryKind::Directory => DeleteKind::Directory,
            ExplorerEntryKind::Link => DeleteKind::Link,
            ExplorerEntryKind::Other => {
                return Err("The selected file type cannot be deleted".to_string());
            }
        };
        Ok(Self {
            path: entry.path.clone(),
            relative,
            kind,
        })
    }
}

fn notify(model: &mut Model, status: StatusMessage) -> Option<Effect> {
    Some(crate::notifications::report_status(model, status))
}

pub(super) fn open(model: &mut Model) -> Option<Effect> {
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        return notify(model, StatusMessage::info("No folder open"));
    };
    if model.explorer.delete.is_open() || !model.modal.open(Context::ExplorerDelete) {
        return None;
    }
    let Some(entry) = model.explorer.selected_entry() else {
        model.modal.close(Context::ExplorerDelete);
        return notify(model, StatusMessage::info("No file or folder selected"));
    };
    let target = match DeleteTarget::new(&root, entry) {
        Ok(target) => target,
        Err(error) => {
            model.modal.close(Context::ExplorerDelete);
            return notify(model, StatusMessage::error(error));
        }
    };

    model.explorer.delete.open(target);
    model.context_stack.push(Context::ExplorerDelete);
    None
}

pub(super) fn cancel(model: &mut Model) -> Option<Effect> {
    if model.explorer.delete.is_deleting() {
        return None;
    }
    close_prompt(model);
    None
}

pub(super) fn confirm(model: &mut Model) -> Option<Effect> {
    if !model.explorer.delete.is_confirming() {
        return None;
    }
    let Some(root) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
    else {
        return notify(model, StatusMessage::error("No folder open"));
    };
    let target = model.explorer.delete.target()?.clone();
    if let Some(effect) = buffers::reject_blocked(model, &target.path, target.kind, "deleting") {
        return Some(effect);
    }
    let revision = model.explorer.delete.begin_delete()?;
    Some(Effect::feature(DeleteRequest {
        root,
        target,
        revision,
    }))
}

fn close_prompt(model: &mut Model) {
    model.explorer.delete.close();
    model.modal.close(Context::ExplorerDelete);
    model
        .context_stack
        .pop_if(|context| context == &Context::ExplorerDelete);
}

struct DeleteRequest {
    root: PathBuf,
    target: DeleteTarget,
    revision: u64,
}

impl FeatureEffect for DeleteRequest {
    fn perform(self: Box<Self>) -> Message {
        let relative = self.target.relative.clone();
        let result = filesystem::delete(&self.root, &self.target.path, self.target.kind)
            .map_err(|error| format!("failed to delete `{}`: {error}", relative.display()));
        Message::FeatureCompletion(FeatureCompletion::new(DeleteCompletion {
            revision: self.revision,
            kind: self.target.kind,
            result,
        }))
    }
}

struct DeleteCompletion {
    revision: u64,
    kind: DeleteKind,
    result: Result<PathBuf, String>,
}

impl Completion for DeleteCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match self.result {
            Ok(path) if model.explorer.delete.accepts_completion(self.revision) => {
                close_prompt(model);
                let close = buffers::close(model, &path, self.kind);
                let refresh = super::refresh_changed(model, &path);
                Effect::combine(refresh, close)
            }
            Ok(_) => None,
            Err(error) => {
                if !model.explorer.delete.fail(self.revision) {
                    return None;
                }
                notify(model, StatusMessage::error(error))
            }
        }
    }
}

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.explorer.delete.is_open().then(|| {
        OverlayContribution::new(
            "explorer_delete",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(64, 6)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
    let state = &model.explorer.delete;
    let styles = model.theme.ui();
    let surface = styles.popup_surface();
    let lines = if state.is_deleting() {
        vec![Line::styled("Deleting…", surface.patch(styles.muted))]
    } else {
        let Some(target) = state.target() else {
            return;
        };
        let question = match target.kind {
            DeleteKind::Directory => format!(
                "Delete directory `{}` recursively?",
                target.relative.display()
            ),
            DeleteKind::File => format!("Delete file `{}`?", target.relative.display()),
            DeleteKind::Link => format!("Delete link `{}`?", target.relative.display()),
        };
        vec![
            Line::raw(question),
            Line::styled("y confirm   Esc cancel", surface.patch(styles.muted)),
        ]
    };
    let content = Paragraph::new(lines).style(surface);
    frame.render_widget(Popup::new("Delete", content, styles), area);
}

#[cfg(test)]
mod tests {
    use std::fs;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        data_object::CommandId, manager::message_manager::MessageManager, startup::initial_model,
    };

    fn complete_feature(
        manager: &MessageManager,
        model: &mut Model,
        effect: Effect,
    ) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected a filesystem effect");
        };
        manager.update(model, request.perform())
    }

    fn apply_features(manager: &MessageManager, model: &mut Model, effect: Effect) {
        match effect {
            Effect::Feature(request) => {
                manager.update(model, request.perform());
            }
            Effect::Batch(effects) => {
                for effect in effects {
                    apply_features(manager, model, effect);
                }
            }
            _ => {}
        }
    }

    fn open_delete(manager: &MessageManager, model: &mut Model) {
        assert!(
            manager
                .update(model, Message::InvokeCommand(CommandId("explorer_delete")),)
                .is_none()
        );
        assert!(model.explorer.delete.is_confirming());
        assert_eq!(model.context_stack.last(), Some(&Context::ExplorerDelete));
    }

    #[test]
    fn cancellation_closes_the_prompt_without_deleting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        fs::write(&path, "keep").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_explorer_delete")),
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "keep");
        assert!(!model.explorer.delete.is_open());
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        assert!(overlay(&model).is_none());
    }

    #[test]
    fn confirmed_file_deletion_refreshes_the_explorer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        fs::write(&path, "delete").unwrap();
        fs::write(directory.path().join("other.txt"), "keep").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = crate::test_utils::open_file(&mut model, &path);
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_delete_confirm")),
            )
            .unwrap();
        assert!(path.exists());
        let refresh = complete_feature(&manager, &mut model, effect).unwrap();
        assert!(!path.exists());
        assert!(!model.explorer.delete.is_open());
        apply_features(&manager, &mut model, refresh);
        assert!(
            model
                .explorer
                .entries
                .iter()
                .all(|entry| entry.path != path)
        );
        assert_eq!(model.editor.buffer_path(buffer), None);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
    }

    #[test]
    fn confirmed_directory_deletion_is_recursive() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("folder");
        fs::create_dir_all(folder.join("nested")).unwrap();
        let nested_file = folder.join("nested/note.txt");
        fs::write(&nested_file, "delete").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = crate::test_utils::open_file(&mut model, &nested_file);
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_delete_confirm")),
            )
            .unwrap();
        let refresh = complete_feature(&manager, &mut model, effect).unwrap();
        assert!(!folder.exists());
        apply_features(&manager, &mut model, refresh);
        assert!(
            model
                .explorer
                .entries
                .iter()
                .all(|entry| entry.path != folder)
        );
        assert_eq!(model.editor.buffer_path(buffer), None);
    }

    #[test]
    fn modified_open_buffer_blocks_deletion_without_losing_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        fs::write(&path, "original").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = crate::test_utils::open_file(&mut model, &path);
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("changed ")
            .unwrap();
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);

        let notification = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_delete_confirm")),
        );
        assert!(notification.is_some());
        assert!(path.exists());
        assert_eq!(model.editor.buffer_path(buffer), Some(path.as_path()));
        assert_eq!(
            model.editor.active_text_snapshot().unwrap().text,
            "changed original"
        );
        assert!(model.explorer.delete.is_confirming());
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Save modified buffers and wait for saves before deleting"
        );
    }

    #[test]
    fn deletion_failure_notifies_and_retains_the_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        fs::write(&path, "delete").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);
        fs::remove_file(&path).unwrap();

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("explorer_delete_confirm")),
            )
            .unwrap();
        let notification = complete_feature(&manager, &mut model, effect);
        assert!(notification.is_some());
        assert!(model.explorer.delete.is_confirming());
        assert_eq!(model.explorer.delete.target().unwrap().path, path);
        assert_eq!(model.context_stack.last(), Some(&Context::ExplorerDelete));
        assert!(
            model
                .notifications
                .current()
                .unwrap()
                .message()
                .contains("failed to delete `note.txt`")
        );
    }

    #[test]
    fn directory_prompt_explains_recursive_deletion_and_confirmation() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("folder")).unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        open_delete(&manager, &mut model);
        let contribution = overlay(&model).unwrap();
        let backend = TestBackend::new(70, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| (contribution.render)(&model, frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Delete directory `folder` recursively?"));
        assert!(content.contains("y confirm"));
        assert!(content.contains("Esc cancel"));
    }
}
