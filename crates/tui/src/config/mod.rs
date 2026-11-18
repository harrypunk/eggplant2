use std::{collections::HashMap, fs, io::ErrorKind, path::PathBuf};

use serde::Deserialize;

const DEFAULT_FOLDER: &str = ".eggplant";
const CONFIG_FILE: &str = "config.toml";

/// Raw keymap entries: context -> action -> key binding.
///
/// Validation and conversion into runtime keybindings happen in the keymap registry.
pub type KeymapConfig = HashMap<String, HashMap<String, String>>;

fn keymap_context<const N: usize>(
    name: &str,
    bindings: [(&str, &str); N],
) -> (String, HashMap<String, String>) {
    let bindings = bindings
        .into_iter()
        .map(|(action, key)| (action.to_string(), key.to_string()))
        .collect();
    (name.to_string(), bindings)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct FeaturesConfig {
    pub page_scroll: PageScrollConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PageScrollConfig {
    pub step: String,
}

impl Default for PageScrollConfig {
    fn default() -> Self {
        Self {
            step: "75%".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum ThemeConfig {
    Name(String),
    Ghostty {
        provider: String,
        path: PathBuf,
        #[serde(default = "default_theme_name")]
        fallback: String,
    },
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self::Name(default_theme_name())
    }
}

impl ThemeConfig {
    pub fn fallback_name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Ghostty { fallback, .. } => fallback,
        }
    }

    pub fn fallback_theme(&self) -> color_eyre::Result<crate::theme::Theme> {
        if let Self::Ghostty { provider, .. } = self
            && provider != "ghostty"
        {
            color_eyre::eyre::bail!("unknown theme provider `{provider}`; expected `ghostty`");
        }
        crate::theme::Theme::from_name(self.fallback_name())
    }
}

fn default_theme_name() -> String {
    "default".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// crossterm event poll timeout in milliseconds
    pub event_poll_timeout: u64,
    /// Built-in theme name or a provider table with an explicit fallback.
    pub theme: ThemeConfig,
    pub features: FeaturesConfig,
    pub keymap: KeymapConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            event_poll_timeout: 250,
            theme: ThemeConfig::default(),
            features: FeaturesConfig::default(),
            keymap: HashMap::from([
                keymap_context(
                    "normal",
                    [
                        ("enter_insert_mode", "i"),
                        ("enter_visual_mode", "v"),
                        ("enter_visual_line_mode", "shift-v"),
                        ("insert_after_cursor", "a"),
                        ("insert_at_line_end", "shift-a"),
                        ("insert_at_first_nonblank", "shift-i"),
                        ("open_line_below", "o"),
                        ("open_line_above", "shift-o"),
                        ("move_left", "h"),
                        ("move_right", "l"),
                        ("move_up", "k"),
                        ("move_down", "j"),
                        ("move_line_start", "0"),
                        ("move_line_end", "$"),
                        ("move_next_word_start", "w"),
                        ("move_previous_word_start", "b"),
                        ("move_next_word_end", "e"),
                        ("focus_left", "ctrl-h"),
                        ("focus_down", "ctrl-j"),
                        ("focus_up", "ctrl-k"),
                        ("focus_right", "ctrl-l"),
                        ("undo", "u"),
                        ("redo", "ctrl-r"),
                        ("page_scroll_down", "ctrl-f"),
                        ("page_scroll_up", "ctrl-u"),
                        ("view_align_start", "z"),
                        ("buffer_jump_start", "s"),
                        ("next_buffer_pending", "]"),
                        ("previous_buffer_pending", "["),
                        ("delete_grapheme", "x"),
                        ("delete_line_pending", "d"),
                        ("change_line_pending", "c"),
                        ("yank_line_pending", "y"),
                        ("paste_after", "p"),
                        ("paste_before", "shift-p"),
                    ],
                ),
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
                keymap_context(
                    "visual",
                    [
                        ("exit_visual_mode", "esc"),
                        ("move_left", "h"),
                        ("move_right", "l"),
                        ("move_up", "k"),
                        ("move_down", "j"),
                        ("move_next_word_start", "w"),
                        ("move_previous_word_start", "b"),
                        ("move_next_word_end", "e"),
                        ("move_line_start", "0"),
                        ("move_line_end", "$"),
                        ("delete_selection_x", "x"),
                        ("delete_selection_d", "d"),
                        ("change_selection", "c"),
                        ("yank_selection", "y"),
                    ],
                ),
                keymap_context(
                    "explorer",
                    [
                        ("explorer_next", "j"),
                        ("explorer_previous", "k"),
                        ("open_selected", "enter"),
                        ("explorer_fold", "h"),
                        ("explorer_unfold", "l"),
                        ("focus_left", "ctrl-h"),
                        ("focus_down", "ctrl-j"),
                        ("focus_up", "ctrl-k"),
                        ("focus_right", "ctrl-l"),
                        ("refresh_explorer", "r"),
                        ("toggle_ignored_files", "shift-i"),
                    ],
                ),
                keymap_context(
                    "file_picker",
                    [
                        ("close_file_picker", "esc"),
                        ("file_picker_backspace", "backspace"),
                        ("open_picker_file", "enter"),
                        ("file_picker_next", "down"),
                        ("file_picker_previous", "up"),
                    ],
                ),
                keymap_context(
                    "theme_picker",
                    [
                        ("close_theme_picker", "esc"),
                        ("apply_theme", "enter"),
                        ("theme_picker_backspace", "backspace"),
                        ("theme_picker_next", "down"),
                        ("theme_picker_previous", "up"),
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
                    "buffer_grep",
                    [
                        ("buffer_grep_close", "esc"),
                        ("buffer_grep_backspace", "backspace"),
                        ("buffer_grep_next", "down"),
                        ("buffer_grep_previous", "up"),
                        ("buffer_grep_accept", "enter"),
                    ],
                ),
                keymap_context(
                    "project_grep",
                    [
                        ("project_grep_close", "esc"),
                        ("project_grep_backspace", "backspace"),
                        ("project_grep_next", "down"),
                        ("project_grep_previous", "up"),
                        ("project_grep_accept", "enter"),
                    ],
                ),
                keymap_context("line_jump", [("line_jump_cancel", "esc")]),
                keymap_context("view_align", [("view_align_cancel", "esc")]),
                keymap_context("change", [("change_cancel", "esc")]),
                keymap_context("yank", [("yank_cancel", "esc")]),
                keymap_context(
                    "vcs_commit",
                    [
                        ("vcs_commit_close", "q"),
                        ("vcs_commit_discard", "ctrl-c"),
                        ("vcs_commit_submit", "ctrl-s"),
                        ("enter_insert_mode", "i"),
                        ("insert_after_cursor", "a"),
                        ("insert_at_line_end", "shift-a"),
                        ("insert_at_first_nonblank", "shift-i"),
                        ("open_line_below", "o"),
                        ("open_line_above", "shift-o"),
                        ("move_left", "h"),
                        ("move_right", "l"),
                        ("move_up", "k"),
                        ("move_down", "j"),
                        ("move_line_start", "0"),
                        ("move_line_end", "$"),
                        ("move_next_word_start", "w"),
                        ("move_previous_word_start", "b"),
                        ("move_next_word_end", "e"),
                        ("delete_grapheme", "x"),
                        ("delete_line_pending", "d"),
                        ("yank_line_pending", "y"),
                        ("paste_after", "p"),
                        ("paste_before", "shift-p"),
                        ("undo", "u"),
                        ("redo", "ctrl-r"),
                    ],
                ),
                keymap_context(
                    "vcs_commit_input",
                    [
                        ("vcs_commit_discard", "ctrl-c"),
                        ("vcs_commit_submit", "ctrl-s"),
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
                        ("close_vcs_history", "esc"),
                        ("close_vcs_history_q", "q"),
                        ("vcs_history_next", "j"),
                        ("vcs_history_previous", "k"),
                        ("vcs_history_toggle_diff", "tab"),
                        ("open_vcs_commit_detail", "enter"),
                        ("vcs_history_copy_short", "c"),
                        ("vcs_history_copy_full", "shift-c"),
                        ("vcs_history_load_more", "n"),
                    ],
                ),
                keymap_context(
                    "vcs_merge",
                    [
                        ("vcs_merge_cancel", "esc"),
                        ("vcs_merge_submit", "enter"),
                        ("vcs_merge_backspace", "backspace"),
                        ("vcs_merge_next", "down"),
                        ("vcs_merge_previous", "up"),
                    ],
                ),
                keymap_context(
                    "vcs_branch_menu",
                    [
                        ("vcs_branch_menu_close", "esc"),
                        ("vcs_branch_create", "c"),
                        ("vcs_branch_switch", "s"),
                    ],
                ),
                keymap_context(
                    "vcs_branch",
                    [
                        ("vcs_branch_cancel", "esc"),
                        ("vcs_branch_submit", "enter"),
                        ("vcs_branch_backspace", "backspace"),
                        ("vcs_branch_next", "down"),
                        ("vcs_branch_previous", "up"),
                    ],
                ),
                keymap_context(
                    "vcs",
                    [
                        ("close_vcs_page", "esc"),
                        ("close_vcs_page_q", "q"),
                        ("vcs_next_change", "j"),
                        ("vcs_previous_change", "k"),
                        ("open_vcs_change", "enter"),
                        ("vcs_stage", "s"),
                        ("vcs_unstage", "u"),
                        ("vcs_stage_all", "ctrl-s"),
                        ("vcs_unstage_all", "shift-u"),
                        ("vcs_toggle_diff", "tab"),
                        ("vcs_discard_placeholder", "x"),
                        ("vcs_commit_placeholder", "c"),
                        ("vcs_help_placeholder", "?"),
                        ("open_vcs_history", "l"),
                        ("vcs_merge_start", "m"),
                        ("vcs_branch_menu", "b"),
                    ],
                ),
            ]),
        }
    }
}

impl Config {
    /// Parse configuration and inherit unspecified keybindings from the defaults.
    pub fn parse(content: &str) -> color_eyre::Result<Self> {
        let mut config: Self = toml::from_str(content)?;
        let configured_keymap = std::mem::take(&mut config.keymap);
        config.keymap = configured_keymap.into_iter().fold(
            Self::default().keymap,
            |mut keymap, (context, overrides)| {
                keymap.entry(context).or_default().extend(overrides);
                keymap
            },
        );
        Ok(config)
    }

    /// Load `~/.eggplant/config.toml`, falling back to defaults when it does not exist.
    pub fn load() -> color_eyre::Result<Self> {
        let Some(path) =
            std::env::home_dir().map(|home| home.join(DEFAULT_FOLDER).join(CONFIG_FILE))
        else {
            return Ok(Self::default());
        };
        match fs::read_to_string(&path) {
            Ok(content) => Self::parse(&content),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_file_uses_defaults() {
        let config = Config::parse("").unwrap();
        assert_eq!(config.event_poll_timeout, 250);
        assert_eq!(config.theme.fallback_name(), "default");
        assert_eq!(config.features.page_scroll.step, "75%");
    }

    #[test]
    fn partial_file_overrides_defaults() {
        let config = Config::parse("event_poll_timeout = 16").unwrap();
        assert_eq!(config.event_poll_timeout, 16);
        assert_eq!(config.theme.fallback_name(), "default");
    }

    #[test]
    fn partial_keymaps_override_actions_and_inherit_all_other_defaults() {
        let config =
            Config::parse("[keymap.normal]\nundo = \"ctrl-z\"\npage_scroll_down = \"ctrl-f\"\n")
                .unwrap();

        assert_eq!(config.keymap["normal"]["undo"], "ctrl-z");
        assert_eq!(config.keymap["normal"]["page_scroll_down"], "ctrl-f");
        assert_eq!(config.keymap["normal"]["enter_insert_mode"], "i");
        assert_eq!(config.keymap["explorer"]["refresh_explorer"], "r");
        assert_eq!(config.keymap["theme_picker"]["apply_theme"], "enter");
    }

    #[test]
    fn ghostty_provider_config_preserves_path_and_fallback() {
        let config = Config::parse(
            "theme = { provider = \"ghostty\", path = \"/tmp/ghostty-theme\", fallback = \"tokyonight\" }",
        )
        .unwrap();
        let ThemeConfig::Ghostty {
            provider,
            path,
            fallback,
        } = &config.theme
        else {
            panic!("expected Ghostty provider config");
        };
        assert_eq!(provider, "ghostty");
        assert_eq!(path, &PathBuf::from("/tmp/ghostty-theme"));
        assert_eq!(fallback, "tokyonight");
        assert_eq!(
            config.theme.fallback_theme().unwrap().builtin(),
            Some(crate::theme::ThemeName::TokyoNight)
        );
    }

    #[test]
    fn provider_config_defaults_fallback_and_rejects_unknown_providers() {
        let config =
            Config::parse("theme = { provider = \"ghostty\", path = \"/tmp/theme\" }").unwrap();
        assert_eq!(config.theme.fallback_name(), "default");

        let config =
            Config::parse("theme = { provider = \"vscode\", path = \"/tmp/theme\" }").unwrap();
        assert!(config.theme.fallback_theme().is_err());
    }

    #[test]
    fn example_file_matches_all_builtin_defaults() {
        let config = Config::parse(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config.example.toml"
        )))
        .unwrap();
        let defaults = Config::default();

        assert_eq!(config.theme.fallback_name(), defaults.theme.fallback_name());
        assert_eq!(config.event_poll_timeout, defaults.event_poll_timeout);
        assert_eq!(
            config.features.page_scroll.step,
            defaults.features.page_scroll.step
        );
        assert_eq!(config.keymap, defaults.keymap);
    }
}
