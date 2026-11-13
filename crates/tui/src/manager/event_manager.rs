use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::event::{self, Event};

use crate::config::Config;
use crate::data_object::{Context, Message, Model};
use crate::manager::{
    context::KeyChord, keyboard_input_registry::Registry, text_input_registry::TextInputRegistry,
};

#[derive(Debug)]
pub struct EventManager {
    poll_timeout: Duration,
    keymap: Registry,
    text_input: TextInputRegistry,
}

impl EventManager {
    pub fn new(config: &Config, keymap: Registry, text_input: TextInputRegistry) -> Self {
        Self {
            poll_timeout: Duration::from_millis(config.event_poll_timeout),
            keymap,
            text_input,
        }
    }

    /// Wait for the next terminal event and translate it into a [`Message`].
    pub fn next_message(&self, model: &Model) -> color_eyre::Result<Option<Message>> {
        Ok(self
            .read_event()?
            .and_then(|event| self.translate(event, model)))
    }

    /// Raw I/O: poll crossterm for the next event.
    fn read_event(&self) -> color_eyre::Result<Option<Event>> {
        if event::poll(self.poll_timeout)? {
            return Ok(Some(event::read()?));
        }
        Ok(None)
    }

    /// Pure routing: dispatch on the event kind.
    fn translate(&self, event: Event, model: &Model) -> Option<Message> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let ctx = model.context_stack.last()?;
                if model.command_menu.is_active() || Self::is_leader_start(ctx, key) {
                    return Some(Message::CommandMenuKey(key.code));
                }
                self.keymap
                    .get_binding(ctx)
                    .and_then(|bindings| bindings.resolve(KeyChord::new(key.code, key.modifiers)))
                    .map(Message::InvokeCommand)
                    .or_else(|| self.unbound_key_message(ctx, key))
            }
            Event::FocusLost => Some(Message::FocusLost),
            Event::Resize(width, height) => Some(Message::Resize(width, height)),
            // Event::Mouse(mouse) => MouseManager.translate(mouse, model),
            _ => None,
        }
    }

    fn is_leader_start(context: &Context, key: KeyEvent) -> bool {
        matches!(context, Context::AppNormal | Context::Explorer)
            && key.code == KeyCode::Char(' ')
            && key.modifiers.is_empty()
    }

    fn unbound_key_message(&self, context: &Context, key: KeyEvent) -> Option<Message> {
        match key.code {
            KeyCode::Char(character)
                if (key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT)
                    && self.text_input.accepts(context) =>
            {
                Some(Message::TextInput(character))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::command_registry::{CommandId, CommandRegistry};

    fn event_manager() -> EventManager {
        let config = Config::default();
        let keymap = Registry::from_config(&config.keymap, &CommandRegistry::default()).unwrap();
        EventManager::new(&config, keymap, TextInputRegistry::builtin())
    }

    #[test]
    fn insert_context_falls_back_to_printable_characters() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::EditorInput);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput('x'))
        ));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert!(manager.translate(event, &model).is_none());

        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(' '))
        ));
    }

    #[test]
    fn routes_picker_input_without_starting_the_leader() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::FilePicker);

        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput(' '))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("close_file_picker")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("file_picker_next")))
        ));
    }

    #[test]
    fn routes_only_theme_picker_navigation_and_actions() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::ThemePicker);

        let translate =
            |key| manager.translate(Event::Key(KeyEvent::new(key, KeyModifiers::NONE)), &model);
        assert!(matches!(
            translate(KeyCode::Down),
            Some(Message::InvokeCommand(CommandId("theme_picker_next")))
        ));
        assert!(matches!(
            translate(KeyCode::Up),
            Some(Message::InvokeCommand(CommandId("theme_picker_previous")))
        ));
        assert!(matches!(
            translate(KeyCode::Enter),
            Some(Message::InvokeCommand(CommandId("apply_theme")))
        ));
        assert!(matches!(
            translate(KeyCode::Esc),
            Some(Message::InvokeCommand(CommandId("close_theme_picker")))
        ));

        for key in [KeyCode::Char('j'), KeyCode::Char('k'), KeyCode::Char(' ')] {
            assert!(translate(key).is_none());
        }
    }

    #[test]
    fn routes_buffer_jump_text_and_commands_in_its_context() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::BufferJump);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::TextInput('z'))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("buffer_jump_backspace")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("buffer_jump_cancel")))
        ));
    }

    #[test]
    fn translates_page_scroll_control_chords_when_crossterm_delivers_them() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

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
        let mut normal_model = Model::default();
        normal_model.context_stack.push(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &normal_model),
            Some(Message::InvokeCommand(CommandId("move_left")))
        ));

        let mut insert_model = Model::default();
        insert_model.context_stack.push(Context::EditorInput);
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &insert_model),
            Some(Message::InvokeCommand(CommandId("exit_insert_mode")))
        ));
    }

    #[test]
    fn routes_explorer_keys_in_explorer_context() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::Explorer);

        let event = Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InvokeCommand(CommandId("open_selected")))
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::CommandMenuKey(KeyCode::Char(' ')))
        ));
    }

    #[test]
    fn shift_i_toggles_ignored_files_only_in_explorer_context() {
        let manager = event_manager();
        let event = || Event::Key(KeyEvent::new(KeyCode::Char('I'), KeyModifiers::SHIFT));
        let mut model = Model::default();
        model.context_stack.push(Context::Explorer);
        assert!(matches!(
            manager.translate(event(), &model),
            Some(Message::InvokeCommand(CommandId("toggle_ignored_files")))
        ));

        model.context_stack.pop();
        model.context_stack.push(Context::AppNormal);
        assert!(manager.translate(event(), &model).is_none());

        model.context_stack.pop();
        model.context_stack.push(Context::EditorInput);
        assert!(matches!(
            manager.translate(event(), &model),
            Some(Message::TextInput('I'))
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
            let mut model = Model::default();
            model.context_stack.push(context);
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
        model.context_stack.push(Context::EditorInput);

        for key in ['h', 'j', 'k', 'l'] {
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
