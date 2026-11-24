use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::AgentProvider;
use eggplant_agent_storage::ProviderCredential;

pub type OpenAiCatalogFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + Send + 'a>>;

/// OpenAI-compatible construction boundary used by login and model-selection workflows.
pub trait OpenAiProviderFactory: std::fmt::Debug + Send + Sync {
    fn list_models(&self, credential: ProviderCredential) -> OpenAiCatalogFuture<'_>;

    fn provider(
        &self,
        credential: ProviderCredential,
        model: String,
    ) -> Result<Arc<dyn AgentProvider>, String>;
}
