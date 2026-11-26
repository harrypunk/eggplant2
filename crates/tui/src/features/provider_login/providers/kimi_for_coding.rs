use super::{ProviderAuthMethod, ProviderSpec};

pub const DEFAULT_ENDPOINT: &str = "https://api.kimi.com/coding/v1";
pub const PROVIDER: ProviderSpec = ProviderSpec::new("kimi-for-coding", ProviderAuthMethod::ApiKey)
    .with_openai_compatible_endpoint(DEFAULT_ENDPOINT);
