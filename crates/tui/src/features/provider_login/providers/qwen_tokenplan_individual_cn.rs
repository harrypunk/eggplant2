use super::{ProviderAuthMethod, ProviderSpec};

pub const DEFAULT_ENDPOINT: &str =
    "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1";
pub const PROVIDER: ProviderSpec =
    ProviderSpec::new("qwen-tokenplan-individual-cn", ProviderAuthMethod::ApiKey)
        .with_openai_compatible_endpoint(DEFAULT_ENDPOINT);
