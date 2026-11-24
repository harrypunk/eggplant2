use std::sync::Arc;

use color_eyre::eyre::bail;
use eggplant_agent::AgentProvider;
use eggplant_agent_openai::{OpenAiConfig, OpenAiProvider};
use eggplant_agent_storage::{ProviderCredentialStore, SessionStore};
#[cfg(feature = "agent-sqlite")]
use eggplant_agent_storage_seaorm::{SeaOrmProviderCredentialStore, SeaOrmSessionStore};

use super::openai::OpenAiFactory;
use crate::{config::Config, data_object::Model};

/// Configure agent persistence and the statically selected provider before terminal startup.
pub async fn configure(config: &Config, model: &mut Model) -> color_eyre::Result<()> {
    model.openai_factory = Some(Arc::new(OpenAiFactory));
    configure_store(config, model).await?;
    if let Some(provider) = agent_provider(config)? {
        let model_id = config
            .features
            .agent
            .model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        install_provider(model, provider, model_id);
        return Ok(());
    }
    if config.features.agent.provider.is_some() {
        return Ok(());
    }
    restore_openai(model).await
}

async fn restore_openai(model: &mut Model) -> color_eyre::Result<()> {
    let Some(store) = model.provider_credentials.clone() else {
        return Ok(());
    };
    let Some(credential) = store
        .get("openai")
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?
    else {
        return Ok(());
    };
    let Some(model_id) = credential.selected_model.clone() else {
        return Ok(());
    };
    let Some(factory) = model.openai_factory.clone() else {
        return Ok(());
    };
    let provider = factory
        .provider(credential, model_id.clone())
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    install_provider(model, provider, model_id);
    Ok(())
}

fn install_provider(model: &mut Model, provider: Arc<dyn AgentProvider>, model_id: String) {
    model.agent.set_provider(provider);
    model
        .agent
        .set_provider_metadata("openai".to_string(), Some(model_id));
}

async fn configure_store(config: &Config, model: &mut Model) -> color_eyre::Result<()> {
    if let Some(path) = &config.features.agent.session_db {
        if !path.is_absolute() {
            bail!("agent session_db must be an absolute per-user path");
        }
        let store = connect_store(path).await?;
        model.agent.set_session_store(store);
        model.provider_credentials = Some(connect_credential_store(path).await?);
    }
    Ok(())
}

#[cfg(feature = "agent-sqlite")]
async fn connect_store(path: &std::path::Path) -> color_eyre::Result<Arc<dyn SessionStore>> {
    let store = SeaOrmSessionStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "agent-sqlite"))]
async fn connect_store(_path: &std::path::Path) -> color_eyre::Result<Arc<dyn SessionStore>> {
    bail!("agent session_db requires the `agent-sqlite` build feature")
}

#[cfg(feature = "agent-sqlite")]
async fn connect_credential_store(
    path: &std::path::Path,
) -> color_eyre::Result<Arc<dyn ProviderCredentialStore>> {
    let store = SeaOrmProviderCredentialStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "agent-sqlite"))]
async fn connect_credential_store(
    _path: &std::path::Path,
) -> color_eyre::Result<Arc<dyn ProviderCredentialStore>> {
    bail!("agent session_db requires the `agent-sqlite` build feature")
}

fn agent_provider(config: &Config) -> color_eyre::Result<Option<Arc<dyn AgentProvider>>> {
    match config.features.agent.provider.as_deref() {
        None | Some("fake") => Ok(None),
        Some("openai") => {
            let mut provider_config = OpenAiConfig::default();
            if let Some(model) = &config.features.agent.model {
                provider_config.model = Some(model.clone());
            }
            if let Some(base_url) = &config.features.agent.base_url {
                provider_config.base_url = base_url.clone();
            }
            if let Some(api_key_env) = &config.features.agent.api_key_env {
                provider_config.api_key_env = api_key_env.clone();
            }
            let provider = OpenAiProvider::new(provider_config)
                .map_err(|error| color_eyre::eyre::eyre!(error.0))?;
            Ok(Some(Arc::new(provider)))
        }
        Some(provider) => bail!("unknown agent provider `{provider}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_database_path_keeps_the_default_store() {
        let mut model = Model::default();
        assert!(
            configure_store(&Config::default(), &mut model)
                .await
                .is_ok()
        );
        assert!(model.provider_credentials.is_none());
    }

    #[tokio::test]
    async fn database_path_must_be_absolute() {
        let mut config = Config::default();
        config.features.agent.session_db = Some(std::path::PathBuf::from("relative.db"));
        let mut model = Model::default();
        let error = configure_store(&config, &mut model).await.unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[cfg(feature = "agent-sqlite")]
    #[tokio::test]
    async fn sqlite_path_creates_and_migrates_the_opt_in_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/agent.db");
        let mut config = Config::default();
        config.features.agent.session_db = Some(path.clone());
        let mut model = Model::default();

        configure_store(&config, &mut model).await.unwrap();

        assert!(path.is_file());
        let credentials = model.provider_credentials.clone().unwrap();
        assert!(credentials.list().await.unwrap().is_empty());
        let store = SeaOrmSessionStore::connect(&path).await.unwrap();
        let recent = store
            .recent_sessions(eggplant_agent_storage::RecentQuery::default())
            .await
            .unwrap();
        assert!(recent.sessions.is_empty());
    }

    #[cfg(feature = "agent-sqlite")]
    #[tokio::test]
    async fn saved_openai_model_is_restored_without_overriding_explicit_configuration() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.db");
        let store = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        store
            .upsert(eggplant_agent_storage::ProviderCredential {
                provider_id: "openai".to_string(),
                auth_type: eggplant_agent_storage::ProviderAuthType::ApiKey,
                base_url: Some("https://api.openai.com/v1".to_string()),
                selected_model: Some("gpt-test".to_string()),
                api_key: Some("stored-secret".to_string()),
                access_token: None,
                refresh_token: None,
                expires_at: None,
                account_id: None,
                created_at: 1,
                updated_at: 1,
            })
            .await
            .unwrap();
        drop(store);
        let mut config = Config::default();
        config.features.agent.session_db = Some(path);
        let mut model = Model::default();

        configure(&config, &mut model).await.unwrap();

        assert!(model.agent.provider().is_some());
        assert_eq!(model.agent.provider_name(), "openai");
        assert_eq!(model.agent.model(), Some("gpt-test"));

        let mut fake_config = Config::default();
        fake_config.features.agent.provider = Some("fake".to_string());
        fake_config.features.agent.session_db = config.features.agent.session_db.clone();
        let mut fake_model = Model::default();
        configure(&fake_config, &mut fake_model).await.unwrap();
        assert!(fake_model.agent.provider().is_none());
    }

    #[cfg(not(feature = "agent-sqlite"))]
    #[tokio::test]
    async fn database_path_requires_the_opt_in_feature() {
        let mut config = Config::default();
        config.features.agent.session_db = Some(std::path::PathBuf::from("/tmp/agent.db"));
        let mut model = Model::default();
        let error = configure_store(&config, &mut model).await.unwrap_err();
        assert!(error.to_string().contains("agent-sqlite"));
    }
}
