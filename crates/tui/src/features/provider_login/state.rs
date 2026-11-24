use std::fmt;

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
}

impl ProviderSpec {
    const fn new(id: &'static str, method: ProviderAuthMethod) -> Self {
        Self { id, method }
    }

    pub fn id(self) -> &'static str {
        self.id
    }

    pub fn method(self) -> ProviderAuthMethod {
        self.method
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
        self.saving = false;
        self.error = None;
    }

    pub fn close(&mut self) -> Option<Context> {
        self.open = false;
        self.api_key_input.clear();
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

    pub fn is_saving(&self) -> bool {
        self.saving
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn select_next(&mut self) {
        if !self.saving
            && matches!(
                self.page,
                ProviderLoginPage::AuthenticationMethod | ProviderLoginPage::Providers(_)
            )
        {
            self.options.select_next();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.saving
            && matches!(
                self.page,
                ProviderLoginPage::AuthenticationMethod | ProviderLoginPage::Providers(_)
            )
        {
            self.options.select_previous();
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
                    ProviderAuthMethod::ApiKey => ProviderLoginPage::ApiKeyInput(provider),
                };
                self.api_key_input.clear();
                self.error = None;
            }
            _ => {}
        }
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        if !matches!(self.page, ProviderLoginPage::ApiKeyInput(_)) || self.saving {
            return;
        }
        if self.api_key_input.handle_event(event).content_changed() {
            self.error = None;
        }
    }

    pub fn api_key_submission(&self) -> Option<(ProviderSpec, &str)> {
        match self.page {
            ProviderLoginPage::ApiKeyInput(provider) if !self.saving => {
                Some((provider, self.api_key_input.value()))
            }
            _ => None,
        }
    }

    pub fn set_error(&mut self, error: impl Into<String>) {
        if matches!(self.page, ProviderLoginPage::ApiKeyInput(_)) {
            self.error = Some(error.into());
        }
    }

    pub fn begin_save(&mut self, provider: ProviderSpec) -> bool {
        if self.page != ProviderLoginPage::ApiKeyInput(provider) || self.saving {
            return false;
        }
        self.saving = true;
        self.error = None;
        true
    }

    /// Return whether the current save completed successfully, or `None` for a stale completion.
    pub fn complete_save(&mut self, provider_id: &str, error: Option<String>) -> Option<bool> {
        let ProviderLoginPage::ApiKeyInput(provider) = self.page else {
            return None;
        };
        if !self.saving || provider.id() != provider_id {
            return None;
        }
        self.saving = false;
        self.error = error;
        Some(self.error.is_none())
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
    ProviderSpec::new("openai", ProviderAuthMethod::ApiKey),
    ProviderSpec::new("kimi-for-coding", ProviderAuthMethod::ApiKey),
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

fn selected_list(items: Vec<ProviderLoginOption>) -> ListState<ProviderLoginOption> {
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
        assert!(matches!(state.page(), ProviderLoginPage::ApiKeyInput(_)));
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
        assert_eq!(state.complete_save("deepseek", None), None);
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
