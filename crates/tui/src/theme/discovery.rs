use std::{collections::BTreeMap, io::ErrorKind, path::PathBuf};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    theme_picker::{ThemePickerEntry, ThemePickerState, ThemeProviderId},
};

pub fn start_discovery(state: &mut ThemePickerState) -> Option<Effect> {
    let revision = state.begin_discovery(ThemeProviderId::GHOSTTY)?;
    Some(Effect::feature(DiscoveryRequest {
        revision,
        directories: local_theme_directories(),
    }))
}

struct DiscoveryRequest {
    revision: u64,
    directories: Vec<PathBuf>,
}

impl FeatureEffect for DiscoveryRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = discover_in(&self.directories);
        Message::FeatureCompletion(FeatureCompletion::new(DiscoveryCompletion {
            revision: self.revision,
            result,
        }))
    }
}

struct DiscoveryCompletion {
    revision: u64,
    result: Result<Vec<ThemePickerEntry>, String>,
}

impl Completion for DiscoveryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match self.result {
            Ok(entries) => {
                model
                    .theme_picker
                    .complete_discovery(self.revision, entries);
                None
            }
            Err(error) => {
                if !model.theme_picker.fail_discovery(self.revision) {
                    return None;
                }
                Some(crate::notifications::report_status(
                    model,
                    StatusMessage::error(format!("Theme discovery failed: {error}")),
                ))
            }
        }
    }
}

fn local_theme_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        directories.push(PathBuf::from(config).join("ghostty").join("themes"));
    } else if let Some(home) = std::env::home_dir() {
        directories.push(home.join(".config").join("ghostty").join("themes"));
    }
    if let Some(resources) =
        std::env::var_os("GHOSTTY_RESOURCES_DIR").filter(|path| !path.is_empty())
    {
        directories.push(PathBuf::from(resources).join("themes"));
    }
    directories.push(PathBuf::from("/usr/local/share/ghostty/themes"));
    directories.push(PathBuf::from("/usr/share/ghostty/themes"));
    directories
}

fn discover_in(directories: &[PathBuf]) -> Result<Vec<ThemePickerEntry>, String> {
    let mut themes = BTreeMap::new();
    for directory in directories {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == ErrorKind::NotFound => continue,
            Err(error) => return Err(format!("{}: {error}", directory.display())),
        };
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            themes.entry(name).or_insert_with(|| {
                ThemePickerEntry::ghostty(
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    path.clone(),
                )
            });
        }
    }
    Ok(themes.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_regular_files_sorted_with_user_precedence() {
        let user = tempfile::tempdir().unwrap();
        let system = tempfile::tempdir().unwrap();
        std::fs::write(user.path().join("zeta"), "user").unwrap();
        std::fs::write(user.path().join("shared"), "user").unwrap();
        std::fs::write(system.path().join("alpha"), "system").unwrap();
        std::fs::write(system.path().join("shared"), "system").unwrap();
        std::fs::create_dir(system.path().join("nested")).unwrap();

        let entries =
            discover_in(&[user.path().to_path_buf(), system.path().to_path_buf()]).unwrap();
        let names = entries.iter().map(|entry| entry.name()).collect::<Vec<_>>();
        assert_eq!(names, ["alpha", "shared", "zeta"]);
        let shared = entries
            .iter()
            .find(|entry| entry.name() == "shared")
            .unwrap();
        let path = shared.file_path().unwrap();
        assert_eq!(path, &user.path().join("shared"));
    }

    #[test]
    fn missing_directories_are_ignored() {
        let missing = PathBuf::from("/definitely/missing/ghostty/themes");
        assert!(discover_in(&[missing]).unwrap().is_empty());
    }
}
