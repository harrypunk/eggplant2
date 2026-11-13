use editor::{DocumentFactory, DocumentOpenCompletion};
use ratatui::layout::Rect;

use crate::{
    buffer,
    command_menu::CommandMenuOutcome,
    data_object::{Context, FocusTarget, Message, Model, StatusMessage, Visibility},
    document, editing,
    explorer::ExplorerEntryKind,
    focus, lifecycle,
    manager::{
        command_registry::{CommandHandler, CommandRegistry},
        effect::Effect,
    },
    page::editor::editor_content_area,
};

#[derive(Debug)]
pub struct MessageManager {
    documents: DocumentFactory,
    commands: CommandRegistry,
}

impl Default for MessageManager {
    fn default() -> Self {
        Self::new(CommandRegistry::builtin())
    }
}

impl MessageManager {
    pub fn new(commands: CommandRegistry) -> Self {
        Self {
            documents: DocumentFactory::new(),
            commands,
        }
    }

    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Effect> {
        match msg {
            Message::CommandMenuKey(key) => {
                return match model.command_menu.handle_key(key) {
                    CommandMenuOutcome::Pending | CommandMenuOutcome::Cancelled => None,
                    CommandMenuOutcome::Action(action) => self.invoke_command(model, action),
                };
            }
            Message::InvokeCommand(id) => return self.invoke_command(model, id),
            Message::InsertChar(character) => return editing::insert_char(model, character),
            Message::SaveFinished(completion) => {
                return lifecycle::finish_save(model, completion);
            }
            Message::OpenSelected => return self.open_selected(model),
            Message::OpenFilePicker => return self.open_file_picker(model),
            Message::FilePickerInsert(character) => {
                let Some(workspace) = &model.workspace else {
                    return None;
                };
                return model
                    .file_picker
                    .insert(workspace.root(), character)
                    .map(Effect::feature);
            }
            Message::FilePickerBackspace => {
                let Some(workspace) = &model.workspace else {
                    return None;
                };
                return model
                    .file_picker
                    .backspace(workspace.root())
                    .map(Effect::feature);
            }
            Message::CloseFilePicker => Self::close_file_picker(model),
            Message::FilePickerNext => {
                return model
                    .file_picker
                    .select_next(&self.documents)
                    .map(Effect::feature);
            }
            Message::FilePickerPrevious => {
                return model
                    .file_picker
                    .select_previous(&self.documents)
                    .map(Effect::feature);
            }
            Message::OpenPickerFile => return self.open_picker_file(model),
            Message::FileOpened(completion) => Self::finish_open_file(model, completion),
            Message::FileScanFinished { revision, result } => {
                let Some(workspace) = &model.workspace else {
                    return None;
                };
                return model
                    .file_picker
                    .complete_scan(workspace.root(), revision, result)
                    .map(Effect::feature);
            }
            Message::FileFilterFinished { revision, results } => {
                return model
                    .file_picker
                    .complete_filter(&self.documents, revision, results)
                    .map(Effect::feature);
            }
            Message::FilePreviewFinished { revision, preview } => {
                model.file_picker.complete_preview(revision, preview);
            }
            Message::FocusLost => return buffer::save_active(model),
            Message::Resize(width, height) => {
                let area = editor_content_area(
                    Rect::new(0, 0, width, height),
                    model.explorer_visibility,
                    model.tools.visibility,
                    model.editor.active_snapshot().as_ref(),
                );
                model.editor.resize_active_view(area.width, area.height);
            }
            _ => {}
        }
        None
    }

    fn invoke_command(
        &self,
        model: &mut Model,
        id: crate::data_object::CommandId,
    ) -> Option<Effect> {
        let Some(command) = self.commands.get(id.as_str()) else {
            model.status = Some(StatusMessage::error(format!(
                "unknown command `{}`",
                id.as_str()
            )));
            return None;
        };
        match &command.handler {
            CommandHandler::Message(message) => self.update(model, message.clone()),
            CommandHandler::Feature(handler) => handler(model, id),
        }
    }

    fn open_selected(&self, model: &mut Model) -> Option<Effect> {
        let entry = model.explorer.selected_entry()?;
        let path = entry.path.clone();
        match entry.kind {
            ExplorerEntryKind::File | ExplorerEntryKind::Link | ExplorerEntryKind::Other => {
                document::open_file(&self.documents, model, &path)
            }
            ExplorerEntryKind::Directory => crate::explorer::handle_command(
                model,
                crate::data_object::CommandId("explorer_unfold"),
            ),
        }
    }

    fn open_file_picker(&self, model: &mut Model) -> Option<Effect> {
        let Some(workspace) = &model.workspace else {
            model.status = Some(StatusMessage::info("No folder open"));
            return None;
        };
        let root = workspace.root().to_path_buf();
        if model.context_stack.last() != Some(&Context::FilePicker) {
            model.context_stack.push(Context::FilePicker);
        }
        model.current_focus = FocusTarget::FilePicker;
        model
            .file_picker
            .open(&root, &self.documents)
            .map(Effect::feature)
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

    fn open_picker_file(&self, model: &mut Model) -> Option<Effect> {
        let entry = model.file_picker.selected_entry()?;
        let path = entry.path.clone();
        document::open_file(&self.documents, model, &path)
    }

    fn finish_open_file(model: &mut Model, completion: DocumentOpenCompletion) {
        if document::finish_open_file(model, completion).is_none() {
            return;
        }
        match model.file_picker.visibility {
            Visibility::Displayed => Self::close_file_picker(model),
            Visibility::Hidden => {}
        }
        focus::focus_editor(model);
    }
}

#[cfg(test)]
mod tests {
    use editor::{EditMode, SaveRequest};

    use super::*;
    use crate::{
        data_object::{CommandId, RunningState, StatusKind},
        explorer::ExpansionState,
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

    fn complete_open(model: &mut Model, manager: &MessageManager, effect: Option<Effect>) {
        let Some(Effect::OpenDocument(request)) = effect else {
            panic!("expected an open-document effect");
        };
        manager.update(model, Message::FileOpened(request.perform()));
    }

    #[tokio::test]
    async fn explorer_edit_switch_close_and_pending_save_quit_work_end_to_end() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("a.txt");
        let second_path = directory.path().join("b.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();

        let effect = manager.update(&mut model, Message::OpenSelected);
        complete_open(&mut model, &manager, effect);
        let first = model.editor.active_buffer().unwrap();
        assert_eq!(
            model.editor.buffer_path(first),
            Some(std::fs::canonicalize(&first_path).unwrap().as_path())
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('A'));
        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        ));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));
        assert_eq!(std::fs::read_to_string(&first_path).unwrap(), "Afirst");

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_next")),
        );
        let effect = manager.update(&mut model, Message::OpenSelected);
        complete_open(&mut model, &manager, effect);
        let second = model.editor.active_buffer().unwrap();
        assert_ne!(first, second);

        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(crate::data_object::CommandId("previous_buffer"))
                )
                .is_none()
        );
        assert_eq!(model.editor.active_buffer(), Some(first));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_buffer")),
        );
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.editor.active_buffer(), Some(second));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('B'));
        assert!(model.editor.active_is_modified());
        let effect = manager.update(&mut model, Message::InvokeCommand(CommandId("quit")));
        assert!(effect.is_some(), "quit did not save: {:?}", model.status);
        let request = expect_save(effect);
        assert_eq!(model.running_state, RunningState::Running);
        assert!(model.editor.has_saving_buffers());
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(model.running_state, RunningState::Done);
        assert!(!model.editor.has_saving_buffers());
        assert_eq!(std::fs::read_to_string(second_path).unwrap(), "Bsecond");
    }

    #[test]
    fn failed_save_keeps_buffer_dirty() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        create_buffer(&mut model, "scratch");
        model.context_stack.push(Context::AppNormal);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );

        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        assert!(effect.is_none());
        assert!(model.editor.active_is_modified());
        assert_eq!(model.status.unwrap().kind, StatusKind::Error);
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
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('!'));

        let request = expect_save(manager.update(&mut model, Message::FocusLost));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "!hello");
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
        assert!(effect.is_none());
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(model.context_stack.last(), Some(&Context::FilePicker));

        let Some(Effect::Feature(request)) =
            manager.update(&mut model, Message::FilePickerInsert('m'))
        else {
            panic!("a query should start one recursive scan");
        };
        let Some(Effect::Feature(request)) = manager.update(&mut model, request.perform()) else {
            panic!("recursive scan completion should start filtering");
        };
        let Some(Effect::Feature(request)) = manager.update(&mut model, request.perform()) else {
            panic!("filter completion should start a preview");
        };
        manager.update(&mut model, request.perform());
        assert_eq!(model.file_picker.results.len(), 1);
        let PreviewState::Text(snapshot) = &model.file_picker.preview else {
            panic!("text file should have a preview snapshot");
        };
        assert_eq!(snapshot.lines[0].text, "fn main() {}");
        assert!(snapshot.lines[0].highlights.iter().any(|span| {
            !span.highlights.is_empty() && span.start_byte == 0 && span.end_byte == 2
        }));
        assert!(snapshot.lines[0].highlights.iter().any(|span| {
            !span.highlights.is_empty() && span.start_byte == 3 && span.end_byte == 7
        }));

        let effect = manager.update(&mut model, Message::OpenPickerFile);
        complete_open(&mut model, &manager, effect);
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
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "fn main() {}"
        );
    }

    #[test]
    fn explorer_navigation_opens_files_and_toggles_focus() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        std::fs::write(directory.path().join("b.txt"), "b").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_next")),
        );
        assert_eq!(model.explorer.selected, Some(1));
        let effect = manager.update(&mut model, Message::OpenSelected);
        complete_open(&mut model, &manager, effect);

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.lines[0].text, "b");
        assert_eq!(
            std::path::Path::new(&snapshot.display_name)
                .file_name()
                .unwrap(),
            "b.txt"
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer_visibility, Visibility::Hidden);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer_visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        let effect = manager.update(&mut model, Message::OpenSelected);
        complete_open(&mut model, &manager, effect);
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }
}
