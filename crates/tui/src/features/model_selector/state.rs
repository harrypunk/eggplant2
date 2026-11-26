use std::{collections::BTreeSet, fmt};

use eggplant_agent_storage::ModelCatalog;

use crate::{component::list::ListState, data_object::Context};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProviderModel {
    provider_id: String,
    model_id: String,
}

impl ProviderModel {
    pub fn new(provider_id: impl Into<String>, model_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
        }
    }

    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }
}

impl fmt::Display for ProviderModel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.provider_id, self.model_id)
    }
}

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
    models: ListState<ProviderModel>,
    active_model: Option<ProviderModel>,
    notice: Option<ModelSelectorNotice>,
    generation: u64,
}

impl Default for ModelSelectorState {
    fn default() -> Self {
        Self {
            page: ModelSelectorPage::Closed,
            return_context: None,
            models: ListState::default(),
            active_model: None,
            notice: None,
            generation: 0,
        }
    }
}

impl ModelSelectorState {
    pub fn open(&mut self, return_context: Option<Context>, active_model: Option<ProviderModel>) {
        self.page = ModelSelectorPage::Loading;
        self.return_context = return_context;
        self.active_model = active_model;
        self.models = ListState::default();
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn close(&mut self) -> Option<Context> {
        self.page = ModelSelectorPage::Closed;
        self.models = ListState::default();
        self.notice = None;
        self.generation = self.generation.wrapping_add(1);
        self.return_context.take()
    }

    pub fn page(&self) -> ModelSelectorPage {
        self.page
    }

    pub fn models(&self) -> &[ProviderModel] {
        self.models.items()
    }

    pub fn selected(&self) -> Option<usize> {
        self.models.selected()
    }

    pub fn selected_model(&self) -> Option<&ProviderModel> {
        self.models.selected_item()
    }

    pub fn active_model(&self) -> Option<&ProviderModel> {
        self.active_model.as_ref()
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

    pub fn apply_cached(&mut self, catalogs: Vec<ModelCatalog>) {
        self.replace_models(flatten(catalogs));
        self.page = ModelSelectorPage::Refreshing;
    }

    /// Merge fresh provider catalogs while retaining cached sections that failed to refresh.
    pub fn apply_refresh(&mut self, catalogs: Vec<ModelCatalog>, errors: Vec<String>) {
        let refreshed = catalogs
            .iter()
            .map(|catalog| catalog.provider_id.as_str())
            .collect::<BTreeSet<_>>();
        let mut models = self
            .models
            .items()
            .iter()
            .filter(|model| !refreshed.contains(model.provider_id()))
            .cloned()
            .collect::<Vec<_>>();
        models.extend(flatten(catalogs));
        models.sort();
        models.dedup();
        self.replace_models(models);
        self.page = ModelSelectorPage::Ready;
        self.notice = notice_for_errors(errors, self.models.items().is_empty());
    }

    pub fn fail_refresh(&mut self, error: String) {
        self.page = ModelSelectorPage::Ready;
        self.notice = Some(if self.models.items().is_empty() {
            ModelSelectorNotice::Error(error)
        } else {
            ModelSelectorNotice::Warning(error)
        });
    }

    pub fn begin_activation(&mut self) -> Option<(ProviderModel, u64)> {
        if self.page != ModelSelectorPage::Ready && self.page != ModelSelectorPage::Refreshing {
            return None;
        }
        let model = self.selected_model()?.clone();
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

    fn replace_models(&mut self, models: Vec<ProviderModel>) {
        let selection = match self.page {
            ModelSelectorPage::Loading => self.active_model.clone(),
            _ => self.selected_model().cloned(),
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

fn flatten(catalogs: Vec<ModelCatalog>) -> Vec<ProviderModel> {
    let mut models = catalogs
        .into_iter()
        .flat_map(|catalog| {
            let provider_id = catalog.provider_id;
            catalog
                .models
                .into_iter()
                .map(move |model_id| ProviderModel::new(provider_id.clone(), model_id))
        })
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models
}

fn notice_for_errors(errors: Vec<String>, empty: bool) -> Option<ModelSelectorNotice> {
    if errors.is_empty() {
        return None;
    }
    let error = errors.join("; ");
    Some(if empty {
        ModelSelectorNotice::Error(error)
    } else {
        ModelSelectorNotice::Warning(error)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog(provider_id: &str, models: &[&str]) -> ModelCatalog {
        ModelCatalog::new(
            provider_id,
            models.iter().map(|model| model.to_string()).collect(),
            1,
        )
        .unwrap()
    }

    #[test]
    fn grouped_selection_uses_full_identity_and_failed_sections_stay_cached() {
        let mut state = ModelSelectorState::default();
        state.open(None, Some(ProviderModel::new("openai", "gpt-b")));
        state.apply_cached(vec![
            catalog("openai", &["gpt-a", "gpt-b"]),
            catalog("qwen-tokenplan-individual-cn", &["qwen-a"]),
        ]);
        assert_eq!(
            state.selected_model(),
            Some(&ProviderModel::new("openai", "gpt-b"))
        );

        state.select_next();
        assert_eq!(
            state.selected_model(),
            Some(&ProviderModel::new(
                "qwen-tokenplan-individual-cn",
                "qwen-a"
            ))
        );
        state.apply_refresh(
            vec![catalog("openai", &["gpt-c"])],
            vec!["qwen refresh failed".to_string()],
        );

        assert_eq!(state.models().len(), 2);
        assert!(
            state
                .models()
                .iter()
                .any(|model| model.provider_id() == "qwen-tokenplan-individual-cn")
        );
        assert_eq!(
            state.selected_model().map(ToString::to_string),
            Some("qwen-tokenplan-individual-cn/qwen-a".to_string())
        );
        assert!(matches!(
            state.notice(),
            Some(ModelSelectorNotice::Warning(_))
        ));
        let (selected, _) = state.begin_activation().unwrap();
        assert_eq!(selected.provider_id(), "qwen-tokenplan-individual-cn");
        assert_eq!(selected.model_id(), "qwen-a");
    }

    #[test]
    fn activation_and_late_completions_are_scoped() {
        let mut state = ModelSelectorState::default();
        state.open(None, None);
        state.apply_cached(vec![catalog("openai", &["gpt-a"])]);
        let (model, generation) = state.begin_activation().unwrap();
        assert_eq!(model, ProviderModel::new("openai", "gpt-a"));
        assert_eq!(state.complete_activation(generation + 1, None), None);
        assert_eq!(state.complete_activation(generation, None), Some(true));
        state.close();
        state.open(None, None);
        state.apply_cached(vec![catalog("openai", &["gpt-a"])]);
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
