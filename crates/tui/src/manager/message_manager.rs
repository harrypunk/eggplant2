use std::sync::Arc;

use editor::DocumentFactory;

use crate::{
    buffer,
    command_menu::CommandMenuOutcome,
    composition::actions::general_actions,
    data_object::{Message, Model, StatusMessage},
    manager::{
        command_registry::{CommandHandler, CommandRegistry},
        effect::Effect,
        general_action::GeneralActionHandler,
        input_routing::InputRouting,
        routed_input::RoutedInput,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug)]
pub struct MessageManager {
    documents: DocumentFactory,
    commands: CommandRegistry,
    text_input: TextInputRegistry,
    general_actions: Arc<dyn GeneralActionHandler>,
    routing: Arc<dyn InputRouting>,
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
            general_actions: Arc::new(general_actions()),
            routing: crate::composition::input::routing(),
        }
    }

    pub fn with_general_action_handler(
        mut self,
        general_actions: Arc<dyn GeneralActionHandler>,
    ) -> Self {
        self.general_actions = general_actions;
        self
    }

    pub fn with_input_routing(mut self, routing: Arc<dyn InputRouting>) -> Self {
        self.routing = routing;
        self
    }

    /// Dispatch input using the context captured during event translation.
    pub fn update_input(&self, model: &mut Model, input: RoutedInput) -> Option<Effect> {
        let RoutedInput { context, message } = input;
        match message {
            Message::GeneralAction(action) => self.general_actions.handle(model, context, action),
            Message::TextInput(event) => self.text_input.handle(model, context, event),
            message => self.update(model, message),
        }
    }

    pub fn update(&self, model: &mut Model, msg: Message) -> Option<Effect> {
        match msg {
            Message::CommandMenuStart => {
                model.command_menu.start();
                return None;
            }
            Message::CommandMenuKey(key) => {
                return match model.command_menu.handle_key(key) {
                    CommandMenuOutcome::Pending | CommandMenuOutcome::Cancelled => None,
                    CommandMenuOutcome::Action(action) => self.invoke_command(model, action),
                };
            }
            Message::InvokeCommand(id) => return self.invoke_command(model, id),
            Message::FeatureCompletion(completion) => return completion.update(model),
            msg @ (Message::GeneralAction(_) | Message::TextInput(_)) => {
                // Compatibility for direct semantic-message callers. Terminal input uses
                // `update_input`, so its originally resolved context cannot change in flight.
                let context = self.routing.context(model)?;
                return self.update_input(model, RoutedInput::new(context, msg));
            }
            Message::FocusLost => return buffer::save_active(model),
            Message::ExternalFileChanged { path, kind } => {
                return crate::external_sync::file_changed(model, &path, kind);
            }
            Message::Resize(width, height) => {
                crate::page::resize(model, width, height);
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
            return Some(crate::notifications::report_status(
                model,
                StatusMessage::error(format!("unknown command `{}`", id.as_str())),
            ));
        };
        match &command.handler {
            CommandHandler::Message(message) => self.update(model, message()),
            CommandHandler::Feature(handler) => handler(model, id),
            CommandHandler::Document(handler) => handler(&self.documents, model),
            CommandHandler::Dispatch(selector) => match selector(model) {
                Some(target) => self.invoke_command(model, target),
                None => None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use crate::component::text_input::TextInputEvent;
    use editor::{EditMode, SaveRequest};

    use super::*;
    use crate::{
        composition::input::input_context,
        data_object::{
            CommandId, Context, FocusTarget, GeneralAction, RunningState, StatusKind, Visibility,
        },
        startup::initial_model,
        test_utils::{create_buffer, open_file},
    };

    #[derive(Debug)]
    struct RecordingActions {
        contexts: Arc<Mutex<Vec<Context>>>,
    }

    impl GeneralActionHandler for RecordingActions {
        fn handle(
            &self,
            _model: &mut Model,
            context: Context,
            _action: GeneralAction,
        ) -> Option<Effect> {
            self.contexts.lock().unwrap().push(context);
            None
        }
    }

    #[test]
    fn routed_input_dispatches_with_its_captured_context() {
        let contexts = Arc::new(Mutex::new(Vec::new()));
        let manager =
            MessageManager::default().with_general_action_handler(Arc::new(RecordingActions {
                contexts: contexts.clone(),
            }));
        let mut model = Model::default();

        manager.update_input(
            &mut model,
            RoutedInput::new(
                Context::ModelSelector,
                Message::GeneralAction(GeneralAction::Next),
            ),
        );

        assert_eq!(
            contexts.lock().unwrap().as_slice(),
            [Context::ModelSelector]
        );
    }

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
        manager.update(model, Message::completion(request.perform()));
    }

    #[test]
    fn old_open_completion_does_not_displace_a_newer_picker() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("note.txt"), "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let manager = MessageManager::default();
        let completion = match manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        ) {
            Some(Effect::OpenDocument(request)) => request.perform(),
            _ => panic!("expected an open-document effect"),
        };

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_file_picker")),
        );
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        manager.update(&mut model, Message::completion(completion));

        assert_eq!(model.file_picker.visibility, Visibility::Displayed);
        assert_eq!(model.current_focus, FocusTarget::FilePicker);
        assert_eq!(input_context(&model), Some(Context::FilePicker));
        assert!(model.modal.is_active(Context::FilePicker));
        assert!(model.context_stack.is_empty());
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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
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
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('A')));
        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        ));
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));
        assert_eq!(std::fs::read_to_string(&first_path).unwrap(), "Afirst");

        manager.update(&mut model, Message::InvokeCommand(CommandId("focus_left")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("explorer_next")),
        );
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
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
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('B')));
        assert!(model.editor.active_is_modified());
        let effect = manager.update(&mut model, Message::InvokeCommand(CommandId("quit")));
        assert!(effect.is_some(), "quit did not save: {:?}", model.status);
        let request = expect_save(effect);
        assert_eq!(model.running_state, RunningState::Running);
        assert!(model.editor.has_saving_buffers());
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));

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

        let effect = MessageManager::default().update(&mut model, Message::completion(completion));

        assert!(matches!(effect, Some(Effect::Feature(_))));
    }

    #[test]
    fn failed_save_keeps_buffer_dirty() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        create_buffer(&mut model, "scratch");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );

        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('!')));
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
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('!')));

        let request = expect_save(manager.update(&mut model, Message::FocusLost));
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));

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
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
        complete_open(&mut model, &manager, effect);

        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
        assert!(model.context_stack.is_empty());
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
        assert!(model.context_stack.is_empty());
        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_selected")),
        );
        complete_open(&mut model, &manager, effect);
        assert_eq!(model.editor.buffer_count(), 1);
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }
}
