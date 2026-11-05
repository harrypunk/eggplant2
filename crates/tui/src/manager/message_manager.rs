use std::{fmt::Display, path::Path};

use ratatui::layout::Rect;

use crate::{
    data_object::{
        Context, DeferredAction, FocusTarget, Message, Model, RunningState, StatusMessage,
    },
    explorer::ExplorerEntryKind,
    manager::effect::Effect,
    page::editor::editor_content_area,
};

#[derive(Debug, Default)]
pub struct MessageManager;

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Effect> {
        match msg {
            Message::Quit => return Self::request_quit(model),
            Message::CloseActive => return Self::request_close(model),
            Message::EnterInsertMode => {
                if model.editor.active_buffer().is_some() {
                    model.context_stack.push(Context::EditorInput);
                    model.editor.enter_insert_mode();
                }
            }
            Message::ExitInsertMode => {
                model
                    .context_stack
                    .pop_if(|ctx| ctx == &Context::EditorInput);
                model.editor.enter_normal_mode();
                return Self::auto_save_active(model);
            }
            Message::MoveCursor(movement) => {
                let _ = model.editor.move_cursor(movement);
            }
            Message::InsertChar(character) => {
                if model.editor.insert_text(&character.to_string()).is_ok() {
                    return Self::auto_save_active(model);
                }
            }
            Message::InsertNewline => {
                if model.editor.insert_text("\n").is_ok() {
                    return Self::auto_save_active(model);
                }
            }
            Message::Backspace => {
                if model.editor.backspace().is_ok() {
                    return Self::auto_save_active(model);
                }
            }
            Message::Undo => {
                if model.editor.undo().unwrap_or(false) {
                    return Self::auto_save_active(model);
                }
            }
            Message::Redo => {
                if model.editor.redo().unwrap_or(false) {
                    return Self::auto_save_active(model);
                }
            }
            Message::SaveFinished(completion) => {
                return Self::finish_save(model, completion);
            }
            Message::ExplorerNext => model.explorer.select_next(),
            Message::ExplorerPrevious => model.explorer.select_previous(),
            Message::OpenSelected => Self::open_selected(model),
            Message::ExplorerFold => Self::change_expansion(model, false),
            Message::ExplorerUnfold => Self::change_expansion(model, true),
            Message::RefreshExplorer => Self::refresh_explorer(model),
            Message::ToggleExplorerFocus => match model.current_focus {
                FocusTarget::FileExplorer => Self::focus_editor(model),
                _ => Self::focus_explorer(model),
            },
            Message::Resize(width, height) => {
                let area = editor_content_area(Rect::new(0, 0, width, height));
                model.editor.resize_active_view(area.width, area.height);
            }
            _ => {}
        }
        None
    }

    fn request_quit(model: &mut Model) -> Option<Effect> {
        model.editor.enter_normal_mode();
        if !model.editor.has_modified_buffers() && !model.editor.has_saving_buffers() {
            model.deferred_action = None;
            model.running_state = RunningState::Done;
            return None;
        }

        model.deferred_action = Some(DeferredAction::Quit);
        model.status = Some(StatusMessage::info("Saving before quit"));
        model
            .editor
            .first_dirty_buffer()
            .filter(|&id| !model.editor.is_saving(id))
            .and_then(|id| Self::start_save(model, id))
    }

    fn request_close(model: &mut Model) -> Option<Effect> {
        model.editor.enter_normal_mode();
        let Some(id) = model.editor.active_buffer() else {
            model.status = Some(StatusMessage::info("No file open"));
            return None;
        };

        if model.editor.is_modified(id) == Some(true) || model.editor.is_saving(id) {
            model.deferred_action = Some(DeferredAction::CloseBuffer(id));
            model.status = Some(StatusMessage::info("Saving before close"));
            if model.editor.is_saving(id) {
                return None;
            }
            return Self::start_save(model, id);
        }

        model.deferred_action = None;
        match model.editor.close_buffer(id) {
            Ok(()) => model.status = None,
            Err(error) => model.status = Some(StatusMessage::error(error.to_string())),
        }
        None
    }

    fn auto_save_active(model: &mut Model) -> Option<Effect> {
        if model.editor.mode() == editor::EditMode::Insert {
            return None;
        }
        let id = model.editor.active_buffer()?;
        if model.editor.is_saving(id) {
            return None;
        }
        Self::start_save(model, id)
    }

    fn start_save(model: &mut Model, id: editor::BufferId) -> Option<Effect> {
        match model.editor.save_buffer(id) {
            Ok(request) => Some(Effect::Save(request)),
            Err(error) => {
                model.deferred_action = None;
                model.status = Some(StatusMessage::error(error.to_string()));
                None
            }
        }
    }

    fn finish_save(model: &mut Model, completion: editor::SaveCompletion) -> Option<Effect> {
        let buffer_id = completion.buffer_id();
        if let Err(error) = model.editor.complete_save(completion) {
            model.deferred_action = None;
            model.status = Some(StatusMessage::error(error.to_string()));
            return None;
        }
        model.status = Some(StatusMessage::info("Saved"));

        match model.deferred_action {
            Some(DeferredAction::Quit) => {
                if !model.editor.has_modified_buffers() && !model.editor.has_saving_buffers() {
                    model.deferred_action = None;
                    model.running_state = RunningState::Done;
                    return None;
                }
                model
                    .editor
                    .first_dirty_buffer()
                    .filter(|&id| !model.editor.is_saving(id))
                    .and_then(|id| Self::start_save(model, id))
            }
            Some(DeferredAction::CloseBuffer(id)) => {
                if model.editor.is_saving(id) {
                    return None;
                }
                match model.editor.is_modified(id) {
                    Some(true) => Self::start_save(model, id),
                    Some(false) => {
                        model.deferred_action = None;
                        match model.editor.close_buffer(id) {
                            Ok(()) => model.status = None,
                            Err(error) => {
                                model.status = Some(StatusMessage::error(error.to_string()));
                            }
                        }
                        None
                    }
                    None => {
                        model.deferred_action = None;
                        None
                    }
                }
            }
            None if model.editor.is_modified(buffer_id) == Some(true)
                && !model.editor.is_saving(buffer_id) =>
            {
                Self::start_save(model, buffer_id)
            }
            None => None,
        }
    }

    fn open_selected(model: &mut Model) {
        let Some(entry) = model.explorer.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        match entry.kind {
            ExplorerEntryKind::File => match model.editor.open_file(&path) {
                Ok(_) => {
                    model.status = None;
                    Self::focus_editor(model);
                }
                Err(error) => Self::report_error(model, "failed to open file", &path, error),
            },
            ExplorerEntryKind::Directory => Self::change_expansion(model, true),
            ExplorerEntryKind::Other => {}
        }
    }

    fn change_expansion(model: &mut Model, unfold: bool) {
        let Some(workspace) = &model.workspace else {
            return;
        };
        let Some(entry) = model.explorer.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        let result = if unfold {
            model.explorer.unfold_selected(workspace.root())
        } else {
            model.explorer.fold_selected(workspace.root())
        };
        match result {
            Ok(true) => model.status = None,
            Ok(false) => {}
            Err(error) => {
                Self::report_error(model, "failed to update directory tree", &path, error);
            }
        }
    }

    fn refresh_explorer(model: &mut Model) {
        let Some(workspace) = &model.workspace else {
            return;
        };
        let root = workspace.root().to_path_buf();

        match model.explorer.reload(&root) {
            Ok(()) => model.status = None,
            Err(error) => Self::report_error(model, "failed to refresh", &root, error),
        }
    }

    fn report_error(model: &mut Model, action: &str, path: &Path, error: impl Display) {
        model.status = Some(StatusMessage::error(format!(
            "{action} `{}`: {error}",
            path.display()
        )));
    }

    fn focus_explorer(model: &mut Model) {
        if model.workspace.is_none() {
            return;
        }

        model.current_focus = FocusTarget::FileExplorer;
        if model.context_stack.last() != Some(&Context::Explorer) {
            model.context_stack.push(Context::Explorer);
        }
    }

    fn focus_editor(model: &mut Model) {
        if model.editor.active_buffer().is_none() {
            return;
        }

        model.current_focus = FocusTarget::Editor;
        model
            .context_stack
            .pop_if(|context| context == &Context::Explorer);
    }
}

#[cfg(test)]
mod tests {
    use editor::EditMode;

    use super::*;

    #[test]
    fn mode_messages_update_editor_session_and_context() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.editor.create_buffer("");

        manager.update(&mut model, Message::EnterInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(model.context_stack.last(), Some(&Context::EditorInput));

        manager.update(&mut model, Message::ExitInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn insert_mode_requires_an_active_buffer() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        manager.update(&mut model, Message::EnterInsertMode);

        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn editing_messages_update_text_history_and_viewport() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.editor.create_buffer("a");
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(
            &mut model,
            Message::MoveCursor(editor::CursorMovement::Right),
        );
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('b'));
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ab");

        manager.update(&mut model, Message::Backspace);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "a");
        manager.update(&mut model, Message::InsertChar('c'));
        manager.update(&mut model, Message::ExitInsertMode);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ac");

        manager.update(&mut model, Message::Undo);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "a");
        manager.update(&mut model, Message::Redo);
        assert_eq!(model.editor.active_snapshot().unwrap().lines[0], "ac");
    }

    #[tokio::test]
    async fn dirty_quit_saves_then_exits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "a").unwrap();
        let manager = MessageManager;
        let mut model = Model::default();
        model.editor.open_file(&path).unwrap();
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let Effect::Save(request) = manager.update(&mut model, Message::Quit).unwrap();
        assert_eq!(model.deferred_action, Some(DeferredAction::Quit));
        assert_eq!(model.running_state, RunningState::Running);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert_eq!(model.running_state, RunningState::Done);
    }

    #[tokio::test]
    async fn dirty_close_saves_then_closes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "a").unwrap();
        let manager = MessageManager;
        let mut model = Model::default();
        model.editor.open_file(&path).unwrap();
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let Effect::Save(request) = manager.update(&mut model, Message::CloseActive).unwrap();
        assert_eq!(model.editor.buffer_count(), 1);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert!(model.editor.is_empty());
    }

    #[test]
    fn failed_save_keeps_buffer_dirty() {
        let manager = MessageManager;
        let mut model = Model::default();
        model.editor.create_buffer("scratch");
        model.context_stack.push(Context::AppNormal);
        manager.update(&mut model, Message::EnterInsertMode);

        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager.update(&mut model, Message::ExitInsertMode);
        assert!(effect.is_none());
        assert!(model.editor.active_is_modified());
        assert_eq!(
            model.status.unwrap().kind,
            crate::data_object::StatusKind::Error
        );
    }

    #[tokio::test]
    async fn quit_waits_for_save_completion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        model.editor.open_file(&path).unwrap();
        model.context_stack.push(Context::AppNormal);
        let manager = MessageManager;
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('x'));
        let Effect::Save(request) = manager.update(&mut model, Message::ExitInsertMode).unwrap();

        manager.update(&mut model, Message::Quit);
        assert_eq!(model.running_state, RunningState::Running);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        manager.update(&mut model, Message::Quit);
        assert_eq!(model.running_state, RunningState::Done);
    }

    #[tokio::test]
    async fn auto_save_on_edit_executes_effect_and_applies_completion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.editor.open_file(&path).unwrap();
        for _ in 0..5 {
            model
                .editor
                .move_cursor(editor::CursorMovement::Right)
                .unwrap();
        }
        let manager = MessageManager;
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager.update(&mut model, Message::ExitInsertMode).unwrap();
        let Effect::Save(request) = effect;
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello!");
        assert!(!model.editor.active_is_modified());
        assert_eq!(
            model.status.unwrap().kind,
            crate::data_object::StatusKind::Info
        );
    }

    #[test]
    fn explorer_navigation_opens_files_and_toggles_focus() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        std::fs::write(directory.path().join("b.txt"), "b").unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(&mut model, Message::ExplorerNext);
        assert_eq!(model.explorer.selected, Some(1));
        manager.update(&mut model, Message::OpenSelected);

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.lines[0], "b");
        assert_eq!(
            std::path::Path::new(&snapshot.display_name)
                .file_name()
                .unwrap(),
            "b.txt"
        );

        manager.update(&mut model, Message::ToggleExplorerFocus);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        manager.update(&mut model, Message::OpenSelected);
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }

    #[test]
    fn explorer_unfolds_and_folds_without_changing_workspace_root() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("note.txt"), "hello").unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        let workspace_root = model.workspace.as_ref().unwrap().root().to_path_buf();

        manager.update(&mut model, Message::OpenSelected);
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.explorer.entries[1].depth, 1);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);

        manager.update(&mut model, Message::ExplorerFold);
        assert_eq!(model.explorer.entries.len(), 1);
        manager.update(&mut model, Message::ExplorerUnfold);
        assert_eq!(model.explorer.entries.len(), 2);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);
    }

    #[test]
    fn explorer_refresh_reloads_workspace_tree() {
        let directory = tempfile::tempdir().unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        assert!(model.explorer.entries.is_empty());

        std::fs::write(directory.path().join("new.txt"), "new").unwrap();
        manager.update(&mut model, Message::RefreshExplorer);

        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.explorer.selected, Some(0));
    }

    #[test]
    fn explorer_failure_preserves_the_visible_tree() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        let original_entries = model.explorer.entries.clone();
        std::fs::remove_dir(&child).unwrap();
        std::fs::remove_dir(directory.path()).unwrap();

        manager.update(&mut model, Message::OpenSelected);

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, crate::data_object::StatusKind::Error);
        assert!(status.text.contains("failed to update directory tree"));
        assert_eq!(model.explorer.entries, original_entries);
    }

    #[test]
    fn explorer_cannot_focus_an_empty_editor() {
        let directory = tempfile::tempdir().unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;

        manager.update(&mut model, Message::ToggleExplorerFocus);

        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
    }
}
