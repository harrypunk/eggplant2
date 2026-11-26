use eggplant_agent_storage::ModelCatalog;

use crate::{component::list::ListState, data_object::Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSelectorPage {
    Closed,
    Loading,
    Refreshing,
    Ready,
    Switching,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSelectorNotice {
    Warning(String),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSelectorState {
    page: ModelSelectorPage,
    return_context: Option<Context>,
    provider_id: Option<String>,
    models: ListState<String>,
    active_model: Option<String>,
    notice: Option<ModelSelectorNotice>,
    generation: u64,
}

impl Default for ModelSelectorState {
    fn default() -> Self {
        Self {
            page: ModelSelectorPage::Closed,
            return_context: None,
            provider_id: None,
            models: ListState::default(),
            active_model: None,
            notice: None,
            generation: 0,
        }
    }
}

impl ModelSelectorState {
    pub fn open(
        &mut self,
        return_context: Option<Context>,
        provider_id: String,
        active_model: Option<String>,
    ) {
        self.page = ModelSelectorPage::Loading;
        self.return_context = return_context;
        self.provider_id = Some(provider_id);
        self.active_model = active_model;
        self.models = ListState::default();
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn close(&mut self) -> Option<Context> {
        self.page = ModelSelectorPage::Closed;
        self.provider_id = None;
        self.models = ListState::default();
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
        self.return_context.take()
    }

    pub fn page(&self) -> ModelSelectorPage {
        self.page
    }

    pub fn provider_id(&self) -> Option<&str> {
        self.provider_id.as_deref()
    }

    pub fn models(&self) -> &[String] {
        self.models.items()
    }

    pub fn selected(&self) -> Option<usize> {
        self.models.selected()
    }

    pub fn selected_model(&self) -> Option<&str> {
        self.models.selected_item().map(String::as_str)
    }

    pub fn active_model(&self) -> Option<&str> {
        self.active_model.as_deref()
    }

    pub fn notice(&self) -> Option<&ModelSelectorNotice> {
        self.notice.as_ref()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn select_next(&mut self) {
        match self.page {
            ModelSelectorPage::Refreshing | ModelSelectorPage::Ready => {
                self.models.select_next();
            }
            _ => {}
        }
    }

    pub fn select_previous(&mut self) {
        match self.page {
            ModelSelectorPage::Refreshing | ModelSelectorPage::Ready => {
                self.models.select_previous();
            }
            _ => {}
        }
    }

    pub fn apply_cached(&mut self, catalog: ModelCatalog) {
        self.set_models(catalog.models);
        self.page = ModelSelectorPage::Refreshing;
    }

    pub fn apply_refresh(&mut self, catalog: ModelCatalog) {
        self.set_models(catalog.models);
        self.notice = None;
        self.page = ModelSelectorPage::Ready;
    }

    pub fn fail_refresh(&mut self, error: String) {
        self.page = ModelSelectorPage::Ready;
        self.notice = Some(if self.models.items().is_empty() {
            ModelSelectorNotice::Error(error)
        } else {
            ModelSelectorNotice::Warning(error)
        });
    }

    pub fn begin_activation(&mut self) -> Option<(String, u64)> {
        if self.page != ModelSelectorPage::Ready && self.page != ModelSelectorPage::Refreshing {
            return None;
        }
        let model = self.selected_model()?.to_string();
        self.page = ModelSelectorPage::Switching;
        self.notice = None;
        Some((model, self.generation))
    }

    pub fn complete_activation(&mut self, generation: u64, error: Option<String>) -> Option<bool> {
        if self.page != ModelSelectorPage::Switching || generation != self.generation {
            return None;
        }
        let succeeded = error.is_none();
        if let Some(error) = error {
            self.page = ModelSelectorPage::Ready;
            self.notice = Some(ModelSelectorNotice::Error(error));
        }
        Some(succeeded)
    }

    fn set_models(&mut self, models: Vec<String>) {
        let selection = match self.page {
            ModelSelectorPage::Loading => self.active_model.clone(),
            _ => self.selected_model().map(str::to_string),
        };
        self.models = ListState::new(models);
        if let Some(index) = selection.and_then(|selected| {
            self.models
                .items()
                .iter()
                .position(|model| *model == selected)
        }) {
            self.models.select(Some(index));
        } else {
            self.models.clear_selection();
            self.models.select_current();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(models: &[&str]) -> ModelCatalog {
        ModelCatalog::new(
            "openai",
            models.iter().map(|model| model.to_string()).collect(),
            1,
        )
        .unwrap()
    }

    #[test]
    fn cached_selection_tracks_active_model_and_refresh_preserves_it() {
        let mut state = ModelSelectorState::default();
        state.open(None, "openai".to_string(), Some("gpt-b".to_string()));
        state.apply_cached(catalog(&["gpt-a", "gpt-b", "gpt-c"]));
        assert_eq!(state.selected_model(), Some("gpt-b"));
        state.select_next();
        assert_eq!(state.selected_model(), Some("gpt-c"));

        state.apply_refresh(catalog(&["gpt-b", "gpt-c", "gpt-d"]));
        assert_eq!(state.selected_model(), Some("gpt-c"));
        state.fail_refresh("offline".to_string());
        assert!(matches!(
            state.notice(),
            Some(ModelSelectorNotice::Warning(_))
        ));
        assert_eq!(state.models().len(), 3);
    }

    #[test]
    fn activation_and_late_completions_are_scoped() {
        let mut state = ModelSelectorState::default();
        state.open(None, "openai".to_string(), None);
        state.apply_cached(catalog(&["gpt-a"]));
        let (model, generation) = state.begin_activation().unwrap();
        assert_eq!(model, "gpt-a");
        assert_eq!(state.complete_activation(generation + 1, None), None);
        assert_eq!(state.complete_activation(generation, None), Some(true));
        state.close();
        state.open(None, "openai".to_string(), None);
        state.apply_cached(catalog(&["gpt-a"]));
        let (_, generation) = state.begin_activation().unwrap();
        assert_eq!(
            state.complete_activation(generation, Some("failed".to_string())),
            Some(false)
        );
        assert!(matches!(
            state.notice(),
            Some(ModelSelectorNotice::Error(_))
        ));
    }
}
