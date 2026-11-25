use std::sync::Arc;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    component::text_input::{TextCursorMovement, TextInputEvent},
    data_object::{Context, Message},
    manager::{
        context::KeyChord, input_resolver::InputResolver, text_input_registry::TextInputRegistry,
    },
};

/// Physical-key to semantic-text-event translation, independent of feature routing.
pub trait TextInputKeymap: std::fmt::Debug + Send + Sync {
    fn resolve(&self, key: KeyChord) -> Option<TextInputEvent>;
}

/// Conventional terminal text-editing keys shared by every full text input.
#[derive(Debug, Default)]
pub struct TerminalTextInputKeymap;

impl TextInputKeymap for TerminalTextInputKeymap {
    fn resolve(&self, key: KeyChord) -> Option<TextInputEvent> {
        match (key.code, key.modifiers) {
            (KeyCode::Left, _) => Some(TextInputEvent::Cursor(TextCursorMovement::Left)),
            (KeyCode::Right, _) => Some(TextInputEvent::Cursor(TextCursorMovement::Right)),
            (KeyCode::Backspace, _) => Some(TextInputEvent::DeleteBackward),
            (KeyCode::Char('a'), modifiers) if modifiers == KeyModifiers::CONTROL => {
                Some(TextInputEvent::Cursor(TextCursorMovement::LineStart))
            }
            (KeyCode::Char('e'), modifiers) if modifiers == KeyModifiers::CONTROL => {
                Some(TextInputEvent::Cursor(TextCursorMovement::LineEnd))
            }
            (KeyCode::Char('u'), modifiers) if modifiers == KeyModifiers::CONTROL => {
                Some(TextInputEvent::DeleteToLineStart)
            }
            (KeyCode::Char(character), modifiers)
                if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT =>
            {
                Some(TextInputEvent::Insert(character))
            }
            _ => None,
        }
    }
}

/// Combines physical text-key translation with context capability checks.
#[derive(Debug)]
pub struct TextInputResolver {
    registry: TextInputRegistry,
    keymap: Arc<dyn TextInputKeymap>,
}

impl TextInputResolver {
    pub fn new(registry: TextInputRegistry) -> Self {
        Self {
            registry,
            keymap: Arc::new(TerminalTextInputKeymap),
        }
    }

    pub fn with_keymap(mut self, keymap: Arc<dyn TextInputKeymap>) -> Self {
        self.keymap = keymap;
        self
    }
}

impl InputResolver for TextInputResolver {
    fn resolve(&self, context: Context, key: KeyChord) -> Option<Message> {
        let event = self.keymap.resolve(key)?;
        self.registry
            .accepts(&context, event)
            .then_some(Message::TextInput(event))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_chords_map_to_semantic_line_editing() {
        let keymap = TerminalTextInputKeymap;
        for (character, expected) in [
            ('a', TextInputEvent::Cursor(TextCursorMovement::LineStart)),
            ('e', TextInputEvent::Cursor(TextCursorMovement::LineEnd)),
            ('u', TextInputEvent::DeleteToLineStart),
        ] {
            let chord = KeyChord::new(KeyCode::Char(character), KeyModifiers::CONTROL);
            assert_eq!(keymap.resolve(chord), Some(expected));
        }
        assert_eq!(
            keymap.resolve(KeyChord::new(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(TextInputEvent::Insert('a'))
        );
    }
}
