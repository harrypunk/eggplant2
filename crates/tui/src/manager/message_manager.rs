use editor::{DocumentFactory, DocumentOpenCompletion};
use ratatui::layout::Rect;

use crate::{
    buffer,
    command_menu::CommandMenuOutcome,
    data_object::{Message, Model, StatusMessage, Visibility},
    document,
    explorer::ExplorerEntryKind,
    file_picker, focus, lifecycle,
    manager::{
        command_registry::{CommandHandler, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    page::editor::editor_content_area,
};

#[derive(Debug)]
pub struct MessageManager {
    documents: DocumentFactory,
    commands: CommandRegistry,
    text_input: TextInputRegistry,
}

impl Default for MessageManager {
    fn default() -> Self {
        Self::new(CommandRegistry::builtin(), TextInputRegistry::builtin())
    }
}

impl MessageManager {
    pub fn new(commands: CommandRegistry, text_input: TextInputRegistry) -> Self {
        Self {
            documents: DocumentFactory::new(),
            commands,
            text_input,
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
            Message::FeatureCompletion(completion) => return completion.update(model),
            Message::TextInput(character) => return self.text_input.handle(model, character),
            Message::SaveFinished(completion) => {
                return lifecycle::finish_save(model, completion);
            }
            Message::OpenSelected => return self.open_selected(model),
            Message::FileOpened(completion) => Self::finish_open_file(model, completion),
            Message::FileScanFinished { revision, result } => {
                return file_picker::complete_scan(model, revision, result);
            }
            Message::FileFilterFinished { revision, results } => {
                return file_picker::complete_filter(model, revision, results);
            }
            Message::FilePreviewFinished { revision, preview } => {
                file_picker::complete_preview(model, revision, preview);
            }
            Message::FocusLost => return buffer::save_active(model),
            Message::Resize(width, height) => {
                let area = editor_content_area(
                    Rect::new(0, 0, width, height),
                    model.explorer.visibility,
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
            CommandHandler::Message(message) => self.update(model, message()),
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

    fn finish_open_file(model: &mut Model, completion: DocumentOpenCompletion) {
        if document::finish_open_file(model, completion).is_none() {
            return;
        }
        match model.file_picker.visibility {
            Visibility::Displayed => file_picker::close(model),
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
        data_object::{CommandId, Context, FocusTarget, RunningState, StatusKind},
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
        manager.update(&mut model, Message::TextInput('A'));
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
        manager.update(&mut model, Message::TextInput('B'));
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

        manager.update(&mut model, Message::TextInput('!'));
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
        manager.update(&mut model, Message::TextInput('!'));

        let request = expect_save(manager.update(&mut model, Message::FocusLost));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(std::fs::read_to_string(path).unwrap(), "!hello");
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
        assert_eq!(model.explorer.visibility, Visibility::Hidden);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("toggle_file_explorer")),
        );
        assert_eq!(model.explorer.visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FileExplorer);
        assert_eq!(model.context_stack.last(), Some(&Context::Explorer));
        let effect = manager.update(&mut model, Message::OpenSelected);
        complete_open(&mut model, &manager, effect);
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }
}
