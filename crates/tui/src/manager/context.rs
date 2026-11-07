use std::{collections::HashMap, str::FromStr};

use crossterm::event::{KeyCode, KeyModifiers};

use crate::data_object::{Context, Message};

/// A key press used for binding lookup, independent of terminal event metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyChord {
    pub const fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }
}

impl FromStr for KeyChord {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split('-').collect::<Vec<_>>();
        let key_name = parts
            .pop()
            .filter(|part| !part.is_empty())
            .ok_or_else(|| format!("invalid key binding `{value}`"))?;

        let mut modifiers = KeyModifiers::NONE;
        for modifier in parts {
            modifiers |= match modifier.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => KeyModifiers::CONTROL,
                "alt" => KeyModifiers::ALT,
                "shift" => KeyModifiers::SHIFT,
                _ => {
                    return Err(format!(
                        "unknown modifier `{modifier}` in key binding `{value}`"
                    ));
                }
            };
        }

        let code = normalize_shifted_character(parse_key_code(key_name)?, modifiers);
        Ok(Self::new(code, modifiers))
    }
}

fn normalize_shifted_character(code: KeyCode, modifiers: KeyModifiers) -> KeyCode {
    match code {
        KeyCode::Char(character) if modifiers.contains(KeyModifiers::SHIFT) => {
            KeyCode::Char(character.to_ascii_uppercase())
        }
        _ => code,
    }
}

fn parse_key_code(value: &str) -> Result<KeyCode, String> {
    let normalized = value.to_ascii_lowercase();
    let code = match normalized.as_str() {
        "esc" | "escape" => KeyCode::Esc,
        "enter" => KeyCode::Enter,
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "page_up" => KeyCode::PageUp,
        "pagedown" | "page_down" => KeyCode::PageDown,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        _ => {
            if let Some(number) = normalized.strip_prefix('f')
                && let Ok(number) = number.parse::<u8>()
                && (1..=12).contains(&number)
            {
                return Ok(KeyCode::F(number));
            }

            let mut chars = value.chars();
            match (chars.next(), chars.next()) {
                (Some(character), None) => KeyCode::Char(character),
                _ => {
                    return Err(format!(
                        "unknown key `{value}`; expected a single character or named key"
                    ));
                }
            }
        }
    };

    Ok(code)
}

type KeyCommandMap = HashMap<KeyChord, Message>;

/// Keybindings associated with one input context.
#[derive(Debug, Clone)]
pub struct ContextBindings {
    key_command_map: KeyCommandMap,
    pub context: Context,
}

impl ContextBindings {
    pub fn from_bindings(
        context: Context,
        bindings: impl IntoIterator<Item = (KeyChord, Message)>,
    ) -> Self {
        Self {
            key_command_map: bindings.into_iter().collect(),
            context,
        }
    }

    pub fn resolve(&self, key: KeyChord) -> Option<Message> {
        self.key_command_map.get(&key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_character_and_named_keys() {
        assert_eq!(
            "q".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE)
        );
        assert_eq!(
            "esc".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Esc, KeyModifiers::NONE)
        );
        assert_eq!(
            "space".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Char(' '), KeyModifiers::NONE)
        );
    }

    #[test]
    fn parses_modifiers() {
        assert_eq!(
            "ctrl-alt-i".parse::<KeyChord>().unwrap(),
            KeyChord::new(
                KeyCode::Char('i'),
                KeyModifiers::CONTROL | KeyModifiers::ALT
            )
        );
    }

    #[test]
    fn shifted_ascii_characters_match_terminal_key_events() {
        assert_eq!(
            "shift-i".parse::<KeyChord>().unwrap(),
            KeyChord::new(KeyCode::Char('I'), KeyModifiers::SHIFT)
        );
    }

    #[test]
    fn rejects_invalid_keys() {
        assert!("ctrl-".parse::<KeyChord>().is_err());
        assert!("hyper-q".parse::<KeyChord>().is_err());
        assert!("meta-q".parse::<KeyChord>().is_err());
        assert!("not-a-key".parse::<KeyChord>().is_err());
    }
}
