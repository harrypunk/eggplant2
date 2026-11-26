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
    pub model_id: String,
    pub result: Result<Arc<dyn AgentProvider>, String>,
}

/// Persist a selected model and construct its provider through the OpenAI-compatible boundary.
pub async fn activate(
    store: Arc<dyn ProviderCredentialStore>,
    factory: Arc<dyn OpenAiProviderFactory>,
    model_id: String,
    updated_at: i64,
) -> OpenAiActivationResult {
    crate::capabilities::diagnostics::log(format_args!("model activation started: {model_id}"));
    let result = async {
        crate::capabilities::diagnostics::log(format_args!("model activation: reading credential"));
        let mut credential = store
            .get("openai")
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "OpenAI credential was not found".to_string())?;
        crate::capabilities::diagnostics::log(format_args!(
            "model activation: constructing provider"
        ));
        let provider = factory.provider(credential.clone(), model_id.clone())?;
        crate::capabilities::diagnostics::log(format_args!(
            "model activation: provider constructed"
        ));
        credential.selected_model = Some(model_id.clone());
        credential.updated_at = updated_at;
        crate::capabilities::diagnostics::log(format_args!(
            "model activation: persisting selected model"
        ));
        store
            .upsert(credential)
            .await
            .map_err(|error| error.to_string())?;
        crate::capabilities::diagnostics::log(format_args!(
            "model activation: selected model persisted"
        ));
        Ok(provider)
    }
    .await;
    match &result {
        Ok(_) => crate::capabilities::diagnostics::log(format_args!(
            "model activation succeeded: {model_id}"
        )),
        Err(error) => crate::capabilities::diagnostics::log(format_args!(
            "model activation failed: {model_id}: {error}"
        )),
    }
    OpenAiActivationResult { model_id, result }
}
