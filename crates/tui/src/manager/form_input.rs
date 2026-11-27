use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    data_object::GeneralAction,
    manager::{
        context::{InputAction, KeyChord},
        keymap_rule::KeymapRule,
    },
};

/// Form-level focus and clipboard behavior shared through `FormActions`.
#[derive(Debug, Default)]
pub struct FormActionsKeymap;

impl KeymapRule for FormActionsKeymap {
    fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        let action = match (key.code, key.modifiers) {
            (KeyCode::Tab, modifiers) if modifiers.is_empty() => GeneralAction::FocusNext,
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => GeneralAction::Paste,
            _ => return None,
        };
        Some(InputAction::General(action))
    }
}
