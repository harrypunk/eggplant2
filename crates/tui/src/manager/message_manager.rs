use std::{fmt::Display, path::Path};

use ratatui::layout::Rect;

use crate::{
    data_object::{Context, FocusTarget, Message, Model, RunningState, StatusMessage},
    explorer::ExplorerEntryKind,
    page::editor::editor_content_area,
};

#[derive(Debug, Default)]
pub struct MessageManager;

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Message> {
        match msg {
            Message::Quit => {
                model.running_state = RunningState::Done;
            }
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
            }
            Message::MoveCursor(movement) => {
                let _ = model.editor.move_cursor(movement);
            }
            Message::InsertChar(character) => {
                let _ = model.editor.insert_text(&character.to_string());
            }
            Message::InsertNewline => {
                let _ = model.editor.insert_text("\n");
            }
            Message::Backspace => {
                let _ = model.editor.backspace();
            }
            Message::Undo => {
                let _ = model.editor.undo();
            }
            Message::Redo => {
                let _ = model.editor.redo();
            }
            Message::ExplorerNext => model.explorer.select_next(),
            Message::ExplorerPrevious => model.explorer.select_previous(),
            Message::OpenSelected => Self::open_selected(model),
            Message::ExplorerParent => Self::open_parent(model),
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
            ExplorerEntryKind::Directory => match crate::explorer::ExplorerState::load(&path) {
                Ok(explorer) => {
                    model.explorer = explorer;
                    model.status = None;
                }
                Err(error) => Self::report_error(model, "failed to open directory", &path, error),
            },
            ExplorerEntryKind::Other => {}
        }
    }

    fn open_parent(model: &mut Model) {
        let Some(workspace) = &model.workspace else {
            return;
        };
        let directory = model.explorer.current_dir.clone();
        match model.explorer.open_parent(workspace.root()) {
            Ok(true) => model.status = None,
            Ok(false) => {}
            Err(error) => Self::report_error(model, "failed to open parent", &directory, error),
        }
    }

    fn refresh_explorer(model: &mut Model) {
        if model.workspace.is_none() {
            return;
        }

        let directory = model.explorer.current_dir.clone();
        match model.explorer.reload() {
            Ok(()) => model.status = None,
            Err(error) => Self::report_error(model, "failed to refresh", &directory, error),
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
    fn explorer_enters_directories_and_returns_to_workspace_root() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        std::fs::write(child.join("note.txt"), "hello").unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        let workspace_root = model.workspace.as_ref().unwrap().root().to_path_buf();

        manager.update(&mut model, Message::OpenSelected);
        assert_eq!(model.explorer.current_dir, child);
        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.workspace.as_ref().unwrap().root(), workspace_root);

        manager.update(&mut model, Message::ExplorerParent);
        assert_eq!(model.explorer.current_dir, workspace_root);
        manager.update(&mut model, Message::ExplorerParent);
        assert_eq!(
            model.explorer.current_dir,
            model.workspace.as_ref().unwrap().root()
        );
    }

    #[test]
    fn explorer_refresh_reloads_current_directory() {
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
    fn explorer_failure_sets_status_without_changing_directory() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager;
        let original_dir = model.explorer.current_dir.clone();
        std::fs::remove_dir(&child).unwrap();

        manager.update(&mut model, Message::OpenSelected);

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, crate::data_object::StatusKind::Error);
        assert!(status.text.contains("failed to open directory"));
        assert_eq!(model.explorer.current_dir, original_dir);
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
