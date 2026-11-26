use eggplant_agent_storage::ProviderCredentialInfo;

use crate::{component::list::ListState, data_object::Context};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLogoutPage {
    Closed,
    Loading,
    Ready,
    Deleting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLogoutLoad {
    Ready,
    Empty,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLogoutState {
    page: ProviderLogoutPage,
    return_context: Option<Context>,
    providers: ListState<ProviderCredentialInfo>,
    error: Option<String>,
    generation: u64,
}

impl Default for ProviderLogoutState {
    fn default() -> Self {
        Self {
            page: ProviderLogoutPage::Closed,
            return_context: None,
            providers: ListState::default(),
            error: None,
            generation: 0,
        }
    }
}

impl ProviderLogoutState {
    pub fn open(&mut self, return_context: Option<Context>) {
        self.page = ProviderLogoutPage::Loading;
        self.return_context = return_context;
        self.providers = ListState::default();
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
    }

    pub fn close(&mut self) -> Option<Context> {
        self.page = ProviderLogoutPage::Closed;
        self.providers = ListState::default();
        self.error = None;
        self.generation = self.generation.wrapping_add(1);
        self.return_context.take()
    }

    pub fn page(&self) -> ProviderLogoutPage {
        self.page
    }

    pub fn providers(&self) -> &[ProviderCredentialInfo] {
        self.providers.items()
    }

    pub fn selected(&self) -> Option<usize> {
        self.providers.selected()
    }

    pub fn selected_provider_id(&self) -> Option<&str> {
        self.providers
            .selected_item()
            .map(|provider| provider.provider_id.as_str())
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn select_next(&mut self) {
        if self.page == ProviderLogoutPage::Ready {
            self.providers.select_next();
        }
    }

    pub fn select_previous(&mut self) {
        if self.page == ProviderLogoutPage::Ready {
            self.providers.select_previous();
        }
    }

    pub fn complete_load(
        &mut self,
        generation: u64,
        result: Result<Vec<ProviderCredentialInfo>, String>,
    ) -> Option<ProviderLogoutLoad> {
        if self.page != ProviderLogoutPage::Loading || generation != self.generation {
            return None;
        }
        match result {
            Ok(providers) if providers.is_empty() => Some(ProviderLogoutLoad::Empty),
            Ok(providers) => {
                self.providers = ListState::new(providers);
                self.providers.select_current();
                self.page = ProviderLogoutPage::Ready;
                self.error = None;
                Some(ProviderLogoutLoad::Ready)
            }
            Err(error) => {
                self.page = ProviderLogoutPage::Ready;
                self.error = Some(error);
                Some(ProviderLogoutLoad::Failed)
            }
        }
    }

    pub fn begin_delete(&mut self) -> Option<(String, u64)> {
        if self.page != ProviderLogoutPage::Ready {
            return None;
        }
        let provider_id = self.selected_provider_id()?.to_string();
        self.page = ProviderLogoutPage::Deleting;
        self.error = None;
        Some((provider_id, self.generation))
    }

    pub fn complete_delete(&mut self, generation: u64, error: Option<String>) -> Option<bool> {
        if self.page != ProviderLogoutPage::Deleting || generation != self.generation {
            return None;
        }
        if let Some(error) = error {
            self.page = ProviderLogoutPage::Ready;
            self.error = Some(error);
            return Some(false);
        }
        Some(true)
    }
}

#[cfg(test)]
mod tests {
    use eggplant_agent_storage::ProviderAuthType;

    use super::*;

    fn provider(provider_id: &str) -> ProviderCredentialInfo {
        ProviderCredentialInfo {
            provider_id: provider_id.to_string(),
            auth_type: ProviderAuthType::ApiKey,
        }
    }

    #[test]
    fn navigation_and_completion_identity_are_explicit() {
        let mut state = ProviderLogoutState::default();
        state.open(None);
        let generation = state.generation();
        assert_eq!(
            state.complete_load(generation, Ok(vec![provider("openai"), provider("kimi")])),
            Some(ProviderLogoutLoad::Ready)
        );
        assert_eq!(state.selected_provider_id(), Some("openai"));
        state.select_next();
        assert_eq!(state.selected_provider_id(), Some("kimi"));
        state.select_previous();
        assert_eq!(state.selected_provider_id(), Some("openai"));

        let (_, deletion_generation) = state.begin_delete().unwrap();
        assert_eq!(state.complete_delete(deletion_generation + 1, None), None);
        assert_eq!(state.complete_delete(deletion_generation, None), Some(true));
        state.close();
        assert_eq!(
            state.complete_load(generation, Ok(vec![provider("late")])),
            None
        );
    }
}
