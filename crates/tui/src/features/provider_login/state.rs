use crate::{component::list::ListState, data_object::Context};

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLoginBack {
    Back,
    Close,
}

/// Provider-login menu navigation and parent-dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderLoginState {
    open: bool,
    return_context: Option<Context>,
    page: ProviderLoginPage,
    options: ListState<ProviderLoginOption>,
}

impl Default for ProviderLoginState {
    fn default() -> Self {
        Self {
            open: false,
            return_context: None,
            page: ProviderLoginPage::AuthenticationMethod,
            options: authentication_options(),
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
    }

    pub fn close(&mut self) -> Option<Context> {
        self.open = false;
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

    pub fn select_next(&mut self) {
        if !matches!(self.page, ProviderLoginPage::Placeholder(_)) {
            self.options.select_next();
        }
    }

    pub fn select_previous(&mut self) {
        if !matches!(self.page, ProviderLoginPage::Placeholder(_)) {
            self.options.select_previous();
        }
    }

    pub fn activate(&mut self) {
        let Some(option) = self.options.selected_item().copied() else {
            return;
        };
        match (self.page, option) {
            (ProviderLoginPage::AuthenticationMethod, ProviderLoginOption::Method(method)) => {
                self.page = ProviderLoginPage::Providers(method);
                self.options = provider_options(method);
            }
            (ProviderLoginPage::Providers(_), ProviderLoginOption::Provider(provider)) => {
                self.page = ProviderLoginPage::Placeholder(provider);
            }
            _ => {}
        }
    }

    pub fn back(&mut self) -> ProviderLoginBack {
        match self.page {
            ProviderLoginPage::Placeholder(provider) => {
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
        assert!(matches!(state.page(), ProviderLoginPage::Placeholder(_)));
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
}
