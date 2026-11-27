use std::{collections::HashSet, sync::Arc, time::Duration};

use crossterm::event::{KeyEvent, KeyEventKind};
use ratatui::crossterm::event::{self, Event};

use crate::config::Config;
use crate::data_object::{Context, Message, Model};
use crate::manager::{
    context::{KeyChord, LeaderInput, leader_input},
    input_resolver::{InputResolver, InputResolverChain, default_input_resolvers},
    input_routing::InputRouting,
    keyboard_input_registry::Registry,
    keymap_contribution::KeymapCapability,
    routed_input::RoutedInput,
    text_input_registry::TextInputRegistry,
};

/// Translation result separating context-owned input from context-free system messages.
#[derive(Debug)]
pub enum TranslatedEvent {
    Input(RoutedInput),
    System(Message),
}

#[derive(Debug)]
pub struct EventManager {
    poll_timeout: Duration,
    leader: KeyChord,
    leader_contexts: HashSet<Context>,
    input: InputResolverChain,
    routing: Arc<dyn InputRouting>,
}

impl EventManager {
    pub fn new(
        config: &Config,
        keymap: Registry,
        text_input: TextInputRegistry,
    ) -> color_eyre::Result<Self> {
        Self::from_resolver(config, default_input_resolvers(keymap, text_input))
    }

    pub fn from_resolver(config: &Config, input: InputResolverChain) -> color_eyre::Result<Self> {
        let leader = config
            .leader
            .parse::<KeyChord>()
            .map_err(|error| color_eyre::eyre::eyre!(error))?;
        let leader_contexts = crate::composition::keymaps::catalog()
            .contexts_with(KeymapCapability::LeaderMenu)
            .collect();
        Ok(Self {
            poll_timeout: Duration::from_millis(config.event_poll_timeout),
            leader,
            leader_contexts,
            input,
            routing: crate::composition::input::routing(),
        })
    }

    pub fn with_input_resolver(mut self, input: InputResolverChain) -> Self {
        self.input = input;
        self
    }

    pub fn with_input_routing(mut self, routing: Arc<dyn InputRouting>) -> Self {
        self.routing = routing;
        self
    }

    /// Wait for the next terminal event and translate it into routed input or a system message.
    pub fn next_event(&self, model: &Model) -> color_eyre::Result<Option<TranslatedEvent>> {
        Ok(self
            .read_event()?
            .and_then(|event| self.translate_event(event, model)))
    }

    /// Raw I/O: poll crossterm for the next event.
    fn read_event(&self) -> color_eyre::Result<Option<Event>> {
        if event::poll(self.poll_timeout)? {
            return Ok(Some(event::read()?));
        }
        Ok(None)
    }

    /// Pure routing: dispatch on the event kind and retain the resolved input context.
    pub(crate) fn translate_event(&self, event: Event, model: &Model) -> Option<TranslatedEvent> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                self.translate_key(key, model).map(TranslatedEvent::Input)
            }
            Event::FocusLost => Some(TranslatedEvent::System(Message::FocusLost)),
            Event::Resize(width, height) => {
                Some(TranslatedEvent::System(Message::Resize(width, height)))
            }
            // Event::Mouse(mouse) => MouseManager.translate(mouse, model),
            _ => None,
        }
    }

    fn translate_key(&self, key: KeyEvent, model: &Model) -> Option<RoutedInput> {
        let context = self.routing.context(model)?;
        let chord = KeyChord::from(key);
        let message = match leader_input(
            chord,
            self.leader,
            model.command_menu.is_active(),
            self.leader_contexts.contains(&context),
        ) {
            Some(LeaderInput::Start) => Message::CommandMenuStart,
            Some(LeaderInput::Pending) => Message::CommandMenuKey(chord),
            None => self.input.resolve(context, chord)?,
        };
        Some(RoutedInput::new(context, message))
    }

    #[cfg(test)]
    fn translate(&self, event: Event, model: &Model) -> Option<Message> {
        match self.translate_event(event, model)? {
            TranslatedEvent::Input(input) => Some(input.message),
            TranslatedEvent::System(message) => Some(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::{
        component::text_input::{TextCursorMovement, TextInputEvent},
        data_object::{AppPage, Context, FocusTarget, GeneralAction},
        manager::{
            command_registry::{CommandId, CommandRegistry},
            message_manager::MessageManager,
        },
    };

    fn event_manager() -> EventManager {
        let config = Config::default();
        let keymap = Registry::from_config(
            &config.keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        EventManager::new(&config, keymap, TextInputRegistry::builtin()).unwrap()
    }

    fn dispatch_key(
        events: &EventManager,
        messages: &MessageManager,
        model: &mut Model,
        key: KeyCode,
    ) {
        let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
        let Some(TranslatedEvent::Input(input)) = events.translate_event(event, model) else {
            panic!("expected routable input");
        };
        let _ = messages.update_input(model, input);
    }

    fn model_with_buffer(context: Context) -> Model {
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "text");
        match context {
            Context::AppNormal => {}
            Context::Explorer => model.current_focus = FocusTarget::FileExplorer,
            Context::VCS => model.active_page = AppPage::Vcs,
            Context::VcsHistory => model.active_page = AppPage::VcsHistory,
            Context::VcsCommit => model.active_page = AppPage::VcsCommit,
            Context::ProviderLogin => assert!(model.modal.open(Context::ProviderLogin)),
            transient => model.context_stack.push(transient),
        }
        model
    }

    #[test]
    fn translated_input_retains_the_context_that_resolved_the_key() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::FilePicker));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));

        let Some(TranslatedEvent::Input(RoutedInput {
            context: Context::FilePicker,
            message: Message::TextInput(TextInputEvent::Insert('x')),
        })) = manager.translate_event(event, &model)
        else {
            panic!("file-picker input must retain its resolved context");
        };
    }

    #[test]
    fn insert_context_falls_back_to_printable_characters() {
        let manager = event_manager();
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "text");
        model.editor.enter_insert_mode();

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert!(manager.translate(event, &model).is_none());

        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert(' ')))
        ));
    }

    #[test]
    fn terminal_line_editing_requires_full_text_input_capability() {
        let manager = event_manager();
        let mut picker = Model::default();
        assert!(picker.modal.open(Context::FilePicker));
        for (character, expected) in [
            ('a', TextInputEvent::Cursor(TextCursorMovement::LineStart)),
            ('e', TextInputEvent::Cursor(TextCursorMovement::LineEnd)),
            ('u', TextInputEvent::DeleteToLineStart),
        ] {
            let event = Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            ));
            assert!(matches!(
                manager.translate(event, &picker),
                Some(Message::TextInput(actual)) if actual == expected
            ));
        }

        for character in ['j', 'k'] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &picker),
                Some(Message::TextInput(TextInputEvent::Insert(actual))) if actual == character
            ));
        }

        let prefix_input = model_with_buffer(Context::BufferJump);
        for character in ['a', 'e', 'u'] {
            let event = Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            ));
            assert!(manager.translate(event, &prefix_input).is_none());
        }
    }

    #[test]
    fn routes_picker_input_without_starting_the_leader() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::FilePicker));

        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert(' ')))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::GeneralAction(GeneralAction::Back))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::GeneralAction(GeneralAction::Next))
        ));
        for (character, action) in [('n', GeneralAction::Next), ('p', GeneralAction::Previous)] {
            let event = Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            ));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
    }

    #[test]
    fn explicit_ctrl_binding_overrides_menu_navigation_alias() {
        let mut config = Config::default();
        config
            .keymap
            .entry("file_picker".to_string())
            .or_default()
            .insert("toggle_file_explorer".to_string(), "ctrl-n".to_string());
        let keymap = Registry::from_config(
            &config.keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        let manager = EventManager::new(&config, keymap, TextInputRegistry::builtin()).unwrap();
        let mut model = Model::default();
        assert!(model.modal.open(Context::FilePicker));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::CONTROL));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("toggle_file_explorer")))
        ));
    }

    #[test]
    fn configured_leader_starts_and_continues_the_command_menu() {
        let config = Config::parse("leader = \",\"").unwrap();
        let keymap = Registry::from_config(
            &config.keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        let manager = EventManager::new(&config, keymap, TextInputRegistry::builtin()).unwrap();
        let mut model = model_with_buffer(Context::AppNormal);

        let comma = KeyEvent::new(KeyCode::Char(','), KeyModifiers::NONE);
        assert!(matches!(
            manager.translate(Event::Key(comma), &model),
            Some(Message::CommandMenuStart)
        ));
        let space = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        let translated = manager.translate(space, &model);
        assert!(!matches!(translated, Some(Message::CommandMenuStart)));

        model.command_menu.start();
        let next = KeyChord::from(KeyCode::Char('b'));
        assert!(matches!(
            manager.translate(Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE)), &model),
            Some(Message::CommandMenuKey(actual)) if actual == next
        ));
    }

    #[test]
    fn invalid_leader_configuration_is_rejected_at_startup() {
        let config = Config {
            leader: "not-a-key".to_string(),
            ..Config::default()
        };
        let keymap = Registry::from_config(
            &config.keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        assert!(EventManager::new(&config, keymap, TextInputRegistry::builtin()).is_err());
    }

    #[test]
    fn common_escape_and_backspace_work_without_configured_entries() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::ThemePicker));
        assert!(matches!(
            manager.translate(
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                &model,
            ),
            Some(Message::GeneralAction(GeneralAction::Back))
        ));
        assert!(matches!(
            manager.translate(
                Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
                &model,
            ),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));

        model.modal.close(Context::ThemePicker);
        crate::test_utils::create_buffer(&mut model, "text");
        model.editor.enter_insert_mode();
        assert!(matches!(
            manager.translate(
                Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
                &model,
            ),
            Some(Message::InvokeCommand(CommandId("exit_insert_mode")))
        ));
        assert!(matches!(
            manager.translate(
                Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
                &model,
            ),
            Some(Message::InvokeCommand(CommandId("backspace")))
        ));
    }

    #[test]
    fn routes_only_theme_picker_navigation_and_actions() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::ThemePicker));

        let translate =
            |key| manager.translate(Event::Key(KeyEvent::new(key, KeyModifiers::NONE)), &model);
        assert!(matches!(
            translate(KeyCode::Down),
            Some(Message::GeneralAction(GeneralAction::Next))
        ));
        assert!(matches!(
            translate(KeyCode::Up),
            Some(Message::GeneralAction(GeneralAction::Previous))
        ));
        for (character, action) in [('n', GeneralAction::Next), ('p', GeneralAction::Previous)] {
            let event = Event::Key(KeyEvent::new(
                KeyCode::Char(character),
                KeyModifiers::CONTROL,
            ));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        assert!(matches!(
            translate(KeyCode::Enter),
            Some(Message::GeneralAction(GeneralAction::Activate))
        ));
        assert!(matches!(
            translate(KeyCode::Esc),
            Some(Message::GeneralAction(GeneralAction::Back))
        ));

        for (key, character) in [
            (KeyCode::Char('j'), 'j'),
            (KeyCode::Char('k'), 'k'),
            (KeyCode::Char(' '), ' '),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::TextInput(TextInputEvent::Insert(actual))) if actual == character
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
    }

    #[test]
    fn routes_bracket_buffer_navigation_and_cancellation() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);

        for (character, command) in [
            (']', "next_buffer_pending"),
            ('[', "previous_buffer_pending"),
        ] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        model.context_stack.push(Context::BufferNavigation);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('b')))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId(
                "buffer_navigation_cancel"
            )))
        ));
    }

    #[test]
    fn routes_vcs_page_keys() {
        let manager = event_manager();
        let model = Model {
            active_page: AppPage::Vcs,
            ..Model::default()
        };

        for (key, command) in [
            (KeyCode::Esc, "close_vcs_page"),
            (KeyCode::Char('q'), "close_vcs_page_q"),
            (KeyCode::Char('j'), "vcs_next_change"),
            (KeyCode::Char('k'), "vcs_previous_change"),
            (KeyCode::Enter, "open_vcs_change"),
            (KeyCode::Char('s'), "vcs_stage"),
            (KeyCode::Char('u'), "vcs_unstage"),
            (KeyCode::Tab, "vcs_toggle_diff"),
            (KeyCode::Char('x'), "vcs_discard"),
            (KeyCode::Char('c'), "vcs_commit_placeholder"),
            (KeyCode::Char('?'), "vcs_help_placeholder"),
            (KeyCode::Char('l'), "open_vcs_history"),
            (KeyCode::Char('m'), "vcs_merge_start"),
            (KeyCode::Char('r'), "vcs_reset_menu"),
            (KeyCode::Char('b'), "vcs_branch_menu"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        for (key, modifiers, command) in [
            (KeyCode::Char('s'), KeyModifiers::CONTROL, "vcs_stage_all"),
            (KeyCode::Char('U'), KeyModifiers::SHIFT, "vcs_unstage_all"),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
    }

    #[test]
    fn routes_vcs_history_page_keys() {
        let manager = event_manager();
        let model = Model {
            active_page: AppPage::VcsHistory,
            ..Model::default()
        };

        for (key, command) in [
            (KeyCode::Esc, "close_vcs_history"),
            (KeyCode::Char('q'), "close_vcs_history_q"),
            (KeyCode::Char('j'), "vcs_history_next"),
            (KeyCode::Char('k'), "vcs_history_previous"),
            (KeyCode::Tab, "vcs_history_toggle_diff"),
            (KeyCode::Enter, "open_vcs_commit_detail"),
            (KeyCode::Char('c'), "vcs_history_copy_short"),
            (KeyCode::Char('n'), "vcs_history_load_more"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char('C'), KeyModifiers::SHIFT));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("vcs_history_copy_full")))
        ));
    }

    #[test]
    fn routes_vcs_merge_input_keys() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::VcsMerge));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));
        for (key, action) in [
            (KeyCode::Down, GeneralAction::Next),
            (KeyCode::Up, GeneralAction::Previous),
            (KeyCode::Enter, GeneralAction::Activate),
            (KeyCode::Esc, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
    }

    #[test]
    fn routes_agent_dialog_keys() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::CONTROL));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("agent_open")))
        ));

        assert!(model.modal.open(Context::AgentChatDialog));
        for (key, modifiers, action) in [
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        for (key, modifiers, command) in [
            (KeyCode::F(7), KeyModifiers::CONTROL, "agent_interrupt"),
            (KeyCode::Tab, KeyModifiers::NONE, "agent_command_quick_fill"),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
        for (key, movement) in [
            (KeyCode::Left, TextCursorMovement::Left),
            (KeyCode::Right, TextCursorMovement::Right),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::TextInput(TextInputEvent::Cursor(actual))) if actual == movement
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));

        assert!(
            model
                .modal
                .replace(Context::AgentChatDialog, Context::AgentApproval)
        );
        for (key, modifiers, command) in [
            (KeyCode::Char('y'), KeyModifiers::NONE, "agent_approve"),
            (KeyCode::Char('n'), KeyModifiers::NONE, "agent_reject"),
            (KeyCode::Esc, KeyModifiers::NONE, "agent_close"),
            (KeyCode::F(7), KeyModifiers::CONTROL, "agent_interrupt"),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(!matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert(_)))
        ));
    }

    #[test]
    fn routes_welcome_buttons_and_preserves_leader_commands() {
        let manager = event_manager();
        let model = crate::startup::initial_model(None).unwrap();
        assert!(model.context_stack.is_empty());

        for (key, modifiers, action) in [
            (KeyCode::Up, KeyModifiers::NONE, GeneralAction::Previous),
            (KeyCode::Down, KeyModifiers::NONE, GeneralAction::Next),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        for (key, modifiers, command) in [
            (KeyCode::Enter, KeyModifiers::NONE, "welcome_activate"),
            (KeyCode::Char('h'), KeyModifiers::CONTROL, "focus_left"),
            (KeyCode::Char('l'), KeyModifiers::CONTROL, "focus_right"),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::CommandMenuStart)
        ));
    }

    #[test]
    fn routes_provider_login_menu_keys() {
        let manager = event_manager();
        let model = model_with_buffer(Context::ProviderLogin);

        for (key, modifiers, action) in [
            (KeyCode::Down, KeyModifiers::NONE, GeneralAction::Next),
            (KeyCode::Up, KeyModifiers::NONE, GeneralAction::Previous),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        for (key, modifiers, action) in [
            (KeyCode::Tab, KeyModifiers::NONE, GeneralAction::FocusNext),
            (
                KeyCode::Char('v'),
                KeyModifiers::CONTROL,
                GeneralAction::Paste,
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
    }

    #[test]
    fn routes_agent_panel_input_keys() {
        let manager = event_manager();
        let mut model = Model {
            current_focus: crate::data_object::FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(crate::tools::ToolKind::Agent);
        model.tools.visibility = crate::data_object::Visibility::Displayed;

        for (key, command) in [
            (KeyCode::Char('i'), "agent_insert"),
            (KeyCode::Char('?'), "agent_help"),
            (KeyCode::Esc, "agent_panel_normal"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        for (key, command) in [
            (KeyCode::Char('h'), "focus_left"),
            (KeyCode::Char('l'), "focus_right"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::CONTROL));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        model.agent.enter_insert_mode();
        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert(' ')))
        ));
        for (key, modifiers, action) in [
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        for (key, modifiers, command) in [
            (KeyCode::F(7), KeyModifiers::CONTROL, "agent_interrupt"),
            (KeyCode::Tab, KeyModifiers::NONE, "agent_command_quick_fill"),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
        for key in [KeyCode::Left, KeyCode::Right] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::TextInput(TextInputEvent::Cursor(_)))
            ));
        }
        for key in [KeyCode::Char('h'), KeyCode::Char('l')] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::CONTROL));
            assert!(manager.translate(event, &model).is_none());
        }

        model.agent.enter_normal_mode();
        assert!(model.modal.open(Context::AgentChatHelp));
        for (key, command) in [
            (KeyCode::Char('?'), "agent_help"),
            (KeyCode::Esc, "agent_panel_normal"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert!(manager.translate(event, &model).is_none());
    }

    #[test]
    fn agent_normal_mode_routes_leader_commands_without_changing_focus_first() {
        let events = event_manager();
        let messages = MessageManager::default();
        let mut model = Model {
            current_focus: FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(crate::tools::ToolKind::Agent);
        model.tools.visibility = crate::data_object::Visibility::Displayed;

        dispatch_key(&events, &messages, &mut model, KeyCode::Char(' '));
        assert!(model.command_menu.is_active());
        assert_eq!(model.current_focus, FocusTarget::ToolsPanel);

        for key in [KeyCode::Char('a'), KeyCode::Char('t')] {
            dispatch_key(&events, &messages, &mut model, key);
        }
        assert!(!model.command_menu.is_active());
        assert_eq!(
            model.tools.visibility,
            crate::data_object::Visibility::Hidden
        );
        assert_eq!(model.current_focus, FocusTarget::Editor);
    }

    #[test]
    fn leader_ai_login_opens_the_existing_provider_workflow() {
        let events = event_manager();
        let messages = MessageManager::default();
        let mut model = Model::default();

        for key in [KeyCode::Char(' '), KeyCode::Char('a'), KeyCode::Char('i')] {
            dispatch_key(&events, &messages, &mut model, key);
        }

        assert!(model.provider_login.is_open());
        assert!(model.modal.is_active(Context::ProviderLogin));
        assert!(!model.command_menu.is_active());
    }

    #[test]
    fn leader_ai_logout_opens_the_provider_logout_workflow() {
        let events = event_manager();
        let messages = MessageManager::default();
        let (store, _directory) = crate::test_utils::provider_credential_store();
        let mut model = Model {
            provider_credentials: Some(store),
            ..Model::default()
        };

        for key in [KeyCode::Char(' '), KeyCode::Char('a'), KeyCode::Char('o')] {
            dispatch_key(&events, &messages, &mut model, key);
        }

        assert!(model.modal.is_active(Context::ProviderLogout));
        assert!(!model.command_menu.is_active());
    }

    #[test]
    fn routes_vcs_reset_menu_and_popup_keys() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::VcsResetMenu);
        for (key, command) in [
            (KeyCode::Esc, "vcs_reset_menu_close"),
            (KeyCode::Char('s'), "vcs_reset_soft"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        assert_eq!(model.context_stack.pop(), Some(Context::VcsResetMenu));
        assert!(model.modal.open(Context::VcsReset));
        for (key, action) in [
            (KeyCode::Down, GeneralAction::Next),
            (KeyCode::Up, GeneralAction::Previous),
            (KeyCode::Enter, GeneralAction::Activate),
            (KeyCode::Esc, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('a')))
        ));
    }

    #[test]
    fn vcs_selectors_compose_list_navigation() {
        let manager = event_manager();
        for context in [Context::VcsBranch, Context::VcsMerge, Context::VcsReset] {
            let mut model = Model::default();
            assert!(model.modal.open(context));
            for (key, action) in [
                (KeyCode::Char('n'), GeneralAction::Next),
                (KeyCode::Char('p'), GeneralAction::Previous),
            ] {
                let event = Event::Key(KeyEvent::new(key, KeyModifiers::CONTROL));
                assert!(matches!(
                    manager.translate(event, &model),
                    Some(Message::GeneralAction(actual)) if actual == action
                ));
            }
        }
    }

    #[test]
    fn routes_vcs_branch_menu_and_popup_keys() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::VcsBranchMenu);
        for (key, command) in [
            (KeyCode::Esc, "vcs_branch_menu_close"),
            (KeyCode::Char('c'), "vcs_branch_create"),
            (KeyCode::Char('s'), "vcs_branch_switch"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        assert_eq!(model.context_stack.pop(), Some(Context::VcsBranchMenu));
        assert!(model.modal.open(Context::VcsBranch));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));
        for (key, action) in [
            (KeyCode::Down, GeneralAction::Next),
            (KeyCode::Up, GeneralAction::Previous),
            (KeyCode::Enter, GeneralAction::Activate),
            (KeyCode::Esc, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
    }

    #[test]
    fn routes_vcs_commit_page_keys() {
        let manager = event_manager();
        let mut model = Model {
            active_page: AppPage::VcsCommit,
            ..Model::default()
        };
        for (key, modifiers, command) in [
            (KeyCode::Char('q'), KeyModifiers::NONE, "vcs_commit_close"),
            (KeyCode::Char('i'), KeyModifiers::NONE, "enter_insert_mode"),
            (KeyCode::Char('x'), KeyModifiers::NONE, "delete_grapheme"),
            (
                KeyCode::Char('d'),
                KeyModifiers::NONE,
                "delete_line_pending",
            ),
            (
                KeyCode::Char('c'),
                KeyModifiers::NONE,
                "change_line_pending",
            ),
            (KeyCode::Char('y'), KeyModifiers::NONE, "yank_line_pending"),
            (KeyCode::Char('p'), KeyModifiers::NONE, "paste_after"),
            (KeyCode::Char('P'), KeyModifiers::SHIFT, "paste_before"),
            (
                KeyCode::Char('s'),
                KeyModifiers::CONTROL,
                "vcs_commit_submit",
            ),
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                "vcs_commit_discard",
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        model
            .vcs
            .commit
            .editor
            .insert_document(editor::DocumentFactory::new().create(""));
        model.vcs.commit.editor.enter_insert_mode();
        for (key, modifiers, command) in [
            (KeyCode::Esc, KeyModifiers::NONE, "exit_insert_mode"),
            (KeyCode::Enter, KeyModifiers::NONE, "insert_newline"),
            (KeyCode::Backspace, KeyModifiers::NONE, "backspace"),
            (
                KeyCode::Char('s'),
                KeyModifiers::CONTROL,
                "vcs_commit_submit",
            ),
            (
                KeyCode::Char('c'),
                KeyModifiers::CONTROL,
                "vcs_commit_discard",
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));
    }

    #[test]
    fn normal_s_routes_to_visible_buffer_jump() {
        let manager = event_manager();
        let model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("buffer_jump_start")))
        ));
    }

    #[test]
    fn routes_buffer_jump_text_and_commands_in_its_context() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::BufferJump));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('z')))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(manager.translate(event, &model).is_none());
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("buffer_jump_cancel")))
        ));
    }

    #[test]
    fn routes_buffer_grep_input_selection_acceptance_and_cancellation() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::BufferGrep));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('e')))
        ));
        for (key, modifiers, action) in [
            (KeyCode::Down, KeyModifiers::NONE, GeneralAction::Next),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (KeyCode::Up, KeyModifiers::NONE, GeneralAction::Previous),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
    }

    #[test]
    fn routes_project_grep_input_selection_acceptance_and_cancellation() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::ProjectGrep));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('e')))
        ));
        for (key, modifiers, action) in [
            (KeyCode::Down, KeyModifiers::NONE, GeneralAction::Next),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (KeyCode::Up, KeyModifiers::NONE, GeneralAction::Previous),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
    }

    #[test]
    fn routes_yank_commands_and_pending_cancellation() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("yank_line_pending")))
        ));

        model.context_stack.push(Context::EditorYank);
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("yank_cancel")))
        ));
        assert_eq!(model.context_stack.pop(), Some(Context::EditorYank));
        model.editor.enter_visual_mode();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("yank_selection")))
        ));
    }

    #[test]
    fn routes_delete_change_commands_and_pending_cancellation() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);
        for (character, command) in [
            ('x', "delete_grapheme"),
            ('d', "delete_line_pending"),
            ('c', "change_line_pending"),
        ] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        model.context_stack.push(Context::EditorChange);
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("change_cancel")))
        ));
        assert_eq!(model.context_stack.pop(), Some(Context::EditorChange));
        model.editor.enter_visual_mode();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("delete_selection_d")))
        ));
    }

    #[test]
    fn routes_visual_entry_motion_and_exit() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("enter_visual_mode")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('V'), KeyModifiers::SHIFT));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("enter_visual_line_mode")))
        ));

        model.editor.enter_visual_mode();
        let event = Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("move_left")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("exit_visual_mode")))
        ));
    }

    #[test]
    fn line_boundary_bindings_do_not_consume_count_digits() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);

        for (character, modifiers, command) in [
            ('0', KeyModifiers::NONE, "move_line_start"),
            ('$', KeyModifiers::NONE, "move_line_end"),
            ('$', KeyModifiers::SHIFT, "move_line_end"),
        ] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        model.context_stack.push(Context::LineJump);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('0'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('0')))
        ));
    }

    #[test]
    fn routes_view_alignment_prefix_second_key_and_escape() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("view_align_start")))
        ));

        model.context_stack.push(Context::ViewAlign);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('z')))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("view_align_cancel")))
        ));
    }

    #[test]
    fn routes_line_jump_text_and_escape() {
        let manager = event_manager();
        let mut model = model_with_buffer(Context::AppNormal);

        for (character, modifiers) in [('2', KeyModifiers::NONE), ('G', KeyModifiers::SHIFT)] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(character), modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::TextInput(TextInputEvent::Insert(actual))) if actual == character
            ));
        }

        model.context_stack.push(Context::LineJump);
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("line_jump_cancel")))
        ));
    }

    #[test]
    fn translates_page_scroll_control_chords_when_crossterm_delivers_them() {
        let manager = event_manager();
        let model = model_with_buffer(Context::AppNormal);

        for (key, command) in [('f', "page_scroll_down"), ('u', "page_scroll_up")] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
    }

    #[test]
    fn configured_bindings_still_take_precedence() {
        let manager = event_manager();
        let normal_model = model_with_buffer(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &normal_model),
            Some(Message::InvokeCommand(CommandId("move_left")))
        ));

        let mut insert_model = Model::default();
        crate::test_utils::create_buffer(&mut insert_model, "text");
        insert_model.editor.enter_insert_mode();
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &insert_model),
            Some(Message::InvokeCommand(CommandId("exit_insert_mode")))
        ));
    }

    #[test]
    fn routes_code_navigation_location_keys() {
        let manager = event_manager();
        let mut model = Model::default();
        assert!(model.modal.open(Context::CodeNavigation));

        for (key, modifiers, action) in [
            (KeyCode::Enter, KeyModifiers::NONE, GeneralAction::Activate),
            (KeyCode::Down, KeyModifiers::NONE, GeneralAction::Next),
            (KeyCode::Up, KeyModifiers::NONE, GeneralAction::Previous),
            (KeyCode::Esc, KeyModifiers::NONE, GeneralAction::Back),
            (KeyCode::Char('j'), KeyModifiers::NONE, GeneralAction::Next),
            (
                KeyCode::Char('k'),
                KeyModifiers::NONE,
                GeneralAction::Previous,
            ),
            (
                KeyCode::Char('n'),
                KeyModifiers::CONTROL,
                GeneralAction::Next,
            ),
            (
                KeyCode::Char('p'),
                KeyModifiers::CONTROL,
                GeneralAction::Previous,
            ),
        ] {
            let event = Event::Key(KeyEvent::new(key, modifiers));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::GeneralAction(actual)) if actual == action
            ));
        }
    }

    #[test]
    fn routes_explorer_keys_in_explorer_context() {
        let manager = event_manager();
        let model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };

        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("open_selected")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::CommandMenuStart)
        ));
    }

    #[test]
    fn routes_explorer_help_and_file_operation_keys() {
        let manager = event_manager();
        let mut model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };

        for (key, command) in [
            (KeyCode::Char('?'), "explorer_help"),
            (KeyCode::Char('a'), "explorer_add"),
            (KeyCode::Char('d'), "explorer_delete"),
            (KeyCode::Char('m'), "explorer_move"),
            (KeyCode::Char('c'), "explorer_copy"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }

        assert!(model.modal.open(Context::ExplorerHelp));
        for (key, command) in [
            (KeyCode::Esc, "close_explorer_help"),
            (KeyCode::Char('?'), "explorer_help"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(manager.translate(event, &model).is_none());

        assert!(
            model
                .modal
                .replace(Context::ExplorerHelp, Context::ExplorerAdd)
        );
        for (key, command) in [
            (KeyCode::Enter, "explorer_add_submit"),
            (KeyCode::Esc, "close_explorer_add"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));

        assert!(
            model
                .modal
                .replace(Context::ExplorerAdd, Context::ExplorerDelete)
        );
        for (key, command) in [
            (KeyCode::Char('y'), "explorer_delete_confirm"),
            (KeyCode::Esc, "close_explorer_delete"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(manager.translate(event, &model).is_none());

        assert!(
            model
                .modal
                .replace(Context::ExplorerDelete, Context::ExplorerTransfer)
        );
        for (key, command) in [
            (KeyCode::Enter, "explorer_transfer_submit"),
            (KeyCode::Esc, "close_explorer_transfer"),
        ] {
            let event = Event::Key(KeyEvent::new(key, KeyModifiers::NONE));
            assert!(matches!(
                manager.translate(event, &model),
                Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
            ));
        }
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::DeleteBackward))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(TextInputEvent::Insert('x')))
        ));
    }

    #[test]
    fn shift_i_toggles_ignored_files_only_in_explorer_context() {
        let manager = event_manager();
        let event = || Event::Key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT));
        let mut model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };
        assert!(matches!(
            manager.translate(event(), &model),
            Some(Message::InvokeCommand(CommandId("toggle_ignored_files")))
        ));

        model.current_focus = FocusTarget::Editor;
        crate::test_utils::create_buffer(&mut model, "text");
        assert!(matches!(
            manager.translate(event(), &model),
            Some(Message::InvokeCommand(CommandId(
                "insert_at_first_nonblank"
            )))
        ));

        model.editor.enter_insert_mode();
        assert!(matches!(
            manager.translate(event(), &model),
            Some(Message::TextInput(TextInputEvent::Insert('I')))
        ));
    }

    #[test]
    fn routes_directional_focus_in_normal_contexts() {
        let manager = event_manager();
        let directions = [
            ('h', "focus_left"),
            ('j', "focus_down"),
            ('k', "focus_up"),
            ('l', "focus_right"),
        ];

        for context in [Context::AppNormal, Context::Explorer] {
            let model = model_with_buffer(context);
            for (key, command) in directions {
                let event = Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL));
                assert!(matches!(
                    manager.translate(event, &model),
                    Some(Message::InvokeCommand(actual)) if actual == CommandId(command)
                ));
            }
        }
    }

    #[test]
    fn insert_context_has_no_directional_focus_bindings() {
        let manager = event_manager();
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "text");
        model.editor.enter_insert_mode();

        for key in ['h', 'j', 'k', 'l', 'n', 'p'] {
            let event = Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::CONTROL));
            assert!(manager.translate(event, &model).is_none());
        }
    }

    #[test]
    fn translates_resize_events() {
        let manager = event_manager();
        let model = Model::default();

        assert!(matches!(
            manager.translate(Event::Resize(100, 40), &model),
            Some(Message::Resize(100, 40))
        ));
        assert!(matches!(
            manager.translate(Event::FocusLost, &model),
            Some(Message::FocusLost)
        ));
    }
}
