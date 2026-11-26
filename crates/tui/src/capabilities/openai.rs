use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::AgentProvider;
use eggplant_agent_storage::{ProviderCredential, ProviderCredentialStore};

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

pub struct OpenAiActivationResult {
    pub provider_id: String,
    pub model_id: String,
    pub result: Result<Arc<dyn AgentProvider>, String>,
}

/// Persist a selected model and construct its provider through the OpenAI-compatible boundary.
pub async fn activate(
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    provider_id: String,
    model_id: String,
    updated_at: i64,
) -> OpenAiActivationResult {
    let result = async {
        let mut credential = store
            .get(&provider_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("{provider_id} credential was not found"))?;
        let provider = factory.provider(credential.clone(), model_id.clone())?;
        credential.selected_model = Some(model_id.clone());
        credential.updated_at = updated_at;
        store
            .upsert(credential)
            .await
            .map_err(|error| error.to_string())?;
        Ok(provider)
    }
    .await;
    OpenAiActivationResult {
        provider_id,
        model_id,
        result,
    }
}
