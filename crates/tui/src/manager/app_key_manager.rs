use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::data_object::Message;

/// Handles keys while the app is in vim-style normal mode.
#[derive(Debug, Default)]
pub struct AppKeyManager;

impl AppKeyManager {
    pub fn handle_key(&self, key: KeyEvent) -> Option<Message> {
        match key.code {
            KeyCode::Char('q') => Some(Message::Quit),
            KeyCode::Char('i') => Some(Message::EnterInsertMode),
            _ => None,
        }
    }
}
