use std::sync::Arc;

use eggplant_agent::AgentProvider;
use eggplant_agent_openai::{OpenAiConfig, OpenAiProvider};
use eggplant_agent_storage::{ProviderAuthType, ProviderCredential};

use crate::capabilities::openai::{OpenAiCatalogFuture, OpenAiProviderFactory};

#[derive(Debug)]
pub struct OpenAiFactory {
    context_window: Option<u64>,
}

impl OpenAiFactory {
    pub fn new(context_window: Option<u64>) -> Self {
        Self { context_window }
    }
}

impl OpenAiProviderFactory for OpenAiFactory {
    fn list_models(&self, credential: ProviderCredential) -> OpenAiCatalogFuture<'_> {
        Box::pin(async move {
            OpenAiProvider::new(openai_config(credential, None, self.context_window)?)
                .map_err(|error| error.0)?
                .list_models()
                .await
                .map_err(|error| error.0)
        })
    }

    fn provider(
        &self,
        credential: ProviderCredential,
        model: String,
    ) -> Result<Arc<dyn AgentProvider>, String> {
        let provider =
            OpenAiProvider::new(openai_config(credential, Some(model), self.context_window)?)
                .map_err(|error| error.0)?;
        Ok(Arc::new(provider))
    }
}

fn openai_config(
    credential: ProviderCredential,
    model: Option<String>,
    context_window: Option<u64>,
) -> Result<OpenAiConfig, String> {
    if credential.auth_type != ProviderAuthType::ApiKey {
        return Err("OpenAI-compatible login requires an API key".to_string());
    }
    let api_key = credential
        .api_key
        .ok_or_else(|| "OpenAI-compatible API key is missing".to_string())?;
    let base_url = credential
        .base_url
        .ok_or_else(|| "OpenAI-compatible endpoint is missing".to_string())?;
    let config = OpenAiConfig {
        base_url,
        model,
        context_window,
        ..OpenAiConfig::default()
    };
    Ok(config.with_api_key(api_key))
}
