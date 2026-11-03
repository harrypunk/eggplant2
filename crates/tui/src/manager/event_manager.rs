use std::time::Duration;

use crossterm::event::KeyEventKind;
use ratatui::crossterm::event::{self, Event};

use crate::config::Config;
use crate::data_object::{Message, Model};
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
        let ctx = model.context_stack.last()?;
        match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                let current_bindings = self.keymap.get_binding(ctx)?;
                current_bindings.resolve(KeyChord::new(key.code, key.modifiers))
            }
            // Event::Mouse(mouse) => MouseManager.translate(mouse, model),
            // Event::Resize(..)   => Some(Message::Redraw),
            _ => None,
        }
    }
}
