use std::path::PathBuf;

use crate::{
    config::ThemeConfig,
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

#[derive(Debug, Default)]
pub struct ThemePersistenceState {
    revision: u64,
    in_flight: bool,
    trailing: Option<ThemeConfig>,
    path: Option<PathBuf>,
}

impl ThemePersistenceState {
    fn begin(&mut self, config: ThemeConfig, path: PathBuf) -> Option<Effect> {
        self.path = Some(path);
        if self.in_flight {
            self.trailing = Some(config);
            return None;
        }
        self.in_flight = true;
        self.trailing = None;
        self.revision = self.revision.wrapping_add(1);
        Some(self.request(config, self.revision))
    }

    fn request(&self, config: ThemeConfig, revision: u64) -> Effect {
        Effect::feature(ThemePersistRequest {
            config,
            revision,
            path: self
                .path
                .clone()
                .expect("persistence path must be initialized"),
        })
    }

    fn complete(&mut self, revision: u64, result: Result<(), String>) -> PersistOutcome {
        if !self.in_flight || self.revision != revision {
            return PersistOutcome::default();
        }
        self.in_flight = false;
        if self.trailing.is_some() {
            self.in_flight = true;
            self.revision = self.revision.wrapping_add(1);
        }
        PersistOutcome {
            effect: self
                .trailing
                .take()
                .map(|config| self.request(config, self.revision)),
            error: result.err(),
        }
    }
}

#[derive(Default)]
struct PersistOutcome {
    effect: Option<Effect>,
    error: Option<String>,
}

pub fn persist_theme(config: ThemeConfig, state: &mut ThemePersistenceState) -> Option<Effect> {
    let path = std::env::home_dir()
        .map(|home| home.join(".eggplant").join("config.toml"))
        .expect("home directory must be available to persist a theme");
    state.begin(config, path)
}

struct ThemePersistRequest {
    config: ThemeConfig,
    revision: u64,
    path: PathBuf,
}

impl FeatureEffect for ThemePersistRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = crate::config::save_theme_to(&self.path, &self.config);
        Message::FeatureCompletion(FeatureCompletion::new(ThemePersistCompletion {
            revision: self.revision,
            result,
        }))
    }
}

struct ThemePersistCompletion {
    revision: u64,
    result: Result<(), String>,
}

impl Completion for ThemePersistCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let outcome = model.theme_persistence.complete(self.revision, self.result);
        let error = outcome.error.map(|error| {
            crate::notifications::report_status(
                model,
                StatusMessage::error(format!("Failed to save theme: {error}")),
            )
        });
        Effect::combine(outcome.effect, error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::Config, theme::ThemeName};

    fn request(effect: Effect) -> Box<dyn FeatureEffect> {
        let Effect::Feature(request) = effect else {
            panic!("expected persistence effect");
        };
        request
    }

    fn complete(model: &mut Model, effect: Box<dyn FeatureEffect>) -> Option<Effect> {
        let Message::FeatureCompletion(completion) = effect.perform() else {
            panic!("expected persistence completion");
        };
        completion.update(model)
    }

    fn persist_with_path(config: ThemeConfig, model: &mut Model, path: PathBuf) -> Option<Effect> {
        model.theme_persistence.begin(config, path)
    }

    fn complete_all(model: &mut Model, effect: Effect) {
        match effect {
            Effect::Feature(request) => {
                if let Some(next) = complete(model, request) {
                    complete_all(model, next);
                }
            }
            Effect::Batch(effects) => {
                for effect in effects {
                    complete_all(model, effect);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn trailing_selection_is_written_after_the_inflight_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut model = Model::default();
        let first = persist_with_path(
            ThemeConfig::Name(ThemeName::TokyoNight.as_str().to_string()),
            &mut model,
            path.clone(),
        )
        .unwrap();
        assert!(
            persist_with_path(
                ThemeConfig::Name(ThemeName::CatppuccinMocha.as_str().to_string()),
                &mut model,
                path.clone(),
            )
            .is_none()
        );

        let trailing = complete(&mut model, request(first)).unwrap();
        let trailing = request(trailing);
        complete(&mut model, trailing);

        let config = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.theme.fallback_name(), "catppuccin_mocha");
        assert!(!model.theme_persistence.in_flight);
    }

    #[test]
    fn save_failure_still_allows_the_latest_selection() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut model = Model::default();
        let _first = persist_with_path(
            ThemeConfig::Name("invalid name".to_string()),
            &mut model,
            path.clone(),
        )
        .unwrap();
        assert!(
            persist_with_path(
                ThemeConfig::Name(ThemeName::TokyoNight.as_str().to_string()),
                &mut model,
                path.clone(),
            )
            .is_none()
        );

        let trailing = Box::new(ThemePersistCompletion {
            revision: 1,
            result: Err("write failed".to_string()),
        })
        .update(&mut model)
        .unwrap();
        assert!(model.status.is_some());
        complete_all(&mut model, trailing);
        let config = Config::parse(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(config.theme.fallback_name(), "tokyonight");
    }
}
