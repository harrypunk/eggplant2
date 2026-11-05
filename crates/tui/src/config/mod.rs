use std::{collections::HashMap, fs, io::ErrorKind};

use serde::Deserialize;

const DEFAULT_FOLDER: &str = ".eggplant";
const CONFIG_FILE: &str = "config.toml";

/// Raw keymap entries: context -> action -> key binding.
///
/// Validation and conversion into runtime keybindings happen in the keymap registry.
pub type KeymapConfig = HashMap<String, HashMap<String, String>>;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// crossterm event poll timeout in milliseconds
    pub event_poll_timeout: u64,
    // todo theme
    pub theme: String,
    pub keymap: KeymapConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            event_poll_timeout: 250,
            theme: "default".to_string(),
            keymap: HashMap::from([
                (
                    "normal".to_string(),
                    HashMap::from([
                        ("quit".to_string(), "q".to_string()),
                        ("enter_insert_mode".to_string(), "i".to_string()),
                        ("move_left".to_string(), "h".to_string()),
                        ("move_right".to_string(), "l".to_string()),
                        ("move_up".to_string(), "k".to_string()),
                        ("move_down".to_string(), "j".to_string()),
                        ("undo".to_string(), "u".to_string()),
                        ("redo".to_string(), "ctrl-r".to_string()),
                        ("toggle_explorer_focus".to_string(), "ctrl-e".to_string()),
                    ]),
                ),
                (
                    "editor_input".to_string(),
                    HashMap::from([
                        ("exit_insert_mode".to_string(), "esc".to_string()),
                        ("backspace".to_string(), "backspace".to_string()),
                        ("insert_newline".to_string(), "enter".to_string()),
                        ("move_left".to_string(), "left".to_string()),
                        ("move_right".to_string(), "right".to_string()),
                        ("move_up".to_string(), "up".to_string()),
                        ("move_down".to_string(), "down".to_string()),
                    ]),
                ),
                (
                    "explorer".to_string(),
                    HashMap::from([
                        ("quit".to_string(), "q".to_string()),
                        ("explorer_next".to_string(), "j".to_string()),
                        ("explorer_previous".to_string(), "k".to_string()),
                        ("open_selected".to_string(), "enter".to_string()),
                        ("explorer_parent".to_string(), "backspace".to_string()),
                        ("refresh_explorer".to_string(), "r".to_string()),
                        ("toggle_explorer_focus".to_string(), "ctrl-e".to_string()),
                    ]),
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
    fn example_file_is_valid() {
        let config: Config = toml::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../config.example.toml"
        )))
        .unwrap();

        assert_eq!(config.keymap["normal"]["quit"], "q");
        assert_eq!(config.keymap["editor_input"]["exit_insert_mode"], "esc");
    }
}
