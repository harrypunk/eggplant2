use std::collections::{HashMap, hash_map::Entry};

use color_eyre::eyre::eyre;
use crossterm::event::KeyCode;

use crate::{
    config::{KeymapConfig, keymap_context},
    data_object::{CommandId, Context, GeneralAction},
    manager::{
        command_registry::CommandRegistry,
        context::{ContextBindings, KeyChord},
        keymap_contribution::{KeymapCapability, KeymapCatalog},
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
        catalog: &KeymapCatalog,
    ) -> color_eyre::Result<Self> {
        let mut declarations = HashMap::new();
        let mut contexts = HashMap::new();
        for entry in catalog.iter() {
            if contexts.insert(entry.context, entry.name).is_some() {
                return Err(eyre!(
                    "duplicate keymap declaration for `{:?}`",
                    entry.context
                ));
            }
            if declarations
                .insert(entry.name, (entry.context, entry.capabilities))
                .is_some()
            {
                return Err(eyre!("duplicate keymap declaration for `{}`", entry.name));
            }
        }
        let mut merged = config
            .iter()
            .map(|(context, bindings)| (context.clone(), bindings.clone()))
            .chain(common_bindings())
            .fold(
                HashMap::<String, HashMap<String, String>>::new(),
                |mut merged, (context, bindings)| {
                    merged.entry(context).or_default().extend(bindings);
                    merged
                },
            );
        for entry in catalog.iter() {
            merged.entry(entry.name.to_string()).or_default().extend(
                entry
                    .commands
                    .iter()
                    .map(|(action, key)| (action.to_string(), key.to_string())),
            );
        }
        merged
            .iter()
            .map(|(context_name, bindings)| {
                let declaration = declarations.get(context_name.as_str());
                let context = declaration
                    .map(|(context, _)| *context)
                    .or_else(|| parse_context(context_name))
                    .ok_or_else(|| eyre!("unknown keymap context `{context_name}`"))?;
                let capabilities = declaration.map_or(&[][..], |(_, capabilities)| *capabilities);
                let mut bindings =
                    parse_context_bindings(context, context_name, bindings, commands)?;
                if list_navigation_contexts().contains(&context_name.as_str()) {
                    bindings = bindings.with_list_navigation();
                }
                for capability in capabilities {
                    bindings = apply_capability(bindings, *capability);
                }
                Ok((bindings.context, bindings))
            })
            .collect::<color_eyre::Result<ContextBindingMap>>()
            .map(|bindings| Self { bindings })
    }

    pub fn get_binding(&self, context: &Context) -> Option<&ContextBindings> {
        self.bindings.get(context)
    }
}

/// Contexts that compose the shared Ctrl-N/P aliases through Down/Up.
fn list_navigation_contexts() -> &'static [&'static str] {
    &["welcome", "vcs_branch", "vcs_merge", "vcs_reset"]
}

const MODAL_GENERAL_ACTIONS: [(KeyCode, GeneralAction); 4] = [
    (KeyCode::Down, GeneralAction::Next),
    (KeyCode::Up, GeneralAction::Previous),
    (KeyCode::Enter, GeneralAction::Activate),
    (KeyCode::Esc, GeneralAction::Back),
];

fn apply_capability(bindings: ContextBindings, capability: KeymapCapability) -> ContextBindings {
    match capability {
        KeymapCapability::ListNavigation => bindings.with_list_navigation(),
        KeymapCapability::ModalActions => bindings.with_general_actions(
            MODAL_GENERAL_ACTIONS.map(|(key, action)| (KeyChord::from(key), action)),
        ),
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
        keymap_context("explorer", [("open_selected", "enter")]),
        keymap_context(
            "explorer_add",
            [
                ("explorer_add_submit", "enter"),
                ("close_explorer_add", "esc"),
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
            ],
        ),
        keymap_context(
            "explorer_help",
            [("close_explorer_help", "esc"), ("explorer_help", "?")],
        ),
        keymap_context("buffer_jump", [("buffer_jump_cancel", "esc")]),
        keymap_context("buffer_navigation", [("buffer_navigation_cancel", "esc")]),
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
            ],
        ),
        keymap_context(
            "vcs_reset",
            [
                ("vcs_reset_submit", "enter"),
                ("vcs_reset_next", "down"),
                ("vcs_reset_previous", "up"),
                ("vcs_reset_cancel", "esc"),
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
            ],
        ),
        keymap_context(
            "vcs",
            [("open_vcs_change", "enter"), ("close_vcs_page", "esc")],
        ),
    ])
}

fn parse_context_bindings(
    context: Context,
    context_name: &str,
    bindings: &HashMap<String, String>,
    commands: &CommandRegistry,
) -> color_eyre::Result<ContextBindings> {
    let parsed = bindings.iter().try_fold(
        HashMap::<KeyChord, (&str, CommandId)>::new(),
        |mut parsed, (action_name, key_name)| -> color_eyre::Result<_> {
            if action_name.ends_with("_backspace") && commands.get(action_name).is_none() {
                return Ok(parsed);
            }
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
        "buffer_jump" => Some(Context::BufferJump),
        "buffer_navigation" => Some(Context::BufferNavigation),
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
    use crate::{config::Config, manager::context::InputAction};

    #[test]
    fn removed_backspace_actions_do_not_break_existing_config() {
        use crossterm::event::KeyCode;

        let config = HashMap::from([keymap_context(
            "file_picker",
            [("file_picker_backspace", "backspace")],
        )]);
        let registry = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();
        assert!(
            registry
                .get_binding(&Context::FilePicker)
                .unwrap()
                .resolve(KeyChord::from(KeyCode::Backspace))
                .is_none()
        );
    }

    #[test]
    fn loads_default_bindings() {
        let registry = Registry::from_config(
            &Config::default().keymap,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();

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
            Some(InputAction::Command(CommandId("exit_insert_mode")))
        ));
    }

    #[test]
    fn loads_bindings_with_modifiers() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("quit".to_string(), "ctrl-q".to_string())]),
        )]);
        let registry = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();

        assert!(matches!(
            registry
                .get_binding(&Context::AppNormal)
                .unwrap()
                .resolve(KeyChord::new(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            Some(InputAction::Command(CommandId("quit")))
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
    fn common_keys_keep_priority_and_custom_commands_remain_available() {
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
            (
                "model_selector".to_string(),
                HashMap::from([("agent_interrupt".to_string(), "ctrl-x".to_string())]),
            ),
        ]);
        let registry = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap();

        let picker = registry.get_binding(&Context::FilePicker).unwrap();
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(InputAction::General(GeneralAction::Activate))
        );
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(InputAction::General(GeneralAction::Next))
        );
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Char('x'), KeyModifiers::NONE)),
            Some(InputAction::Command(CommandId("open_picker_file")))
        );
        assert_eq!(
            picker.resolve(KeyChord::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            Some(InputAction::Command(CommandId("file_picker_next")))
        );

        let selector = registry.get_binding(&Context::ModelSelector).unwrap();
        assert_eq!(
            selector.resolve(KeyChord::new(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            Some(InputAction::Command(CommandId("agent_interrupt")))
        );
        assert_eq!(
            selector.resolve(KeyChord::new(KeyCode::Down, KeyModifiers::NONE)),
            Some(InputAction::General(GeneralAction::Next))
        );
        assert_eq!(
            selector.resolve(KeyChord::new(KeyCode::Enter, KeyModifiers::NONE)),
            Some(InputAction::General(GeneralAction::Activate))
        );

        let delete = registry.get_binding(&Context::ExplorerDelete).unwrap();
        assert_eq!(
            delete.resolve(KeyChord::new(KeyCode::Char('y'), KeyModifiers::NONE)),
            Some(InputAction::Command(CommandId("explorer_delete_confirm")))
        );
        assert_eq!(
            delete.resolve(KeyChord::new(KeyCode::Char('n'), KeyModifiers::NONE)),
            None
        );
    }

    #[test]
    fn rejects_unknown_context() {
        let config = HashMap::from([("unknown".to_string(), HashMap::new())]);
        let error = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("unknown keymap context"));
    }

    #[test]
    fn rejects_unknown_action() {
        let config = HashMap::from([(
            "normal".to_string(),
            HashMap::from([("unknown".to_string(), "q".to_string())]),
        )]);
        let error = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap_err();

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

        let error = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap_err();
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
        let error = Registry::from_config(
            &config,
            &CommandRegistry::default(),
            &crate::composition::keymaps::catalog(),
        )
        .unwrap_err();

        assert!(error.to_string().contains("normal.quit"));
    }
}
