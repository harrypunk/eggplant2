use std::collections::HashMap;

use color_eyre::eyre::eyre;
use editor::CursorMovement;

use crate::{
    config::KeymapConfig,
    data_object::{Context, Message},
    manager::context::{ContextBindings, KeyChord},
};

type ContextBindingMap = HashMap<Context, ContextBindings>;

/// Validated runtime keybindings grouped by input context.
#[derive(Debug, Default)]
pub struct Registry {
    bindings: ContextBindingMap,
}

impl Registry {
    /// Validate raw configuration and construct the runtime keymap.
    pub fn from_config(config: &KeymapConfig) -> color_eyre::Result<Self> {
        config
            .iter()
            .map(|(context_name, bindings)| {
                parse_context_bindings(context_name, bindings)
                    .map(|bindings| (bindings.context, bindings))
            })
            .collect::<color_eyre::Result<ContextBindingMap>>()
            .map(|bindings| Self { bindings })
    }

    pub fn get_binding(&self, context: &Context) -> Option<&ContextBindings> {
        self.bindings.get(context)
    }
}

fn parse_context_bindings(
    context_name: &str,
    bindings: &HashMap<String, String>,
) -> color_eyre::Result<ContextBindings> {
    let context = parse_context(context_name).ok_or_else(|| {
        eyre!(
            "unknown keymap context `{context_name}`; expected normal, editor_input, explorer, agent_chat, agent_chat_dialog, or vcs"
        )
    })?;

    bindings
        .iter()
        .map(|(action_name, key_name)| parse_binding(context_name, action_name, key_name))
        .collect::<color_eyre::Result<HashMap<KeyChord, Message>>>()
        .map(|bindings| ContextBindings::from_bindings(context, bindings))
}

fn parse_binding(
    context_name: &str,
    action_name: &str,
    key_name: &str,
) -> color_eyre::Result<(KeyChord, Message)> {
    let message = parse_message(action_name).ok_or_else(|| {
        eyre!("unknown action `{action_name}` in keymap context `{context_name}`")
    })?;
    let key = key_name
        .parse::<KeyChord>()
        .map_err(|error| eyre!("invalid key for `{context_name}.{action_name}`: {error}"))?;

    Ok((key, message))
}

fn parse_context(value: &str) -> Option<Context> {
    match value {
        "normal" => Some(Context::AppNormal),
        "agent_chat" => Some(Context::AgentChat),
        "agent_chat_dialog" => Some(Context::AgentChatDialog),
        "vcs" => Some(Context::VCS),
        "editor_input" => Some(Context::EditorInput),
        "explorer" => Some(Context::Explorer),
        _ => None,
    }
}

fn parse_message(value: &str) -> Option<Message> {
    match value {
        "quit" => Some(Message::Quit),
        "enter_insert_mode" => Some(Message::EnterInsertMode),
        "exit_insert_mode" => Some(Message::ExitInsertMode),
        "move_left" => Some(Message::MoveCursor(CursorMovement::Left)),
        "move_right" => Some(Message::MoveCursor(CursorMovement::Right)),
        "move_up" => Some(Message::MoveCursor(CursorMovement::Up)),
        "move_down" => Some(Message::MoveCursor(CursorMovement::Down)),
        "backspace" => Some(Message::Backspace),
        "insert_newline" => Some(Message::InsertNewline),
        "undo" => Some(Message::Undo),
        "redo" => Some(Message::Redo),
        "explorer_next" => Some(Message::ExplorerNext),
        "explorer_previous" => Some(Message::ExplorerPrevious),
        "open_selected" => Some(Message::OpenSelected),
        "explorer_parent" => Some(Message::ExplorerParent),
        "refresh_explorer" => Some(Message::RefreshExplorer),
        "toggle_explorer_focus" => Some(Message::ToggleExplorerFocus),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyModifiers};

    use super::*;
    use crate::config::Config;

    #[test]
    fn loads_default_bindings() {
        let registry = Registry::from_config(&Config::default().keymap).unwrap();

        assert_eq!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            Some(Message::Quit)
        );
        assert_eq!(
            registry
                .get_binding(&Context::EditorInput)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Message::ExitInsertMode)
        );
    }

    #[test]
    fn loads_bindings_with_modifiers() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("quit".to_string(), "ctrl-q".to_string())]),
        )]);
        let registry = Registry::from_config(&config).unwrap();

        assert_eq!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(Message::Quit)
        );
        assert_eq!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn rejects_unknown_context() {
        let config = HashMap::from([("unknown".to_string(), HashMap::new())]);
        let error = Registry::from_config(&config).unwrap_err();

        assert!(error.to_string().contains("unknown keymap context"));
    }

    #[test]
    fn rejects_unknown_action() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("unknown".to_string(), "q".to_string())]),
        )]);
        let error = Registry::from_config(&config).unwrap_err();

        assert!(error.to_string().contains("unknown action `unknown`"));
    }

    #[test]
    fn reports_binding_location_for_invalid_key() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("quit".to_string(), "ctrl-".to_string())]),
        )]);
        let error = Registry::from_config(&config).unwrap_err();

        assert!(error.to_string().contains("normal.quit"));
    }
}
