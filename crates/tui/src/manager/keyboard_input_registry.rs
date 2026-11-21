use std::collections::{HashMap, hash_map::Entry};

use color_eyre::eyre::eyre;

use crate::{
    config::{KeymapConfig, keymap_context},
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
        let config = config
            .iter()
            .map(|(context, bindings)| (context.clone(), bindings.clone()))
            .chain(common_bindings());
        config
            .into_iter()
            .fold(
                HashMap::<String, HashMap<String, String>>::new(),
                |mut merged, (context, bindings)| {
                    merged.entry(context).or_default().extend(bindings);
                    merged
                },
            )
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

fn common_bindings() -> KeymapConfig {
    HashMap::from([
        keymap_context(
            "editor_input",
            [
                ("exit_insert_mode", "esc"),
                ("backspace", "backspace"),
                ("insert_newline", "enter"),
                ("move_left", "left"),
                ("move_right", "right"),
                ("move_up", "up"),
                ("move_down", "down"),
            ],
        ),
        keymap_context("visual", [("exit_visual_mode", "esc")]),
        keymap_context(
            "code_navigation",
            [
                ("code_navigation_open", "enter"),
                ("code_navigation_next", "down"),
                ("code_navigation_previous", "up"),
                ("close_code_navigation", "esc"),
            ],
        ),
        keymap_context("explorer", [("open_selected", "enter")]),
        keymap_context(
            "explorer_add",
            [
                ("explorer_add_submit", "enter"),
                ("close_explorer_add", "esc"),
                ("explorer_add_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "explorer_delete",
            [
                ("explorer_delete_confirm", "y"),
                ("close_explorer_delete", "esc"),
            ],
        ),
        keymap_context(
            "explorer_transfer",
            [
                ("explorer_transfer_submit", "enter"),
                ("close_explorer_transfer", "esc"),
                ("explorer_transfer_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "explorer_help",
            [("close_explorer_help", "esc"), ("explorer_help", "?")],
        ),
        keymap_context(
            "file_picker",
            [
                ("open_picker_file", "enter"),
                ("file_picker_next", "down"),
                ("file_picker_previous", "up"),
                ("close_file_picker", "esc"),
                ("file_picker_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "theme_picker",
            [
                ("apply_theme", "enter"),
                ("theme_picker_next", "down"),
                ("theme_picker_previous", "up"),
                ("close_theme_picker", "esc"),
                ("theme_picker_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "buffer_jump",
            [
                ("buffer_jump_cancel", "esc"),
                ("buffer_jump_backspace", "backspace"),
            ],
        ),
        keymap_context("buffer_navigation", [("buffer_navigation_cancel", "esc")]),
        keymap_context(
            "agent_chat_dialog",
            [
                ("agent_close", "esc"),
                ("agent_submit", "enter"),
                ("agent_backspace", "backspace"),
                ("agent_interrupt", "ctrl-f7"),
            ],
        ),
        keymap_context(
            "buffer_grep",
            [
                ("buffer_grep_next", "down"),
                ("buffer_grep_previous", "up"),
                ("buffer_grep_accept", "enter"),
                ("buffer_grep_close", "esc"),
                ("buffer_grep_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "project_grep",
            [
                ("project_grep_next", "down"),
                ("project_grep_previous", "up"),
                ("project_grep_accept", "enter"),
                ("project_grep_close", "esc"),
                ("project_grep_backspace", "backspace"),
            ],
        ),
        keymap_context("line_jump", [("line_jump_cancel", "esc")]),
        keymap_context("view_align", [("view_align_cancel", "esc")]),
        keymap_context("change", [("change_cancel", "esc")]),
        keymap_context("yank", [("yank_cancel", "esc")]),
        keymap_context(
            "vcs_commit_input",
            [
                ("exit_insert_mode", "esc"),
                ("backspace", "backspace"),
                ("insert_newline", "enter"),
                ("move_left", "left"),
                ("move_right", "right"),
                ("move_up", "up"),
                ("move_down", "down"),
            ],
        ),
        keymap_context(
            "vcs_history",
            [
                ("open_vcs_commit_detail", "enter"),
                ("close_vcs_history", "esc"),
            ],
        ),
        keymap_context(
            "vcs_merge",
            [
                ("vcs_merge_submit", "enter"),
                ("vcs_merge_next", "down"),
                ("vcs_merge_previous", "up"),
                ("vcs_merge_cancel", "esc"),
                ("vcs_merge_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "vcs_reset",
            [
                ("vcs_reset_submit", "enter"),
                ("vcs_reset_next", "down"),
                ("vcs_reset_previous", "up"),
                ("vcs_reset_cancel", "esc"),
                ("vcs_reset_backspace", "backspace"),
            ],
        ),
        keymap_context("vcs_reset_menu", [("vcs_reset_menu_close", "esc")]),
        keymap_context("vcs_branch_menu", [("vcs_branch_menu_close", "esc")]),
        keymap_context(
            "vcs_branch",
            [
                ("vcs_branch_submit", "enter"),
                ("vcs_branch_next", "down"),
                ("vcs_branch_previous", "up"),
                ("vcs_branch_cancel", "esc"),
                ("vcs_branch_backspace", "backspace"),
            ],
        ),
        keymap_context(
            "vcs",
            [("open_vcs_change", "enter"), ("close_vcs_page", "esc")],
        ),
    ])
}

fn parse_context_bindings(
    context_name: &str,
    bindings: &HashMap<String, String>,
    commands: &CommandRegistry,
) -> color_eyre::Result<ContextBindings> {
    let context = parse_context(context_name)
        .ok_or_else(|| eyre!("unknown keymap context `{context_name}`"))?;

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
        "vcs_history" => Some(Context::VcsHistory),
        "vcs_commit" => Some(Context::VcsCommit),
        "vcs_commit_input" => Some(Context::VcsCommitInput),
        "vcs_merge" => Some(Context::VcsMerge),
        "vcs_reset_menu" => Some(Context::VcsResetMenu),
        "vcs_reset" => Some(Context::VcsReset),
        "vcs_branch_menu" => Some(Context::VcsBranchMenu),
        "vcs_branch" => Some(Context::VcsBranch),
        "editor_input" => Some(Context::EditorInput),
        "visual" => Some(Context::EditorVisual),
        "explorer" => Some(Context::Explorer),
        "explorer_help" => Some(Context::ExplorerHelp),
        "explorer_add" => Some(Context::ExplorerAdd),
        "explorer_delete" => Some(Context::ExplorerDelete),
        "explorer_transfer" => Some(Context::ExplorerTransfer),
        "code_navigation" => Some(Context::CodeNavigation),
        "file_picker" => Some(Context::FilePicker),
        "theme_picker" => Some(Context::ThemePicker),
        "buffer_jump" => Some(Context::BufferJump),
        "buffer_navigation" => Some(Context::BufferNavigation),
        "buffer_grep" => Some(Context::BufferGrep),
        "project_grep" => Some(Context::ProjectGrep),
        "line_jump" => Some(Context::LineJump),
        "view_align" => Some(Context::ViewAlign),
        "change" => Some(Context::EditorChange),
        "yank" => Some(Context::EditorYank),
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
    fn common_keys_cannot_be_overridden_by_custom_entries() {
        let config = HashMap::from([
            (
                "file_picker".to_string(),
                HashMap::from([
                    ("open_picker_file".to_string(), "x".to_string()),
                    ("file_picker_next".to_string(), "n".to_string()),
                ]),
            ),
            (
                "explorer_delete".to_string(),
                HashMap::from([("explorer_delete_confirm".to_string(), "n".to_string())]),
            ),
        ]);
        let registry = Registry::from_config(&config, &CommandRegistry::default()).unwrap();

        let picker = registry.get_binding(&Context::FilePicker).unwrap();
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(CommandId("open_picker_file"))
        );
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(CommandId("file_picker_next"))
        );
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            None
        );

        let delete = registry.get_binding(&Context::ExplorerDelete).unwrap();
        assert_eq!(
            delete.resolve(KeyChord::new(KeyCode::Char('y'), KeyModifiers::NONE)),
            Some(CommandId("explorer_delete_confirm"))
        );
        assert_eq!(
            delete.resolve(KeyChord::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            None
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
