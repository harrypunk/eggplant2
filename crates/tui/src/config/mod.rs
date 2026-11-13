use std::{collections::HashMap, fs, io::ErrorKind};

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

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// crossterm event poll timeout in milliseconds
    pub event_poll_timeout: u64,
    /// Name of a built-in theme, resolved by the TUI theme module.
    pub theme: String,
    pub keymap: KeymapConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            event_poll_timeout: 250,
            theme: "default".to_string(),
            keymap: HashMap::from([
                keymap_context(
                    "normal",
                    [
                        ("enter_insert_mode", "i"),
                        ("close_buffer", "ctrl-w"),
                        ("move_left", "h"),
                        ("move_right", "l"),
                        ("move_up", "k"),
                        ("move_down", "j"),
                        ("focus_left", "ctrl-h"),
                        ("focus_down", "ctrl-j"),
                        ("focus_up", "ctrl-k"),
                        ("focus_right", "ctrl-l"),
                        ("undo", "u"),
                        ("redo", "ctrl-r"),
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
                        ("theme_picker_next", "down"),
                        ("theme_picker_previous", "up"),
                    ],
                ),
            ]),
        }
    }
}

impl Config {
    /// Load `~/.eggplant/config.toml`, falling back to defaults when it does not exist.
    pub fn load() -> color_eyre::Result<Self> {
        let Some(path) =
            std::env::home_dir().map(|home| home.join(DEFAULT_FOLDER).join(CONFIG_FILE))
        else {
            return Ok(Self::default());
        };
        match fs::read_to_string(&path) {
            Ok(content) => Ok(toml::from_str(&content)?),
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
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.event_poll_timeout, 250);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn partial_file_overrides_defaults() {
        let config: Config = toml::from_str("event_poll_timeout = 16").unwrap();
        assert_eq!(config.event_poll_timeout, 16);
        assert_eq!(config.theme, "default");
    }

    #[test]
    fn example_file_matches_all_builtin_defaults() {
        let config: Config = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config.example.toml"
        )))
        .unwrap();
        let defaults = Config::default();

        assert_eq!(config.theme, defaults.theme);
        assert_eq!(config.event_poll_timeout, defaults.event_poll_timeout);
        assert_eq!(config.keymap, defaults.keymap);
    }
}
