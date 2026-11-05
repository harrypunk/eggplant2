use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::crossterm::event::{self, Event};

use crate::config::Config;
use crate::data_object::{Context, Message, Model};
use crate::manager::{context::KeyChord, keyboard_input_registry::Registry};

#[derive(Debug)]
pub struct EventManager {
    poll_timeout: Duration,
    keymap: Registry,
}

impl EventManager {
    pub fn new(config: &Config, keymap: Registry) -> Self {
        Self {
            poll_timeout: Duration::from_millis(config.event_poll_timeout),
            keymap,
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
                self.keymap
                    .get_binding(ctx)
                    .and_then(|bindings| bindings.resolve(KeyChord::new(key.code, key.modifiers)))
                    .or_else(|| Self::unbound_key_message(ctx, key))
            }
            Event::Resize(width, height) => Some(Message::Resize(width, height)),
            // Event::Mouse(mouse) => MouseManager.translate(mouse, model),
            _ => None,
        }
    }

    fn unbound_key_message(context: &Context, key: KeyEvent) -> Option<Message> {
        if context != &Context::EditorInput {
            return None;
        }

        match key.code {
            KeyCode::Char(character)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                Some(Message::InsertChar(character))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use editor::CursorMovement;

    use super::*;

    fn event_manager() -> EventManager {
        let config = Config::default();
        let keymap = Registry::from_config(&config.keymap).unwrap();
        EventManager::new(&config, keymap)
    }

    #[test]
    fn insert_context_falls_back_to_printable_characters() {
        let manager = event_manager();
        let mut model = Model::default();
        model.context_stack.push(Context::EditorInput);

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::InsertChar('x'))
        ));

        let event = Event::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
        assert!(manager.translate(event, &model).is_none());
    }

    #[test]
    fn configured_bindings_still_take_precedence() {
        let manager = event_manager();
        let mut normal_model = Model::default();
        normal_model.context_stack.push(Context::AppNormal);
        let event = Event::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &normal_model),
            Some(Message::MoveCursor(CursorMovement::Left))
        ));

        let mut insert_model = Model::default();
        insert_model.context_stack.push(Context::EditorInput);
        let event = Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(
            manager.translate(event, &insert_model),
            Some(Message::ExitInsertMode)
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
            Some(Message::OpenSelected)
        ));
        let event = Event::Key(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL));
        assert!(matches!(
            manager.translate(event, &model),
            Some(Message::ToggleExplorerFocus)
        ));
    }

    #[test]
    fn translates_resize_events() {
        let manager = event_manager();
        let model = Model::default();

        assert!(matches!(
            manager.translate(Event::Resize(100, 40), &model),
            Some(Message::Resize(100, 40))
        ));
    }
}
