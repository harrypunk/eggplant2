use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    data_object::GeneralAction,
    manager::{
        context::{InputAction, KeyChord},
        keymap_rule::KeymapRule,
    },
};

/// Vertical list navigation shared by pickers, menus, and location lists.
#[derive(Debug, Clone, Copy)]
pub struct ListNavigationKeymap {
    vim_keys: bool,
}

impl ListNavigationKeymap {
    pub const fn new() -> Self {
        Self { vim_keys: false }
    }

    /// Opt in only where `j`/`k` cannot be printable text input.
    pub const fn with_vim_keys() -> Self {
        Self { vim_keys: true }
    }

    fn action(self, key: KeyChord) -> Option<GeneralAction> {
        match (key.code, key.modifiers) {
            (KeyCode::Down, modifiers) if modifiers.is_empty() => Some(GeneralAction::Next),
            (KeyCode::Up, modifiers) if modifiers.is_empty() => Some(GeneralAction::Previous),
            (KeyCode::Char('n'), KeyModifiers::CONTROL) => Some(GeneralAction::Next),
            (KeyCode::Char('p'), KeyModifiers::CONTROL) => Some(GeneralAction::Previous),
            (KeyCode::Char('j'), modifiers) if self.vim_keys && modifiers.is_empty() => {
                Some(GeneralAction::Next)
            }
            (KeyCode::Char('k'), modifiers) if self.vim_keys && modifiers.is_empty() => {
                Some(GeneralAction::Previous)
            }
            _ => None,
        }
    }
}

impl Default for ListNavigationKeymap {
    fn default() -> Self {
        Self::new()
    }
}

impl KeymapRule for ListNavigationKeymap {
    fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        self.action(key).map(InputAction::General)
    }
}
