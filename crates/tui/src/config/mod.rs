use std::{fs, io::ErrorKind};

use serde::Deserialize;

const DEFAULT_FOLDER: &str = ".eggplant";
const CONFIG_FILE: &str = "config.toml";

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    /// crossterm event poll timeout in milliseconds
    pub event_poll_timeout: u64,
    // todo theme
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            event_poll_timeout: 250,
            theme: "default".to_string(),
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
}
