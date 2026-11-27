use crossterm::event::KeyCode;

use crate::{
    data_object::GeneralAction,
    manager::{
        context::{InputAction, KeyChord},
        keymap_rule::KeymapRule,
    },
};

/// Acceptance and cancellation for modal workflows.
#[derive(Debug, Default)]
pub struct ModalActionsKeymap;

impl KeymapRule for ModalActionsKeymap {
    fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        let action = match (key.code, key.modifiers) {
            (KeyCode::Enter, modifiers) if modifiers.is_empty() => GeneralAction::Activate,
            (KeyCode::Esc, modifiers) if modifiers.is_empty() => GeneralAction::Back,
            _ => return None,
        };
        Some(InputAction::General(action))
    }
}
