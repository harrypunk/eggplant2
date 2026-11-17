use editor::DocumentFactory;
use ratatui::layout::Rect;

use crate::{
    buffer,
    command_menu::CommandMenuOutcome,
    data_object::{CommandId, Message, Model, StatusMessage, Visibility},
    document,
    explorer::ExplorerEntryKind,
    file_picker, focus, lifecycle,
    manager::{
        command_registry::{CommandHandler, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    page::{editor::editor_content_area, vcs_commit::message_content_area},
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
                return Self::finish_save(model, completion);
            }
            Message::OpenSelected => return self.open_selected(model),
            Message::FileOpened(completion) => return Self::finish_open_file(model, completion),
            Message::FocusLost => return buffer::save_active(model),
            Message::ExternalFileChanged { path, kind } => {
                return crate::external_sync::file_changed(model, &path, kind);
            }
            Message::Resize(width, height) => {
                let area = editor_content_area(
                    Rect::new(0, 0, width, height),
                    model.explorer.visibility,
                    model.tools.visibility,
                    model.editor.active_snapshot().as_ref(),
                );
                model.editor.resize_active_view(area.width, area.height);
                let area = message_content_area(
                    Rect::new(0, 0, width, height),
                    model.vcs.commit.editor.active_snapshot().as_ref(),
                );
                model.vcs.commit.resize_active_view(area.width, area.height);
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
        if Self::lifecycle_blocked_by_commit(model, id) {
            model.status = Some(StatusMessage::info(
                "Commit in progress; try again after it finishes",
            ));
            return None;
        }
        match &command.handler {
            CommandHandler::Message(message) => self.update(model, message()),
            CommandHandler::Feature(handler) => handler(model, id),
        }
    }

    fn lifecycle_blocked_by_commit(model: &Model, id: CommandId) -> bool {
        const LIFECYCLE_COMMANDS: [&str; 5] = [
            "quit",
            "close_buffer",
            "close_other_buffers",
            "close_buffers_right",
            "close_all_buffers",
        ];

        model.vcs.commit.is_in_progress() && LIFECYCLE_COMMANDS.contains(&id.as_str())
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

    fn finish_save(model: &mut Model, completion: editor::SaveCompletion) -> Option<Effect> {
        match lifecycle::complete_save(model, completion) {
            lifecycle::SaveOutcome::Saved(buffer_id) => {
                if model.vcs.commit.is_waiting_for_save() {
                    crate::vcs::commit::continue_after_save(model)
                } else {
                    let continuation = lifecycle::continue_save(model, buffer_id);
                    Effect::combine(continuation, crate::external_sync::save_completed(model))
                }
            }
            lifecycle::SaveOutcome::Failed(error) => {
                lifecycle::save_failed(model, &error);
                crate::vcs::commit::save_failed(model, &error);
                None
            }
        }
    }

    fn finish_open_file(
        model: &mut Model,
        completion: document::OpenFileCompletion,
    ) -> Option<Effect> {
        let opened = document::finish_open_file(model, completion)?;
        let reconcile = model.context_stack.last() == opened.context.as_ref()
            && match opened.origin {
                document::OpenOrigin::FilePicker { session } => {
                    model.file_picker.visibility == Visibility::Displayed
                        && model.file_picker.session_revision() == session
                }
                document::OpenOrigin::Other => true,
            };
        if reconcile {
            if matches!(opened.origin, document::OpenOrigin::FilePicker { .. }) {
                file_picker::close(model);
            }
            focus::focus_editor(model);
        }
        crate::vcs::sync_active_buffer(model)
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

    #[test]
    fn old_open_completion_does_not_displace_a_newer_picker() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let completion = match manager.update(&mut model, Message::OpenSelected) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_file_picker")),
        );
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        manager.update(&mut model, Message::FileOpened(completion));

        assert_eq!(model.file_picker.visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(model.context_stack.last(), Some(&Context::FilePicker));
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

        let _ = manager.update(
            &mut model,
            Message::InvokeCommand(crate::data_object::CommandId("previous_buffer")),
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

    #[tokio::test]
    async fn successful_save_schedules_status_refresh() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        model.editor.enter_normal_mode();
        let completion = model.editor.save_buffer(id).unwrap().perform().await;

        let effect =
            MessageManager::default().update(&mut model, Message::SaveFinished(completion));

        assert!(matches!(effect, Some(Effect::Feature(_))));
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
