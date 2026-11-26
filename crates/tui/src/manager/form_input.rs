use crossterm::event::{KeyCode, KeyModifiers};

use crate::{data_object::GeneralAction, manager::context::KeyChord};

/// Form-level semantic actions composed by `KeymapCapability::FormActions`.
pub const FORM_GENERAL_ACTIONS: [(KeyChord, GeneralAction); 2] = [
    (
        KeyChord::new(KeyCode::Tab, KeyModifiers::NONE),
        GeneralAction::FocusNext,
    ),
    (
        KeyChord::new(KeyCode::Char('v'), KeyModifiers::CONTROL),
        GeneralAction::Paste,
    ),
];
