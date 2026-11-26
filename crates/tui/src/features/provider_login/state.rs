use std::fmt;

use super::openai::{OpenAiField, OpenAiForm};
use crate::{
    component::{
        list::ListState,
        text_input::{TextInput, TextInputEvent},
    },
    data_object::Context,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMethod {
    Account,
    ApiKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSpec {
    id: &'static str,
    method: ProviderAuthMethod,
    openai_compatible_endpoint: Option<&'static str>,
}

impl ProviderSpec {
    pub(super) const fn new(id: &'static str, method: ProviderAuthMethod) -> Self {
        Self {
            id,
            method,
            openai_compatible_endpoint: None,
        }
    }

    pub(super) const fn with_openai_compatible_endpoint(mut self, endpoint: &'static str) -> Self {
        self.openai_compatible_endpoint = Some(endpoint);
        self
    }

    pub fn id(self) -> &'static str {
        self.id
    }

    pub fn method(self) -> ProviderAuthMethod {
        self.method
    }

    pub fn openai_compatible_endpoint(self) -> Option<&'static str> {
        self.openai_compatible_endpoint
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginOption {
    Method(ProviderAuthMethod),
    Provider(ProviderSpec),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginPage {
    AuthenticationMethod,
    Providers(ProviderAuthMethod),
    Placeholder(ProviderSpec),
    ApiKeyInput(ProviderSpec),
    OpenAiInput(ProviderSpec),
    OpenAiModelsLoading(ProviderSpec),
    OpenAiModelSelection(ProviderSpec),
    OpenAiActivating(ProviderSpec),
}

impl ProviderLoginPage {
    fn is_openai_input(self) -> bool {
        if let ProviderLoginPage::OpenAiInput(_) = self {
            return true;
        }
        false
    }

    fn accepts_error(self) -> bool {
        if self.is_openai_input() {
            return true;
        }
        if let ProviderLoginPage::ApiKeyInput(_) = self {
            return true;
        }
        false
    }
}

pub enum ProviderLoginSubmission {
    OpenAi {
        provider: ProviderSpec,
        endpoint: String,
        api_key: String,
    },
    ApiKey {
        provider: ProviderSpec,
        api_key: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginBack {
    Back,
    Close,
}

/// Provider-login menu navigation, secret draft, and parent-dialog state.
#[derive(Clone, PartialEq, Eq)]
pub struct ProviderLoginState {
    open: bool,
    return_context: Option<Context>,
    page: ProviderLoginPage,
    options: ListState<ProviderLoginOption>,
    api_key_input: TextInput,
    openai: OpenAiForm,
    models: ListState<String>,
    saving: bool,
    error: Option<String>,
}

impl fmt::Debug for ProviderLoginState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderLoginState")
            .field("open", &self.open)
            .field("return_context", &self.return_context)
            .field("page", &self.page)
            .field("options", &self.options)
            .field(
                "api_key",
                &(!self.api_key_input.value().is_empty()).then_some("[redacted]"),
            )
            .field("openai_focused", &self.openai.focused_field())
            .field(
                "openai_api_key",
                &(!self.openai.input(OpenAiField::ApiKey).value().is_empty())
                    .then_some("[redacted]"),
            )
            .field("selected_model", &self.models.selected_item())
            .field("saving", &self.saving)
            .field("error", &self.error)
            .finish()
    }
}

impl Default for ProviderLoginState {
    fn default() -> Self {
        Self {
            open: false,
            return_context: None,
            page: ProviderLoginPage::AuthenticationMethod,
            options: authentication_options(),
            api_key_input: TextInput::default(),
            openai: OpenAiForm::default(),
            models: ListState::default(),
            saving: false,
            error: None,
        }
    }
}

impl ProviderLoginState {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, return_context: Option<Context>) {
        self.open = true;
        self.return_context = return_context;
        self.page = ProviderLoginPage::AuthenticationMethod;
        self.options = authentication_options();
        self.api_key_input.clear();
        self.openai.reset();
        self.models = ListState::default();
        self.saving = false;
        self.error = None;
    }

    pub fn close(&mut self) -> Option<Context> {
        self.open = false;
        self.api_key_input.clear();
        self.openai.reset();
        self.models = ListState::default();
        self.saving = false;
        self.error = None;
        self.return_context.take()
    }

    pub fn page(&self) -> ProviderLoginPage {
        self.page
    }

    pub fn options(&self) -> &[ProviderLoginOption] {
        self.options.items()
    }

    pub fn selected(&self) -> Option<usize> {
        self.options.selected()
    }

    pub fn api_key_input(&self) -> &TextInput {
        &self.api_key_input
    }

    pub fn openai_input(&self, field: OpenAiField) -> &TextInput {
        self.openai.input(field)
    }

    pub fn openai_focused_field(&self) -> OpenAiField {
        self.openai.focused_field()
    }

    pub fn models(&self) -> &[String] {
        self.models.items()
    }

    pub fn model_selected(&self) -> Option<usize> {
        self.models.selected()
    }

    pub fn selected_model(&self) -> Option<&str> {
        self.models.selected_item().map(String::as_str)
    }

    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn select_next(&mut self) {
        if self.saving {
            return;
        }
        match self.page {
            ProviderLoginPage::AuthenticationMethod | ProviderLoginPage::Providers(_) => {
                self.options.select_next();
            }
            ProviderLoginPage::OpenAiModelSelection(_) => {
                self.models.select_next();
            }
            _ => {}
        }
    }

    pub fn select_previous(&mut self) {
        if self.saving {
            return;
        }
        match self.page {
            ProviderLoginPage::AuthenticationMethod | ProviderLoginPage::Providers(_) => {
                self.options.select_previous();
            }
            ProviderLoginPage::OpenAiModelSelection(_) => {
                self.models.select_previous();
            }
            _ => {}
        }
    }

    pub fn activate(&mut self) {
        if self.saving {
            return;
        }
        let Some(option) = self.options.selected_item().copied() else {
            return;
        };
        match (self.page, option) {
            (ProviderLoginPage::AuthenticationMethod, ProviderLoginOption::Method(method)) => {
                self.page = ProviderLoginPage::Providers(method);
                self.options = provider_options(method);
            }
            (ProviderLoginPage::Providers(_), ProviderLoginOption::Provider(provider)) => {
                self.page = match provider.method() {
                    ProviderAuthMethod::Account => ProviderLoginPage::Placeholder(provider),
                    ProviderAuthMethod::ApiKey => match provider.openai_compatible_endpoint() {
                        Some(endpoint) => {
                            self.openai.reset_to(endpoint);
                            ProviderLoginPage::OpenAiInput(provider)
                        }
                        None => ProviderLoginPage::ApiKeyInput(provider),
                    },
                };
                self.api_key_input.clear();
                self.error = None;
            }
            _ => {}
        }
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        if self.saving {
            return;
        }
        let changed = match self.page {
            ProviderLoginPage::ApiKeyInput(_) => {
                self.api_key_input.handle_event(event).content_changed()
            }
            ProviderLoginPage::OpenAiInput(_) => self.openai.handle_input(event),
            _ => return,
        };
        if changed {
            self.error = None;
        }
    }

    pub fn submission(&self) -> Option<ProviderLoginSubmission> {
        if self.saving {
            return None;
        }
        match self.page {
            ProviderLoginPage::ApiKeyInput(provider) => Some(ProviderLoginSubmission::ApiKey {
                provider,
                api_key: self.api_key_input.value().to_string(),
            }),
            ProviderLoginPage::OpenAiInput(provider) => {
                let (endpoint, api_key) = self.openai.values();
                Some(ProviderLoginSubmission::OpenAi {
                    provider,
                    endpoint: endpoint.to_string(),
                    api_key: api_key.to_string(),
                })
            }
            _ => None,
        }
    }

    pub fn focus_next(&mut self) {
        if self.page.is_openai_input() && !self.saving {
            self.openai.focus_next();
        }
    }

    pub fn begin_openai_paste(&mut self) -> Option<u64> {
        if !self.page.is_openai_input() || self.saving {
            return None;
        }
        Some(self.openai.begin_paste())
    }

    pub fn complete_openai_paste(
        &mut self,
        revision: u64,
        text: Result<String, String>,
    ) -> Option<Result<(), String>> {
        if !self.page.is_openai_input() {
            return None;
        }
        let result = self.openai.complete_paste(revision, text)?;
        if result.is_ok() {
            self.error = None;
        }
        Some(result)
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        let accepts_error = self.page.accepts_error();
        if accepts_error {
            self.error = Some(error.into());
        }
    }

    pub fn begin_save(&mut self, provider: ProviderSpec) -> bool {
        let provider_matches = match self.page {
            ProviderLoginPage::ApiKeyInput(current) | ProviderLoginPage::OpenAiInput(current) => {
                current == provider
            }
            _ => false,
        };
        if !provider_matches || self.saving {
            return false;
        }
        self.saving = true;
        self.error = None;
        self.openai.invalidate();
        true
    }

    /// Return whether the current save completed successfully, or `None` for a stale completion.
    pub fn complete_save(&mut self, provider_id: &str, error: Option<String>) -> Option<bool> {
        let provider = match self.page {
            ProviderLoginPage::ApiKeyInput(provider) | ProviderLoginPage::OpenAiInput(provider) => {
                provider
            }
            _ => return None,
        };
        if !self.saving || provider.id() != provider_id {
            return None;
        }
        self.saving = false;
        self.error = error;
        Some(self.error.is_none())
    }

    pub fn begin_model_loading(&mut self, provider_id: &str) -> Option<()> {
        let ProviderLoginPage::OpenAiInput(provider) = self.page else {
            return None;
        };
        if provider.id() != provider_id {
            return None;
        }
        self.page = ProviderLoginPage::OpenAiModelsLoading(provider);
        self.error = None;
        Some(())
    }

    pub fn complete_model_loading(
        &mut self,
        provider_id: &str,
        result: Result<Vec<String>, String>,
    ) -> Option<Result<(), String>> {
        if !self.open {
            return None;
        }
        let ProviderLoginPage::OpenAiModelsLoading(provider) = self.page else {
            return None;
        };
        if provider.id() != provider_id {
            return None;
        }
        match result {
            Ok(models) if !models.is_empty() => {
                self.models = selected_list(models);
                self.page = ProviderLoginPage::OpenAiModelSelection(provider);
                self.error = None;
                Some(Ok(()))
            }
            Ok(_) => {
                let error = format!("{} model catalog contained no models", provider.id());
                self.page = ProviderLoginPage::OpenAiInput(provider);
                self.error = Some(error.clone());
                Some(Err(error))
            }
            Err(error) => {
                self.page = ProviderLoginPage::OpenAiInput(provider);
                self.error = Some(error.clone());
                Some(Err(error))
            }
        }
    }

    pub fn begin_activation(&mut self, provider_id: &str) -> Option<String> {
        let ProviderLoginPage::OpenAiModelSelection(provider) = self.page else {
            return None;
        };
        if provider.id() != provider_id {
            return None;
        }
        let model = self.selected_model()?.to_string();
        self.page = ProviderLoginPage::OpenAiActivating(provider);
        self.error = None;
        Some(model)
    }

    pub fn complete_activation(
        &mut self,
        provider_id: &str,
        error: Option<String>,
    ) -> Option<bool> {
        if !self.open {
            return None;
        }
        let ProviderLoginPage::OpenAiActivating(provider) = self.page else {
            return None;
        };
        if provider.id() != provider_id {
            return None;
        }
        let succeeded = error.is_none();
        if let Some(error) = error {
            self.page = ProviderLoginPage::OpenAiModelSelection(provider);
            self.error = Some(error);
        } else {
            self.error = None;
        }
        Some(succeeded)
    }

    pub fn back(&mut self) -> ProviderLoginBack {
        match self.page {
            ProviderLoginPage::Placeholder(provider) => {
                self.page = ProviderLoginPage::Providers(provider.method());
                ProviderLoginBack::Back
            }
            ProviderLoginPage::ApiKeyInput(provider) => {
                if self.saving {
                    return ProviderLoginBack::Back;
                }
                self.api_key_input.clear();
                self.error = None;
                self.page = ProviderLoginPage::Providers(provider.method());
                ProviderLoginBack::Back
            }
            ProviderLoginPage::OpenAiInput(provider) => {
                if self.saving {
                    return ProviderLoginBack::Back;
                }
                self.openai.reset();
                self.error = None;
                self.page = ProviderLoginPage::Providers(provider.method());
                ProviderLoginBack::Back
            }
            ProviderLoginPage::OpenAiModelSelection(provider) => {
                self.models = ListState::default();
                self.error = None;
                self.page = ProviderLoginPage::OpenAiInput(provider);
                ProviderLoginBack::Back
            }
            ProviderLoginPage::OpenAiModelsLoading(_) | ProviderLoginPage::OpenAiActivating(_) => {
                ProviderLoginBack::Close
            }
            ProviderLoginPage::Providers(_) => {
                self.page = ProviderLoginPage::AuthenticationMethod;
                self.options = authentication_options();
                ProviderLoginBack::Back
            }
            ProviderLoginPage::AuthenticationMethod => ProviderLoginBack::Close,
        }
    }
}

const ACCOUNT_PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec::new("openai", ProviderAuthMethod::Account),
    ProviderSpec::new("qwen.com", ProviderAuthMethod::Account),
    ProviderSpec::new("kimi.com", ProviderAuthMethod::Account),
];

const API_KEY_PROVIDERS: &[ProviderSpec] = &[
    super::openai::PROVIDER,
    super::kimi_for_coding::PROVIDER,
    ProviderSpec::new("qwen-tokenplan-individual-cn", ProviderAuthMethod::ApiKey),
    ProviderSpec::new("deepseek", ProviderAuthMethod::ApiKey),
];

fn authentication_options() -> ListState<ProviderLoginOption> {
    selected_list(vec![
        ProviderLoginOption::Method(ProviderAuthMethod::Account),
        ProviderLoginOption::Method(ProviderAuthMethod::ApiKey),
    ])
}

fn provider_options(method: ProviderAuthMethod) -> ListState<ProviderLoginOption> {
    let providers = match method {
        ProviderAuthMethod::Account => ACCOUNT_PROVIDERS,
        ProviderAuthMethod::ApiKey => API_KEY_PROVIDERS,
    };
    selected_list(
        providers
            .iter()
            .copied()
            .map(ProviderLoginOption::Provider)
            .collect(),
    )
}

fn selected_list<T>(items: Vec<T>) -> ListState<T> {
    let mut list = ListState::new(items);
    list.select_current();
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option_ids(state: &ProviderLoginState) -> Vec<&'static str> {
        state
            .options()
            .iter()
            .map(|option| match option {
                ProviderLoginOption::Method(ProviderAuthMethod::Account) => "account",
                ProviderLoginOption::Method(ProviderAuthMethod::ApiKey) => "api_key",
                ProviderLoginOption::Provider(provider) => provider.id(),
            })
            .collect()
    }

    fn open_api_key(state: &mut ProviderLoginState) -> ProviderSpec {
        state.open(None);
        state.select_next();
        state.activate();
        for _ in 0..3 {
            state.select_next();
        }
        state.activate();
        let ProviderLoginPage::ApiKeyInput(provider) = state.page() else {
            panic!("API-key provider should open its input");
        };
        provider
    }

    #[test]
    fn catalogs_have_the_requested_provider_order() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        assert_eq!(option_ids(&state), ["account", "api_key"]);

        state.activate();
        assert_eq!(option_ids(&state), ["openai", "qwen.com", "kimi.com"]);

        state.back();
        state.select_next();
        state.activate();
        assert_eq!(
            option_ids(&state),
            [
                "openai",
                "kimi-for-coding",
                "qwen-tokenplan-individual-cn",
                "deepseek",
            ]
        );
    }

    #[test]
    fn selection_activation_and_back_preserve_menu_navigation() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        assert_eq!(state.selected(), Some(0));
        state.select_previous();
        assert_eq!(state.selected(), Some(1));
        state.activate();
        assert_eq!(
            state.page(),
            ProviderLoginPage::Providers(ProviderAuthMethod::ApiKey)
        );

        state.select_next();
        state.activate();
        match state.page() {
            ProviderLoginPage::OpenAiInput(provider) => {
                assert_eq!(provider.id(), "kimi-for-coding");
                assert_eq!(
                    state.openai_input(OpenAiField::Endpoint).value(),
                    crate::provider_login::DEFAULT_KIMI_FOR_CODING_ENDPOINT
                );
            }
            other => panic!("expected Kimi's OpenAI-compatible input, got {other:?}"),
        }
        let selected = state.selected();
        state.select_next();
        assert_eq!(state.selected(), selected);

        assert_eq!(state.back(), ProviderLoginBack::Back);
        assert_eq!(
            state.page(),
            ProviderLoginPage::Providers(ProviderAuthMethod::ApiKey)
        );
        assert_eq!(state.back(), ProviderLoginBack::Back);
        assert_eq!(state.page(), ProviderLoginPage::AuthenticationMethod);
        assert_eq!(state.back(), ProviderLoginBack::Close);
    }

    fn open_openai(state: &mut ProviderLoginState) -> ProviderSpec {
        state.open(None);
        state.select_next();
        state.activate();
        state.activate();
        let ProviderLoginPage::OpenAiInput(provider) = state.page() else {
            panic!("OpenAI input should be selected");
        };
        provider
    }

    fn open_model_selection(state: &mut ProviderLoginState) -> ProviderSpec {
        let provider = open_openai(state);
        state.begin_model_loading(provider.id()).unwrap();
        state
            .complete_model_loading(provider.id(), Ok(vec!["gpt-test".to_string()]))
            .unwrap()
            .unwrap();
        provider
    }

    #[test]
    fn loading_and_activation_close_without_accepting_late_completions() {
        let mut loading = ProviderLoginState::default();
        let provider = open_openai(&mut loading);
        loading.begin_model_loading(provider.id()).unwrap();
        assert_eq!(loading.back(), ProviderLoginBack::Close);
        loading.close();
        assert_eq!(
            loading.complete_model_loading(provider.id(), Ok(vec!["late".to_string()])),
            None
        );

        let mut activation = ProviderLoginState::default();
        let provider = open_model_selection(&mut activation);
        activation.begin_activation(provider.id()).unwrap();
        assert_eq!(activation.back(), ProviderLoginBack::Close);
        activation.close();
        assert_eq!(activation.complete_activation(provider.id(), None), None);
    }

    #[test]
    fn secret_drafts_are_redacted_and_save_completions_are_scoped() {
        let mut state = ProviderLoginState::default();
        let provider = open_api_key(&mut state);
        for character in "key-secret".chars() {
            state.handle_input_event(TextInputEvent::Insert(character));
        }
        assert_eq!(state.api_key_input().value(), "key-secret");
        let debug = format!("{state:?}");
        assert!(!debug.contains("key-secret"));
        assert!(debug.contains("[redacted]"));

        assert!(state.begin_save(provider));
        state.handle_input_event(TextInputEvent::Insert('x'));
        assert_eq!(state.api_key_input().value(), "key-secret");
        assert_eq!(state.complete_save("other-provider", None), None);
        assert_eq!(state.complete_save(provider.id(), None), Some(true));
        assert!(!state.is_saving());

        assert!(state.begin_save(provider));
        assert_eq!(
            state.complete_save(provider.id(), Some("failed".to_string())),
            Some(false)
        );
        assert_eq!(state.error(), Some("failed"));
        state.handle_input_event(TextInputEvent::Insert('x'));
        assert_eq!(state.error(), None);
    }
}
