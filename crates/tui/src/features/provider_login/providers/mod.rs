pub mod kimi_for_coding;
pub mod openai;
pub mod qwen_tokenplan_individual_cn;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthMethod {
    Account,
    ApiKey,
}

/// Immutable built-in provider metadata consumed by login workflows.
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

pub(super) const ACCOUNT_PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec::new("openai", ProviderAuthMethod::Account),
    ProviderSpec::new("qwen.com", ProviderAuthMethod::Account),
    ProviderSpec::new("kimi.com", ProviderAuthMethod::Account),
];

pub(super) const API_KEY_PROVIDERS: &[ProviderSpec] = &[
    openai::PROVIDER,
    kimi_for_coding::PROVIDER,
    qwen_tokenplan_individual_cn::PROVIDER,
    ProviderSpec::new("deepseek", ProviderAuthMethod::ApiKey),
];
