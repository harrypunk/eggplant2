use std::{fs::File, io::Read as _, path::PathBuf};

use crate::{
    config::ThemeConfig,
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{
    Theme,
    ghostty::GhosttyThemeAdapter,
    provider::{ThemeAdapter, ThemeResolver},
};

const MAX_THEME_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Default)]
pub struct ThemeLoadState {
    revision: u64,
    loading: bool,
}

impl ThemeLoadState {
    fn begin(&mut self) -> u64 {
        self.revision = self.revision.wrapping_add(1);
        self.loading = true;
        self.revision
    }

    fn complete(&mut self, revision: u64) -> bool {
        if !self.loading || self.revision != revision {
            return false;
        }
        self.loading = false;
        true
    }
}

pub fn start_loading(source: &ThemeConfig, model: &mut Model) -> Option<Effect> {
    start_loading_inner(source, model, None)
}

pub fn start_loading_and_persist(source: &ThemeConfig, model: &mut Model) -> Option<Effect> {
    start_loading_inner(source, model, Some(source.clone()))
}

fn start_loading_inner(
    source: &ThemeConfig,
    model: &mut Model,
    persist: Option<ThemeConfig>,
) -> Option<Effect> {
    let ThemeConfig::Ghostty { path, .. } = source else {
        return None;
    };
    let revision = model.theme_loading.begin();
    Some(Effect::feature(ThemeLoadRequest {
        path: path.clone(),
        fallback: model.theme.clone(),
        revision,
        persist,
    }))
}

struct ThemeLoadRequest {
    path: PathBuf,
    fallback: Theme,
    revision: u64,
    persist: Option<ThemeConfig>,
}

impl FeatureEffect for ThemeLoadRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = load(&self.path, &self.fallback);
        Message::FeatureCompletion(FeatureCompletion::new(ThemeLoadCompletion {
            revision: self.revision,
            result,
            persist: self.persist,
        }))
    }
}

struct ThemeLoadCompletion {
    revision: u64,
    result: Result<Theme, String>,
    persist: Option<ThemeConfig>,
}

impl Completion for ThemeLoadCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if !model.theme_loading.complete(self.revision) {
            return None;
        }
        match self.result {
            Ok(theme) => {
                let name = theme.name().to_string();
                model.theme = theme;
                let notification = crate::notifications::publish_status(
                    model,
                    StatusMessage::info(format!("Loaded theme `{name}`")),
                    Some("Theme"),
                );
                let persistence = self.persist.and_then(|config| {
                    crate::theme::persist_theme(config, &mut model.theme_persistence)
                });
                Effect::combine(Some(notification), persistence)
            }
            Err(error) => Some(crate::notifications::report_status(
                model,
                StatusMessage::error(format!("Theme load failed: {error}")),
            )),
        }
    }
}

fn load(path: &std::path::Path, fallback: &Theme) -> Result<Theme, String> {
    load_inner(path, fallback).map_err(|error| format!("{}: {error}", path.display()))
}

fn load_inner(path: &std::path::Path, fallback: &Theme) -> Result<Theme, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut source = String::new();
    file.take(MAX_THEME_BYTES + 1)
        .read_to_string(&mut source)
        .map_err(|error| error.to_string())?;
    if source.len() as u64 > MAX_THEME_BYTES {
        return Err(format!("theme exceeds {MAX_THEME_BYTES} bytes"));
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "ghostty".to_string());
    let document = GhosttyThemeAdapter::parse(&source).map_err(|error| error.to_string())?;
    let draft = GhosttyThemeAdapter::new(name)
        .adapt(&document)
        .map_err(|error| error.to_string())?;
    ThemeResolver::resolve(draft, Some(fallback)).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::ThemeConfig, theme::ThemeName};

    fn source(path: &std::path::Path) -> ThemeConfig {
        ThemeConfig::Ghostty {
            provider: "ghostty".to_string(),
            path: path.to_path_buf(),
            fallback: "default".to_string(),
        }
    }

    fn config() -> &'static str {
        "background = #101010\nforeground = #f0f0f0\npalette = 1=#ff0000\n"
    }

    fn request(effect: Effect) -> Box<dyn FeatureEffect> {
        let Effect::Feature(request) = effect else {
            panic!("expected theme-load effect");
        };
        request
    }

    fn complete(model: &mut Model, effect: Box<dyn FeatureEffect>) {
        let Message::FeatureCompletion(completion) = effect.perform() else {
            panic!("expected theme-load completion");
        };
        completion.update(model);
    }

    #[test]
    fn built_in_names_do_not_schedule_loading() {
        let mut model = Model::default();
        assert!(start_loading(&ThemeConfig::Name("default".to_string()), &mut model).is_none());
        assert_eq!(model.theme_loading.revision, 0);
    }

    #[test]
    fn valid_ghostty_load_installs_atomically() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ghostty-test");
        std::fs::write(&path, config()).unwrap();
        let mut model = Model::default();

        let effect = start_loading(&source(&path), &mut model).unwrap();
        assert_eq!(model.theme.builtin(), Some(ThemeName::Default));
        complete(&mut model, request(effect));

        assert_eq!(model.theme.name(), "ghostty-test");
        assert_eq!(model.theme.builtin(), None);
        assert_eq!(
            model.theme.ui().base.bg,
            Some(ratatui::style::Color::Rgb(16, 16, 16))
        );
        assert!(!model.theme_loading.loading);
        assert_eq!(
            model.notifications.current().unwrap().title(),
            Some("Theme")
        );
    }

    #[test]
    fn failure_preserves_the_previous_theme() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("invalid");
        std::fs::write(&path, "background = #000000\n").unwrap();
        let mut model = Model::default();

        let effect = start_loading(&source(&path), &mut model).unwrap();
        complete(&mut model, request(effect));

        assert_eq!(model.theme.name(), "default");
        assert_eq!(model.theme.builtin(), Some(ThemeName::Default));
        assert!(
            model
                .status
                .as_ref()
                .unwrap()
                .text
                .contains("Theme load failed")
        );
        assert!(!model.theme_loading.loading);
    }

    #[test]
    fn stale_completion_cannot_replace_a_newer_load() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first");
        let second = directory.path().join("second");
        std::fs::write(&first, config()).unwrap();
        std::fs::write(&second, config()).unwrap();
        let mut model = Model::default();

        let first_effect = start_loading(&source(&first), &mut model).unwrap();
        let second_effect = start_loading(&source(&second), &mut model).unwrap();
        complete(&mut model, request(first_effect));
        assert_eq!(model.theme.name(), "default");
        assert!(model.theme_loading.loading);

        complete(&mut model, request(second_effect));
        assert_eq!(model.theme.name(), "second");
        assert!(!model.theme_loading.loading);
    }
}
