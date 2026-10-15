use std::time::Duration;

use ratatui::crossterm::event::{self, Event, KeyEvent, KeyEventKind};

use crate::data_object::{EditMode, Message, Model};
use crate::manager::{app_key_manager::AppKeyManager, edit_key_manager::EditKeyManager};

const POLL_TIMEOUT: Duration = Duration::from_millis(250);

#[derive(Debug, Default)]
pub struct EventManager;

impl EventManager {
    /// Wait for the next terminal event and translate it into a [`Message`].
    pub fn next_message(&self, model: &Model) -> color_eyre::Result<Option<Message>> {
        Ok(Self::read_event()?.and_then(|event| Self::translate(event, model)))
    }

    /// Raw I/O: poll crossterm for the next event.
    fn read_event() -> color_eyre::Result<Option<Event>> {
        if event::poll(POLL_TIMEOUT)? {
            return Ok(Some(event::read()?));
        }
        Ok(None)
    }

    /// Pure routing: dispatch on the event kind.
    fn translate(event: Event, model: &Model) -> Option<Message> {
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => Self::translate_key(key, model),
            // Event::Mouse(mouse) => MouseManager.translate(mouse, model),
            // Event::Resize(..)   => Some(Message::Redraw),
            _ => None,
        }
    }

    /// Route key events by the app's current edit mode.
    fn translate_key(key: KeyEvent, model: &Model) -> Option<Message> {
        match model.edit_mode {
            EditMode::Normal => AppKeyManager.handle_key(key),
            EditMode::Insert => EditKeyManager.handle_key(key),
        }
    }
}
