use crossterm::event::KeyCode;

use crate::{
    data_object::CommandId,
    manager::{
        context::{InputAction, KeyChord},
        keymap_rule::KeymapRule,
    },
};

/// Insert-mode commands shared by the main editor and commit scratch editor.
#[derive(Debug, Default)]
pub struct InsertEditingKeymap;

impl KeymapRule for InsertEditingKeymap {
    fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        let command = match (key.code, key.modifiers) {
            (KeyCode::Esc, modifiers) if modifiers.is_empty() => "exit_insert_mode",
            (KeyCode::Backspace, modifiers) if modifiers.is_empty() => "backspace",
            (KeyCode::Enter, modifiers) if modifiers.is_empty() => "insert_newline",
            (KeyCode::Left, modifiers) if modifiers.is_empty() => "move_left",
            (KeyCode::Right, modifiers) if modifiers.is_empty() => "move_right",
            (KeyCode::Up, modifiers) if modifiers.is_empty() => "move_up",
            (KeyCode::Down, modifiers) if modifiers.is_empty() => "move_down",
            _ => return None,
        };
        Some(InputAction::Command(CommandId(command)))
    }
}

/// Vim-style cursor motions shared by Normal, Visual, and commit editing.
#[derive(Debug, Default)]
pub struct MotionsKeymap;

impl KeymapRule for MotionsKeymap {
    fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        if !key.modifiers.is_empty() {
            return None;
        }
        let command = match key.code {
            KeyCode::Char('h') => "move_left",
            KeyCode::Char('l') => "move_right",
            KeyCode::Char('k') => "move_up",
            KeyCode::Char('j') => "move_down",
            KeyCode::Char('0') => "move_line_start",
            KeyCode::Char('$') => "move_line_end",
            KeyCode::Char('w') => "move_next_word_start",
            KeyCode::Char('b') => "move_previous_word_start",
            KeyCode::Char('e') => "move_next_word_end",
            _ => return None,
        };
        Some(InputAction::Command(CommandId(command)))
    }
}
