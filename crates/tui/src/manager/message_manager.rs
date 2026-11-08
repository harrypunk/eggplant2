use std::{fmt::Display, path::Path};

use editor::{BufferId, DocumentFactory, EditMode, SaveCompletion};
use ratatui::layout::Rect;

use crate::{
    command_menu::{CommandAction, CommandMenuOutcome},
    data_object::{
        BufferMovement, Context, DeferredAction, FocusDirection, FocusTarget, Message, Model,
        RunningState, StatusMessage, Visibility,
    },
    explorer::{ExpansionState, ExplorerEntryKind, IgnoreMode},
    manager::effect::Effect,
    page::editor::editor_content_area,
};

#[derive(Debug, Default)]
pub struct MessageManager {
    documents: DocumentFactory,
}

impl MessageManager {
    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Effect> {
        match msg {
            Message::CommandMenuKey(key) => {
                return match model.command_menu.handle_key(key) {
                    CommandMenuOutcome::Pending | CommandMenuOutcome::Cancelled => None,
                    CommandMenuOutcome::Action(action) => {
                        let message = match action {
                            CommandAction::Quit => Message::Quit,
                            CommandAction::NextBuffer => Message::NextBuffer,
                            CommandAction::PreviousBuffer => Message::PreviousBuffer,
                            CommandAction::ToggleFileExplorer => Message::ToggleFileExplorer,
                            CommandAction::OpenFilePicker => Message::OpenFilePicker,
                        };
                        self.update(model, message)
                    }
                };
            }
            Message::Quit => return Self::request_quit(model),
            Message::CloseActive => return Self::request_close(model),
            Message::NextBuffer => {
                return Self::change_buffer(model, BufferMovement::Next);
            }
            Message::PreviousBuffer => {
                return Self::change_buffer(model, BufferMovement::Previous);
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
                return Self::auto_save_active(model);
            }
            Message::MoveCursor(movement) => {
                let _ = model.editor.move_cursor(movement);
            }
            Message::MoveFocus(direction) => Self::move_focus(model, direction),
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
            Message::OpenSelected => self.open_selected(model),
            Message::ExplorerFold => Self::change_expansion(model, ExpansionState::Collapsed),
            Message::ExplorerUnfold => Self::change_expansion(model, ExpansionState::Expanded),
            Message::RefreshExplorer => Self::refresh_explorer(model),
            Message::ToggleIgnoredFiles => Self::toggle_ignored_files(model),
            Message::ToggleFileExplorer => {
                Self::toggle_file_explorer(model);
            }
            Message::OpenFilePicker => return Self::open_file_picker(model),
            Message::FilePickerInsert(character) => {
                return model.file_picker.insert(character).map(Effect::FilePicker);
            }
            Message::FilePickerBackspace => {
                return model.file_picker.backspace().map(Effect::FilePicker);
            }
            Message::CloseFilePicker => Self::close_file_picker(model),
            Message::FilePickerNext => {
                return model.file_picker.select_next().map(Effect::FilePicker);
            }
            Message::FilePickerPrevious => {
                return model.file_picker.select_previous().map(Effect::FilePicker);
            }
            Message::OpenPickerFile => self.open_picker_file(model),
            Message::FileScanFinished { revision, result } => {
                return model
                    .file_picker
                    .complete_scan(revision, result)
                    .map(Effect::FilePicker);
            }
            Message::FileFilterFinished { revision, results } => {
                return model
                    .file_picker
                    .complete_filter(revision, results)
                    .map(Effect::FilePicker);
            }
            Message::FilePreviewFinished { revision, preview } => {
                model.file_picker.complete_preview(revision, preview);
            }
            Message::FocusLost => return Self::save_active(model),
            Message::Resize(width, height) => {
                let area = editor_content_area(
                    Rect::new(0, 0, width, height),
                    model.explorer_visibility,
                    model.tools.visibility,
                );
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

    fn change_buffer(model: &mut Model, movement: BufferMovement) -> Option<Effect> {
        let effect = Self::auto_save_active(model);
        match movement {
            BufferMovement::Next => {
                let _ = model.editor.activate_next_buffer();
            }
            BufferMovement::Previous => {
                let _ = model.editor.activate_previous_buffer();
            }
        }
        effect
    }

    fn auto_save_active(model: &mut Model) -> Option<Effect> {
        if model.editor.mode() == EditMode::Insert {
            return None;
        }
        Self::save_active(model)
    }

    fn save_active(model: &mut Model) -> Option<Effect> {
        let id = model.editor.active_buffer()?;
        if model.editor.is_saving(id) {
            return None;
        }
        Self::start_save(model, id)
    }

    fn start_save(model: &mut Model, id: BufferId) -> Option<Effect> {
        match model.editor.save_buffer(id) {
            Ok(request) => Some(Effect::Save(request)),
            Err(error) => {
                model.deferred_action = None;
                model.status = Some(StatusMessage::error(error.to_string()));
                None
            }
        }
    }

    fn finish_save(model: &mut Model, completion: SaveCompletion) -> Option<Effect> {
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
            None if model.editor.mode() != EditMode::Insert
                && model.editor.is_modified(buffer_id) == Some(true)
                && !model.editor.is_saving(buffer_id) =>
            {
                Self::start_save(model, buffer_id)
            }
            None => None,
        }
    }

    fn open_selected(&self, model: &mut Model) {
        let Some(entry) = model.explorer.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        match entry.kind {
            ExplorerEntryKind::File | ExplorerEntryKind::Link | ExplorerEntryKind::Other => {
                self.open_file(model, &path);
            }
            ExplorerEntryKind::Directory => Self::change_expansion(model, ExpansionState::Expanded),
        }
    }

    fn open_file_picker(model: &mut Model) -> Option<Effect> {
        let Some(workspace) = &model.workspace else {
            model.status = Some(StatusMessage::info("No folder open"));
            return None;
        };
        let root = workspace.root().to_path_buf();
        if model.context_stack.last() != Some(&Context::FilePicker) {
            model.context_stack.push(Context::FilePicker);
        }
        model.current_focus = FocusTarget::FilePicker;
        Some(Effect::FilePicker(model.file_picker.open(&root)))
    }

    fn close_file_picker(model: &mut Model) {
        model.file_picker.close();
        model
            .context_stack
            .pop_if(|context| context == &Context::FilePicker);
        model.current_focus = match model.context_stack.last() {
            Some(Context::Explorer) => FocusTarget::FileExplorer,
            _ => FocusTarget::Editor,
        };
    }

    fn open_picker_file(&self, model: &mut Model) {
        let Some(entry) = model.file_picker.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        self.open_file(model, &path);
    }

    /// Single document-open path shared by explorer, picker, and future commands.
    fn open_file(&self, model: &mut Model, path: &Path) {
        let document = match self.documents.open(path) {
            Ok(document) => document,
            Err(error) => {
                Self::report_error(model, "failed to open file", path, error);
                return;
            }
        };
        let id = model.editor.insert_document(document);
        let opened_path = model.editor.buffer_path(id).map(Path::to_path_buf);
        model.status = None;

        if let (Some(workspace), Some(path)) = (&model.workspace, opened_path) {
            let root = workspace.root().to_path_buf();
            if let Err(error) = model.explorer.reveal(&root, &path) {
                Self::report_error(model, "failed to update file explorer", &path, error);
            }
        }
        match model.file_picker.visibility {
            Visibility::Displayed => Self::close_file_picker(model),
            Visibility::Hidden => {}
        }
        Self::focus_editor(model);
    }

    fn change_expansion(model: &mut Model, target: ExpansionState) {
        let Some(workspace) = &model.workspace else {
            return;
        };
        let Some(entry) = model.explorer.selected_entry() else {
            return;
        };
        let path = entry.path.clone();
        let result = match target {
            ExpansionState::Expanded => model.explorer.unfold_selected(workspace.root()),
            ExpansionState::Collapsed => model.explorer.fold_selected(workspace.root()),
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

    fn toggle_ignored_files(model: &mut Model) {
        let Some(workspace) = &model.workspace else {
            return;
        };
        let root = workspace.root().to_path_buf();

        match model.explorer.toggle_ignore_mode(&root) {
            Ok(mode) => {
                let text = match mode {
                    IgnoreMode::HideIgnored => "Hiding ignored files",
                    IgnoreMode::ShowIgnored => "Showing ignored files",
                };
                model.status = Some(StatusMessage::info(text));
            }
            Err(error) => Self::report_error(model, "failed to toggle ignored files", &root, error),
        }
    }

    fn report_error(model: &mut Model, action: &str, path: &Path, error: impl Display) {
        model.status = Some(StatusMessage::error(format!(
            "{action} `{}`: {error}",
            path.display()
        )));
    }

    // Only explorer focus needs to move when the panel is hidden. Other current
    // and future focus targets intentionally remain unchanged.
    #[allow(clippy::single_match)]
    fn toggle_file_explorer(model: &mut Model) {
        match model.explorer_visibility {
            Visibility::Hidden => match model.workspace {
                None => model.status = Some(StatusMessage::info("No folder open")),
                Some(_) => {
                    model.explorer_visibility = Visibility::Displayed;
                    Self::focus_explorer(model);
                }
            },
            Visibility::Displayed => {
                model.explorer_visibility = Visibility::Hidden;
                match model.current_focus {
                    FocusTarget::FileExplorer => {
                        model
                            .context_stack
                            .pop_if(|context| context == &Context::Explorer);
                        model.current_focus = FocusTarget::Editor;
                    }
                    _ => {}
                }
            }
        }
    }

    // The single-view layout only needs this explicit horizontal relationship.
    // When splits arrive, query the logical layout tree instead: start at the
    // focused pane's stable ID, walk toward `direction` through visible
    // siblings, and select the nearest focusable leaf. Keep terminal cursor
    // coordinates and Ratatui rectangles out of this application-state policy.
    fn move_focus(model: &mut Model, direction: FocusDirection) {
        match (model.current_focus, direction, model.explorer_visibility) {
            (FocusTarget::Editor, FocusDirection::Left, Visibility::Displayed) => {
                Self::focus_explorer(model);
            }
            (FocusTarget::FileExplorer, FocusDirection::Right, _) => {
                Self::focus_editor(model);
            }
            _ => {}
        }
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
    use editor::{CursorMovement, EditMode, SaveRequest};

    use super::*;
    use crate::{
        data_object::StatusKind,
        file_picker::PreviewState,
        startup::initial_model,
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[test]
    fn mode_messages_update_editor_session_and_context() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "");

        manager.update(&mut model, Message::EnterInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(model.context_stack.last(), Some(&Context::EditorInput));

        manager.update(&mut model, Message::ExitInsertMode);
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn insert_mode_requires_an_active_buffer() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        manager.update(&mut model, Message::EnterInsertMode);

        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn editing_messages_update_text_history_and_viewport() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "a");
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(&mut model, Message::MoveCursor(CursorMovement::Right));
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
        let manager = MessageManager::default();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let request = expect_save(manager.update(&mut model, Message::Quit));
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
        let manager = MessageManager::default();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("b").unwrap();
        model.editor.enter_normal_mode();

        let request = expect_save(manager.update(&mut model, Message::CloseActive));
        assert_eq!(model.editor.buffer_count(), 1);
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert!(model.editor.is_empty());
    }

    #[test]
    fn failed_save_keeps_buffer_dirty() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        create_buffer(&mut model, "scratch");
        model.context_stack.push(Context::AppNormal);
        manager.update(&mut model, Message::EnterInsertMode);

        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager.update(&mut model, Message::ExitInsertMode);
        assert!(effect.is_none());
        assert!(model.editor.active_is_modified());
        assert_eq!(model.status.unwrap().kind, StatusKind::Error);
    }

    #[tokio::test]
    async fn quit_waits_for_save_completion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        open_file(&mut model, &path);
        model.context_stack.push(Context::AppNormal);
        let manager = MessageManager::default();
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('x'));
        let request = expect_save(manager.update(&mut model, Message::ExitInsertMode));

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
        open_file(&mut model, &path);
        for _ in 0..5 {
            model.editor.move_cursor(CursorMovement::Right).unwrap();
        }
        let manager = MessageManager::default();
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager.update(&mut model, Message::ExitInsertMode).unwrap();
        let request = expect_save(Some(effect));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello!");
        assert!(!model.editor.active_is_modified());
        assert_eq!(model.status.unwrap().kind, StatusKind::Info);
    }

    #[tokio::test]
    async fn focus_loss_saves_without_leaving_insert_mode() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        manager.update(&mut model, Message::EnterInsertMode);
        manager.update(&mut model, Message::InsertChar('!'));

        let request = expect_save(manager.update(&mut model, Message::FocusLost));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "!hello");
    }

    #[tokio::test]
    async fn switching_buffers_saves_the_outgoing_dirty_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = Model::default();
        let first = open_file(&mut model, &first_path);
        model.editor.enter_insert_mode();
        model.editor.insert_text("!").unwrap();
        model.editor.enter_normal_mode();
        let second = open_file(&mut model, &second_path);
        model.editor.activate_buffer(first).unwrap();
        let manager = MessageManager::default();

        let request = expect_save(manager.update(&mut model, Message::NextBuffer));
        assert_eq!(model.editor.active_buffer(), Some(second));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(first_path).unwrap(), "!first");
        assert!(!model.editor.is_modified(first).unwrap());
    }

    #[test]
    fn file_picker_scans_previews_and_opens_selected_file() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        let file = nested.join("main.rs");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(&file, "fn main() {}").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(120, 20));
        let mut effect = None;
        for key in [' ', 'f', 'f'] {
            effect = manager.update(
                &mut model,
                Message::CommandMenuKey(crossterm::event::KeyCode::Char(key)),
            );
        }
        let Some(Effect::FilePicker(request)) = effect else {
            panic!("Space f f should start a file scan");
        };
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(model.context_stack.last(), Some(&Context::FilePicker));

        let Some(Effect::FilePicker(request)) = manager.update(&mut model, request.perform())
        else {
            panic!("scan completion should start filtering");
        };
        let Some(Effect::FilePicker(request)) = manager.update(&mut model, request.perform())
        else {
            panic!("filter completion should start a preview");
        };
        manager.update(&mut model, request.perform());
        assert_eq!(model.file_picker.results.len(), 1);
        assert_eq!(
            model.file_picker.preview,
            PreviewState::Text(vec!["fn main() {}".to_string()])
        );

        manager.update(&mut model, Message::OpenPickerFile);
        assert_eq!(model.file_picker.visibility, Visibility::Hidden);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model.explorer.selected_entry().unwrap().path,
            std::fs::canonicalize(file).unwrap()
        );
        assert_eq!(
            model.explorer.entries[0].expansion,
            ExpansionState::Expanded
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().lines[0],
            "fn main() {}"
        );
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
        let manager = MessageManager::default();
        let original_entries = model.explorer.entries.clone();

        for path in [binary, link] {
            manager.open_file(&mut model, &path);
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
    fn explorer_navigation_opens_files_and_toggles_focus() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        std::fs::write(directory.path().join("b.txt"), "b").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
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

        manager.update(&mut model, Message::ToggleFileExplorer);
        assert_eq!(model.explorer_visibility, Visibility::Hidden);
        manager.update(&mut model, Message::ToggleFileExplorer);
        assert_eq!(model.explorer_visibility, Visibility::Displayed);
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
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
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
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        assert!(model.explorer.entries.is_empty());

        std::fs::write(directory.path().join("new.txt"), "new").unwrap();
        manager.update(&mut model, Message::RefreshExplorer);

        assert_eq!(model.explorer.entries.len(), 1);
        assert_eq!(model.explorer.selected, Some(0));
    }

    #[test]
    fn explorer_toggles_git_ignored_entries() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(directory.path().join("ignored.txt"), "ignored").unwrap();
        std::fs::write(directory.path().join("visible.txt"), "visible").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(&mut model, Message::ToggleIgnoredFiles);
        assert!(
            model
                .explorer
                .entries
                .iter()
                .any(|entry| entry.name == "ignored.txt")
        );
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Showing ignored files")
        );

        manager.update(&mut model, Message::ToggleIgnoredFiles);
        assert!(
            model
                .explorer
                .entries
                .iter()
                .all(|entry| entry.name != "ignored.txt")
        );
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Hiding ignored files")
        );
    }

    #[test]
    fn explorer_failure_preserves_the_visible_tree() {
        let directory = tempfile::tempdir().unwrap();
        let child = directory.path().join("folder");
        std::fs::create_dir(&child).unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let original_entries = model.explorer.entries.clone();
        std::fs::remove_dir(&child).unwrap();
        std::fs::remove_dir(directory.path()).unwrap();

        manager.update(&mut model, Message::OpenSelected);

        let status = model.status.as_ref().unwrap();
        assert_eq!(status.kind, StatusKind::Error);
        assert!(status.text.contains("failed to update directory tree"));
        assert_eq!(model.explorer.entries, original_entries);
    }

    #[test]
    fn hiding_and_showing_explorer_updates_focus() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(&mut model, Message::ToggleFileExplorer);
        assert_eq!(model.explorer_visibility, Visibility::Hidden);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(&mut model, Message::ToggleFileExplorer);
        assert_eq!(model.explorer_visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
    }

    #[test]
    fn directional_focus_moves_between_editor_and_explorer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        create_buffer(&mut model, "buffer");
        let manager = MessageManager::default();

        manager.update(&mut model, Message::MoveFocus(FocusDirection::Right));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(&mut model, Message::MoveFocus(FocusDirection::Left));
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));

        for direction in [FocusDirection::Up, FocusDirection::Down] {
            manager.update(&mut model, Message::MoveFocus(direction));
            assert_eq!(model.current_focus, FocusTarget::FileExplorer);
            assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        }
    }

    #[test]
    fn directional_focus_ignores_unavailable_targets() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        manager.update(&mut model, Message::MoveFocus(FocusDirection::Right));
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));

        create_buffer(&mut model, "buffer");
        manager.update(&mut model, Message::MoveFocus(FocusDirection::Right));
        manager.update(&mut model, Message::ToggleFileExplorer);
        manager.update(&mut model, Message::MoveFocus(FocusDirection::Left));
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        let mut without_workspace = Model::default();
        without_workspace.context_stack.push(Context::AppNormal);
        create_buffer(&mut without_workspace, "buffer");
        manager.update(
            &mut without_workspace,
            Message::MoveFocus(FocusDirection::Left),
        );
        assert_eq!(without_workspace.current_focus, FocusTarget::Editor);
        assert_eq!(
            without_workspace.context_stack.last(),
            Some(&Context::AppNormal)
        );
    }

    #[test]
    fn leader_buffer_sequence_switches_buffer() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        let first = create_buffer(&mut model, "first");
        create_buffer(&mut model, "second");

        manager.update(
            &mut model,
            Message::CommandMenuKey(crossterm::event::KeyCode::Char(' ')),
        );
        manager.update(
            &mut model,
            Message::CommandMenuKey(crossterm::event::KeyCode::Char('b')),
        );
        manager.update(
            &mut model,
            Message::CommandMenuKey(crossterm::event::KeyCode::Char('n')),
        );

        assert_eq!(model.editor.active_buffer(), Some(first));
        assert!(!model.command_menu.is_active());
    }

    #[test]
    fn leader_q_q_quits() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        for key in [' ', 'q', 'q'] {
            manager.update(
                &mut model,
                Message::CommandMenuKey(crossterm::event::KeyCode::Char(key)),
            );
        }

        assert_eq!(model.running_state, RunningState::Done);
    }
}
