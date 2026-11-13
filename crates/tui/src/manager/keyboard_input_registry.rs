use std::collections::{HashMap, hash_map::Entry};

use color_eyre::eyre::eyre;

use crate::{
    config::KeymapConfig,
    data_object::{CommandId, Context},
    manager::{
        command_registry::CommandRegistry,
        context::{ContextBindings, KeyChord},
    },
};

type ContextBindingMap = HashMap<Context, ContextBindings>;

/// Validated runtime keybindings grouped by input context.
#[derive(Debug, Default)]
pub struct Registry {
    bindings: ContextBindingMap,
}

impl Registry {
    /// Validate raw configuration and construct the runtime keymap.
    pub fn from_config(
        config: &KeymapConfig,
        commands: &CommandRegistry,
    ) -> color_eyre::Result<Self> {
        config
            .iter()
            .map(|(context_name, bindings)| {
                parse_context_bindings(context_name, bindings, commands)
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
    commands: &CommandRegistry,
) -> color_eyre::Result<ContextBindings> {
    let context = parse_context(context_name).ok_or_else(|| {
        eyre!(
            "unknown keymap context `{context_name}`; expected normal, editor_input, explorer, file_picker, theme_picker, buffer_jump, agent_chat, agent_chat_dialog, or vcs"
        )
    })?;

    let parsed = bindings.iter().try_fold(
        HashMap::<KeyChord, (&str, CommandId)>::new(),
        |mut parsed, (action_name, key_name)| -> color_eyre::Result<_> {
            let (key, command) = parse_binding(context_name, action_name, key_name, commands)?;
            match parsed.entry(key) {
                Entry::Occupied(entry) => Err(eyre!(
                    "duplicate key binding `{key_name}` in keymap context `{context_name}` for actions `{}` and `{action_name}`",
                    entry.get().0
                )),
                Entry::Vacant(entry) => {
                    entry.insert((action_name.as_str(), command));
                    Ok(parsed)
                }
            }
        },
    )?;

    Ok(ContextBindings::from_bindings(
        context,
        parsed.into_iter().map(|(key, (_, command))| (key, command)),
    ))
}

fn parse_binding(
    context_name: &str,
    action_name: &str,
    key_name: &str,
    commands: &CommandRegistry,
) -> color_eyre::Result<(KeyChord, CommandId)> {
    let command = commands
        .get(action_name)
        .ok_or_else(|| eyre!("unknown action `{action_name}` in keymap context `{context_name}`"))?
        .id;
    let key = key_name
        .parse::<KeyChord>()
        .map_err(|error| eyre!("invalid key for `{context_name}.{action_name}`: {error}"))?;

    Ok((key, command))
}

fn parse_context(value: &str) -> Option<Context> {
    match value {
        "normal" => Some(Context::AppNormal),
        "agent_chat" => Some(Context::AgentChat),
        "agent_chat_dialog" => Some(Context::AgentChatDialog),
        "vcs" => Some(Context::VCS),
        "editor_input" => Some(Context::EditorInput),
        "explorer" => Some(Context::Explorer),
        "file_picker" => Some(Context::FilePicker),
        "theme_picker" => Some(Context::ThemePicker),
        "buffer_jump" => Some(Context::BufferJump),
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
        let registry =
            Registry::from_config(&Config::default().keymap, &CommandRegistry::default()).unwrap();

        assert!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE))
                .is_none()
        );
        assert!(matches!(
            registry
                .get_binding(&Context::EditorInput)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Esc, KeyModifiers::NONE)),
            Some(CommandId("exit_insert_mode"))
        ));
    }

    #[test]
    fn loads_bindings_with_modifiers() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("quit".to_string(), "ctrl-q".to_string())]),
        )]);
        let registry = Registry::from_config(&config, &CommandRegistry::default()).unwrap();

        assert!(matches!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(CommandId("quit"))
        ));
        assert!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::NONE))
                .is_none()
        );
    }

    #[test]
    fn rejects_unknown_context() {
        let config = HashMap::from([("unknown".to_string(), HashMap::new())]);
        let error = Registry::from_config(&config, &CommandRegistry::default()).unwrap_err();

        assert!(error.to_string().contains("unknown keymap context"));
    }

    #[test]
    fn rejects_unknown_action() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("unknown".to_string(), "q".to_string())]),
        )]);
        let error = Registry::from_config(&config, &CommandRegistry::default()).unwrap_err();

        assert!(error.to_string().contains("unknown action `unknown`"));
    }

    #[test]
    fn rejects_duplicate_key_chords_in_one_context() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([
                ("undo".to_string(), "u".to_string()),
                ("redo".to_string(), "u".to_string()),
            ]),
        )]);

        let error = Registry::from_config(&config, &CommandRegistry::default()).unwrap_err();
        assert!(error.to_string().contains("duplicate key binding `u`"));
        assert!(error.to_string().contains("normal"));
        assert!(error.to_string().contains("undo"));
        assert!(error.to_string().contains("redo"));
    }

    #[test]
    fn reports_binding_location_for_invalid_key() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("quit".to_string(), "ctrl-".to_string())]),
        )]);
        let error = Registry::from_config(&config, &CommandRegistry::default()).unwrap_err();

        assert!(error.to_string().contains("normal.quit"));
    }
}
