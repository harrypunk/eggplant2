use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::data_object::Message;

/// Handles keys while the app is in vim-style insert mode.
#[derive(Debug, Default)]
pub struct EditKeyManager;

impl EditKeyManager {
    pub fn handle_key(&self, key: KeyEvent) -> Option<Message> {
        match key.code {
            KeyCode::Esc => Some(Message::ExitInsertMode),
            // todo: forward text input to the focused editor component
            _ => None,
        }
    }
}
