use std::{collections::HashMap, str::FromStr, sync::Arc};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::data_object::{CommandId, Context, GeneralAction};
use crate::manager::keymap_rule::KeymapRule;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputAction {
    Command(CommandId),
    General(GeneralAction),
}

impl From<CommandId> for InputAction {
    fn from(id: CommandId) -> Self {
        Self::Command(id)
    }
}

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

    /// Normalize terminal-specific SHIFT metadata for symbolic characters.
    pub fn shifted_symbol(self) -> Option<Self> {
        let KeyCode::Char(character) = self.code else {
            return None;
        };
        if character.is_alphabetic() || !self.modifiers.contains(KeyModifiers::SHIFT) {
            return None;
        }
        Some(Self::new(self.code, self.modifiers & !KeyModifiers::SHIFT))
    }
}

impl From<KeyEvent> for KeyChord {
    fn from(key: KeyEvent) -> Self {
        Self::new(key.code, key.modifiers)
    }
}

impl From<KeyCode> for KeyChord {
    fn from(code: KeyCode) -> Self {
        Self::new(code, KeyModifiers::NONE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderInput {
    Start,
    Pending,
}

/// Decide whether a chord starts or continues the configured leader workflow.
pub fn leader_input(
    key: KeyChord,
    leader: KeyChord,
    active: bool,
    enabled: bool,
) -> Option<LeaderInput> {
    match (active, key == leader && enabled) {
        (true, _) => Some(LeaderInput::Pending),
        (false, true) => Some(LeaderInput::Start),
        (false, false) => None,
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

        // Combine every modifier prefix before the final key name, such as ctrl-alt-i.
        let modifiers = parts
            .into_iter()
            .try_fold(KeyModifiers::NONE, |modifiers, modifier| {
                let flag = match modifier.to_ascii_lowercase().as_str() {
                    "ctrl" | "control" => KeyModifiers::CONTROL,
                    "alt" => KeyModifiers::ALT,
                    "shift" => KeyModifiers::SHIFT,
                    _ => {
                        return Err(format!(
                            "unknown modifier `{modifier}` in key binding `{value}`"
                        ));
                    }
                };
                Ok(modifiers | flag)
            })?;

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

type KeyActionMap = HashMap<KeyChord, InputAction>;

/// Keybindings associated with one input context.
#[derive(Debug, Clone)]
pub struct ContextBindings {
    key_action_map: KeyActionMap,
    pub context: Context,
    rules: Vec<Arc<dyn KeymapRule>>,
}

impl ContextBindings {
    pub fn from_bindings(
        context: Context,
        bindings: impl IntoIterator<Item = (KeyChord, CommandId)>,
    ) -> Self {
        Self::from_actions(
            context,
            bindings
                .into_iter()
                .map(|(key, command)| (key, InputAction::Command(command))),
        )
    }

    pub fn from_actions(
        context: Context,
        actions: impl IntoIterator<Item = (KeyChord, InputAction)>,
    ) -> Self {
        Self {
            key_action_map: actions.into_iter().collect(),
            context,
            rules: Vec::new(),
        }
    }

    pub fn with_rule(mut self, rule: impl KeymapRule + 'static) -> Self {
        self.rules.push(Arc::new(rule));
        self
    }

    pub fn resolve(&self, key: KeyChord) -> Option<InputAction> {
        self.key_action_map.get(&key).copied()
    }

    /// Resolve direct bindings before composed behavior rules and compatibility fallbacks.
    pub fn resolve_chord(&self, key: KeyChord) -> Option<InputAction> {
        self.resolve(key)
            .or_else(|| self.rules.iter().find_map(|rule| rule.resolve(key)))
            .or_else(|| {
                key.shifted_symbol().and_then(|shifted| {
                    self.resolve(shifted)
                        .or_else(|| self.rules.iter().find_map(|rule| rule.resolve(shifted)))
                })
            })
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
    fn leader_routing_uses_the_configured_chord_capability_and_pending_state() {
        let leader = KeyChord::from(KeyCode::Char(','));
        assert_eq!(
            leader_input(leader, leader, false, true),
            Some(LeaderInput::Start)
        );
        assert_eq!(leader_input(leader, leader, false, false), None);
        assert_eq!(
            leader_input(KeyChord::from(KeyCode::Esc), leader, true, false),
            Some(LeaderInput::Pending)
        );
        assert_eq!(
            leader_input(KeyChord::from(KeyCode::Char(' ')), leader, false, true),
            None
        );
    }

    #[test]
    fn list_navigation_is_a_composed_rule_with_direct_precedence() {
        use crate::manager::list_navigation::ListNavigationKeymap;

        let next = CommandId("next");
        let ctrl_n = KeyChord::new(KeyCode::Char('n'), KeyModifiers::CONTROL);
        let plain = ContextBindings::from_bindings(
            Context::AppNormal,
            [(KeyChord::from(KeyCode::Down), next)],
        );
        assert_eq!(plain.resolve_chord(ctrl_n), None);

        let list = plain.with_rule(ListNavigationKeymap::new());
        assert_eq!(
            list.resolve_chord(KeyCode::Down.into()),
            Some(InputAction::Command(next))
        );
        assert_eq!(
            list.resolve_chord(ctrl_n),
            Some(InputAction::General(GeneralAction::Next))
        );
        assert_eq!(
            list.resolve_chord(KeyChord::new(KeyCode::Char('p'), KeyModifiers::CONTROL)),
            Some(InputAction::General(GeneralAction::Previous))
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
