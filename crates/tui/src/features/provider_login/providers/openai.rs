use super::{ProviderAuthMethod, ProviderSpec};

pub const DEFAULT_OPENAI_ENDPOINT: &str = "https://api.openai.com/v1";
pub const PROVIDER: ProviderSpec = ProviderSpec::new("openai", ProviderAuthMethod::ApiKey)
    .with_openai_compatible_endpoint(DEFAULT_OPENAI_ENDPOINT);
