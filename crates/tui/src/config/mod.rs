use std::{collections::HashMap, fs, io::ErrorKind, path::PathBuf};

use color_eyre::eyre::WrapErr as _;
use serde::{Deserialize, Serialize};

use app::language::LanguageServers;
use app::language_config::parse_language_servers;

const DEFAULT_FOLDER: &str = ".eggplant";
const CONFIG_FILE: &str = "config.toml";
const LANGUAGES_FILE: &str = "languages.toml";

/// Raw keymap entries: context -> action -> key binding.
///
/// Validation and conversion into runtime keybindings happen in the keymap registry.
pub type KeymapConfig = HashMap<String, HashMap<String, String>>;

pub(crate) fn keymap_context(
    name: &str,
    bindings: impl IntoIterator<Item = (&'static str, &'static str)>,
) -> (String, HashMap<String, String>) {
    let bindings = bindings
        .into_iter()
        .map(|(action, key)| (action.to_string(), key.to_string()))
        .collect();
    (name.to_string(), bindings)
}

/// Bindings shared by Normal-mode EditingTarget consumers.
const SHARED_EDITING_BINDINGS: &[(&str, &str)] = &[
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
    ("change_line_pending", "c"),
    ("yank_line_pending", "y"),
    ("paste_after", "p"),
    ("paste_before", "shift-p"),
    ("undo", "u"),
    ("redo", "ctrl-r"),
];

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

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
                    SHARED_EDITING_BINDINGS.iter().copied().chain([
                        ("enter_visual_mode", "v"),
                        ("enter_visual_line_mode", "shift-v"),
                        ("focus_left", "ctrl-h"),
                        ("focus_down", "ctrl-j"),
                        ("focus_up", "ctrl-k"),
                        ("focus_right", "ctrl-l"),
                        ("page_scroll_down", "ctrl-f"),
                        ("page_scroll_up", "ctrl-u"),
                        ("view_align_start", "z"),
                        ("buffer_jump_start", "s"),
                        ("agent_open", "ctrl-i"),
                        ("next_buffer_pending", "]"),
                        ("previous_buffer_pending", "["),
                    ]),
                ),
                keymap_context(
                    "visual",
                    [
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
                        ("explorer_fold", "h"),
                        ("explorer_unfold", "l"),
                        ("focus_left", "ctrl-h"),
                        ("focus_down", "ctrl-j"),
                        ("focus_up", "ctrl-k"),
                        ("focus_right", "ctrl-l"),
                        ("refresh_explorer", "r"),
                        ("toggle_ignored_files", "shift-i"),
                        ("explorer_help", "?"),
                        ("explorer_add", "a"),
                        ("explorer_delete", "d"),
                        ("explorer_move", "m"),
                        ("explorer_copy", "c"),
                    ],
                ),
                keymap_context("buffer_jump", []),
                keymap_context(
                    "vcs_commit",
                    SHARED_EDITING_BINDINGS.iter().copied().chain([
                        ("vcs_commit_close", "q"),
                        ("vcs_commit_discard", "ctrl-c"),
                        ("vcs_commit_submit", "ctrl-s"),
                    ]),
                ),
                keymap_context(
                    "vcs_commit_input",
                    [
                        ("vcs_commit_discard", "ctrl-c"),
                        ("vcs_commit_submit", "ctrl-s"),
                    ],
                ),
                keymap_context(
                    "vcs_history",
                    [
                        ("close_vcs_history_q", "q"),
                        ("vcs_history_next", "j"),
                        ("vcs_history_previous", "k"),
                        ("vcs_history_toggle_diff", "tab"),
                        ("vcs_history_copy_short", "c"),
                        ("vcs_history_copy_full", "shift-c"),
                        ("vcs_history_load_more", "n"),
                    ],
                ),
                keymap_context("vcs_reset_menu", [("vcs_reset_soft", "s")]),
                keymap_context("vcs_reset", []),
                keymap_context(
                    "vcs_branch_menu",
                    [("vcs_branch_create", "c"), ("vcs_branch_switch", "s")],
                ),
                keymap_context(
                    "vcs",
                    [
                        ("close_vcs_page_q", "q"),
                        ("vcs_next_change", "j"),
                        ("vcs_previous_change", "k"),
                        ("vcs_stage", "s"),
                        ("vcs_unstage", "u"),
                        ("vcs_stage_all", "ctrl-s"),
                        ("vcs_unstage_all", "shift-u"),
                        ("vcs_toggle_diff", "tab"),
                        ("vcs_discard", "x"),
                        ("vcs_commit_placeholder", "c"),
                        ("vcs_help_placeholder", "?"),
                        ("open_vcs_history", "l"),
                        ("vcs_merge_start", "m"),
                        ("vcs_reset_menu", "r"),
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
    /// Atomically replace only the theme setting, preserving unrelated TOML values.
    pub fn save_theme(theme: ThemeConfig) -> Result<(), String> {
        let path = std::env::home_dir()
            .map(|home| home.join(DEFAULT_FOLDER).join(CONFIG_FILE))
            .ok_or_else(|| "home directory is unavailable".to_string())?;
        save_theme_to(&path, &theme)
    }
}

/// Load `~/.eggplant/languages.toml`, preserving community defaults when it is absent.
pub fn load_language_servers() -> color_eyre::Result<LanguageServers> {
    let Some(path) =
        std::env::home_dir().map(|home| home.join(DEFAULT_FOLDER).join(LANGUAGES_FILE))
    else {
        return Ok(LanguageServers::defaults());
    };
    load_language_servers_from(&path)
}

pub(crate) fn load_language_servers_from(
    path: &std::path::Path,
) -> color_eyre::Result<LanguageServers> {
    match fs::read_to_string(path) {
        Ok(content) => parse_language_servers(&content)
            .wrap_err_with(|| format!("failed to parse `{}`", path.display())),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(LanguageServers::defaults()),
        Err(error) => Err(error).wrap_err_with(|| format!("failed to read `{}`", path.display())),
    }
}

pub(crate) fn save_theme_to(path: &std::path::Path, theme: &ThemeConfig) -> Result<(), String> {
    let mut document = match fs::read_to_string(path) {
        Ok(content) => {
            toml::from_str::<toml::Table>(&content).map_err(|error| error.to_string())?
        }
        Err(error) if error.kind() == ErrorKind::NotFound => toml::Table::new(),
        Err(error) => return Err(error.to_string()),
    };
    document.insert(
        "theme".to_string(),
        toml::Value::try_from(theme).map_err(|error| error.to_string())?,
    );
    let content = toml::to_string(&document).map_err(|error| error.to_string())?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, content).map_err(|error| error.to_string())?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

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
    fn language_file_replaces_defaults_and_missing_file_preserves_them() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.toml");
        let defaults = load_language_servers_from(&missing).unwrap();
        assert!(defaults.find_for_path(Path::new("main.rs")).is_some());
        assert!(defaults.find_for_path(Path::new("main.ts")).is_some());
        assert!(defaults.find_for_path(Path::new("main.py")).is_some());

        let path = directory.path().join("languages.toml");
        fs::write(
            &path,
            r#"
[language-server.ts]
command = "typescript-language-server"
args = ["--stdio"]
[[language]]
name = "typescript"
file-types = ["ts", "tsx"]
language-servers = ["ts"]
"#,
        )
        .unwrap();
        let configured = load_language_servers_from(&path).unwrap();
        assert!(configured.find_for_path(Path::new("main.ts")).is_some());
        assert!(configured.find_for_path(Path::new("main.rs")).is_none());
    }

    #[test]
    fn invalid_language_file_reports_its_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("languages.toml");
        fs::write(
            &path,
            r#"
[[language]]
name = "typescript"
file-types = ["ts"]
language-servers = ["missing"]
"#,
        )
        .unwrap();

        let error = load_language_servers_from(&path).unwrap_err();
        let message = format!("{error:#}");
        assert!(message.contains(path.to_str().unwrap()));
        assert!(message.contains("unknown language server"));
    }

    #[test]
    fn save_theme_preserves_unrelated_toml_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        fs::write(
            &path,
            "theme = \"default\"\ncustom_key = \"keep\"\n[keymap.normal]\nundo = \"ctrl-z\"\n",
        )
        .unwrap();
        save_theme_to(&path, &ThemeConfig::Name("tokyonight".to_string())).unwrap();

        let document = fs::read_to_string(&path).unwrap();
        let config = Config::parse(&document).unwrap();
        assert_eq!(config.theme.fallback_name(), "tokyonight");
        assert!(document.contains("custom_key = \"keep\""));
        assert_eq!(config.keymap["normal"]["undo"], "ctrl-z");

        save_theme_to(
            &path,
            &ThemeConfig::Ghostty {
                provider: "ghostty".to_string(),
                path: PathBuf::from("/tmp/ghostty-theme"),
                fallback: "catppuccin_mocha".to_string(),
            },
        )
        .unwrap();
        let config = Config::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        let ThemeConfig::Ghostty {
            provider,
            path,
            fallback,
        } = config.theme
        else {
            panic!("expected persisted Ghostty config");
        };
        assert_eq!(provider, "ghostty");
        assert_eq!(path, PathBuf::from("/tmp/ghostty-theme"));
        assert_eq!(fallback, "catppuccin_mocha");
    }

    #[test]
    fn defaults_omit_common_and_confirmation_bindings() {
        let config = Config::default();
        let common = ["esc", "backspace", "enter", "left", "right", "up", "down"];
        assert!(config.keymap.values().all(|bindings| {
            bindings
                .values()
                .all(|binding| !common.contains(&binding.as_str()))
        }));
        assert!(!config.keymap.contains_key("explorer_delete"));
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
