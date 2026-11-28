use std::sync::Arc;

use color_eyre::eyre::{bail, eyre};
use eggplant_agent::{AgentProvider, CancellationToken};
use eggplant_agent_openai::{OpenAiConfig, OpenAiProvider};
use eggplant_agent_storage::{
    ModelCatalogStore, ProviderCredentialStore, RecentQuery, SessionStore,
};
use eggplant_agent_storage_seaorm::{
    SeaOrmModelCatalogStore, SeaOrmProviderCredentialStore, SeaOrmSessionStore,
};

use super::openai::OpenAiFactory;
use crate::{
    agent::{LoadedSession, load_session},
    config::{Config, default_agent_database_path},
    data_object::Model,
};

/// Configure agent persistence and the statically selected provider before terminal startup.
pub async fn configure(config: &Config, model: &mut Model) -> color_eyre::Result<()> {
    model.openai_factory = Some(Arc::new(OpenAiFactory::new(
        config.features.agent.context_window,
    )));
    configure_store(config, model).await?;
    restore_latest_workspace_session(model).await;
    if let Some(provider) = agent_provider(config)? {
        let model_id = config
            .features
            .agent
            .model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        install_provider(model, provider, "openai".to_string(), model_id);
        return Ok(());
    }
    if config.features.agent.provider.is_some() {
        return Ok(());
    }
    restore_saved_provider(model).await
}

async fn restore_latest_workspace_session(model: &mut Model) {
    let Some(store) = model.agent.runtime().session_store() else {
        return;
    };
    let Some(workspace) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_string_lossy().into_owned())
    else {
        return;
    };
    match latest_workspace_session(store.as_ref(), workspace).await {
        Ok(Some(LoadedSession {
            snapshot,
            created_at,
            title,
        })) => {
            if model
                .agent
                .runtime_mut()
                .restore_session(snapshot, created_at, title)
                .is_err()
            {
                report_restore_error(
                    model,
                    "the stored session has an active run and cannot be restored".to_string(),
                );
            }
        }
        Ok(None) => {}
        Err(error) => report_restore_error(model, error),
    }
}

async fn latest_workspace_session(
    store: &dyn SessionStore,
    workspace: String,
) -> Result<Option<LoadedSession>, String> {
    let page = store
        .recent_sessions(RecentQuery {
            workspace: Some(workspace),
            limit: 1,
            ..RecentQuery::default()
        })
        .await
        .map_err(|error| error.to_string())?;
    let Some(session) = page.sessions.into_iter().next() else {
        return Ok(None);
    };
    load_session(store, session.id, CancellationToken::default())
        .await
        .map(Some)
        .map_err(|error| error.to_string())
}

fn report_restore_error(model: &mut Model, error: String) {
    let error = format!("Session restore failed: {error}")
        .chars()
        .take(512)
        .collect();
    model.agent.runtime_mut().report_storage_error(error);
}

async fn restore_saved_provider(model: &mut Model) -> color_eyre::Result<()> {
    let Some(store) = model.provider_credentials.clone() else {
        return Ok(());
    };
    let providers = store
        .list()
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    let mut selected = Vec::new();
    for provider in providers {
        if let Some(credential) = store
            .get(&provider.provider_id)
            .await
            .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?
            && credential.selected_model.is_some()
        {
            selected.push(credential);
        }
    }
    let Some(credential) = selected.into_iter().max_by(|left, right| {
        left.updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    }) else {
        return Ok(());
    };
    let Some(model_id) = credential.selected_model.clone() else {
        return Ok(());
    };
    let Some(factory) = model.openai_factory.clone() else {
        return Ok(());
    };
    let provider_id = credential.provider_id.clone();
    let provider = factory
        .provider(credential, model_id.clone())
        .map_err(|error| color_eyre::eyre::eyre!(error))?;
    install_provider(model, provider, provider_id, model_id);
    Ok(())
}

fn install_provider(
    model: &mut Model,
    provider: Arc<dyn AgentProvider>,
    provider_id: String,
    model_id: String,
) {
    model.agent.set_provider(provider);
    model
        .agent
        .set_provider_metadata(provider_id, Some(model_id));
}

async fn configure_store(config: &Config, model: &mut Model) -> color_eyre::Result<()> {
    configure_store_with_home(config, model, std::env::home_dir()).await
}

async fn configure_store_with_home(
    config: &Config,
    model: &mut Model,
    home: Option<std::path::PathBuf>,
) -> color_eyre::Result<()> {
    let path = storage_path(config, home)?;
    if !path.is_absolute() {
        bail!("agent session_db must be an absolute per-user path");
    }
    let store = connect_store(&path).await?;
    model.agent.set_session_store(store);
    model.provider_credentials = Some(connect_credential_store(&path).await?);
    model.model_catalogs = Some(connect_model_catalog_store(&path).await?);
    Ok(())
}

fn storage_path(
    config: &Config,
    home: Option<std::path::PathBuf>,
) -> color_eyre::Result<std::path::PathBuf> {
    if let Some(path) = &config.features.agent.session_db {
        return Ok(path.clone());
    }
    home.map(default_agent_database_path)
        .ok_or_else(|| eyre!("default agent SQLite path requires a home directory"))
}

async fn connect_store(path: &std::path::Path) -> color_eyre::Result<Arc<dyn SessionStore>> {
    let store = SeaOrmSessionStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

async fn connect_credential_store(
    path: &std::path::Path,
) -> color_eyre::Result<Arc<dyn ProviderCredentialStore>> {
    let store = SeaOrmProviderCredentialStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

async fn connect_model_catalog_store(
    path: &std::path::Path,
) -> color_eyre::Result<Arc<dyn ModelCatalogStore>> {
    let store = SeaOrmModelCatalogStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

fn agent_provider(config: &Config) -> color_eyre::Result<Option<Arc<dyn AgentProvider>>> {
    match config.features.agent.provider.as_deref() {
        None | Some("fake") => Ok(None),
        Some("openai") => {
            let mut provider_config = OpenAiConfig {
                context_window: config.features.agent.context_window,
                ..OpenAiConfig::default()
            };
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
    use eggplant_agent::{MessageRole, RunId, RunStatus, SessionId};
    use eggplant_agent_storage::{FinalizedRun, MessageRecord, RunRecord, SessionRecord};

    use super::*;

    async fn persist_session(
        store: &SeaOrmSessionStore,
        id: &str,
        run_id: u64,
        updated_at: i64,
        workspace: &str,
    ) {
        let session_id = SessionId::from(id);
        let run = RunId(run_id);
        store
            .finalize_run(
                FinalizedRun {
                    session: SessionRecord {
                        id: session_id.clone(),
                        created_at: 1,
                        updated_at,
                        workspace: Some(workspace.to_string()),
                        title: Some(format!("title-{id}")),
                    },
                    run: RunRecord {
                        id: run,
                        session_id: session_id.clone(),
                        status: RunStatus::Complete,
                        created_at: 1,
                        completed_at: Some(updated_at),
                        provider: "test".to_string(),
                        model: Some("fake".to_string()),
                        editor_context: None,
                    },
                    messages: [
                        (MessageRole::User, "question"),
                        (MessageRole::Assistant, "answer"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(sequence, (role, text))| MessageRecord {
                        session_id: session_id.clone(),
                        run_id: run,
                        sequence: sequence as u64,
                        role,
                        text: format!("{text}-{id}"),
                        created_at: updated_at,
                    })
                    .collect(),
                    tool_calls: Vec::new(),
                },
                CancellationToken::default(),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn default_sqlite_path_uses_the_injected_ai_data_directory() {
        let home = tempfile::tempdir().unwrap();
        let mut model = Model::default();

        configure_store_with_home(
            &Config::default(),
            &mut model,
            Some(home.path().to_path_buf()),
        )
        .await
        .unwrap();

        assert!(home.path().join(".eggplant/ai/data.sqlite").is_file());
        assert!(model.provider_credentials.is_some());
    }

    #[test]
    fn default_sqlite_path_requires_a_home_directory() {
        let error = storage_path(&Config::default(), None).unwrap_err();
        assert!(error.to_string().contains("home directory"));
    }

    #[tokio::test]
    async fn database_path_must_be_absolute() {
        let mut config = Config::default();
        config.features.agent.session_db = Some(std::path::PathBuf::from("relative.db"));
        let mut model = Model::default();
        let error = configure_store(&config, &mut model).await.unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[tokio::test]
    async fn sqlite_path_creates_and_migrates_the_configured_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/agent.db");
        let mut config = Config::default();
        config.features.agent.session_db = Some(path.clone());
        let mut model = Model::default();

        configure_store(&config, &mut model).await.unwrap();

        assert!(path.is_file());
        let credentials = model.provider_credentials.clone().unwrap();
        assert!(credentials.list().await.unwrap().is_empty());
        let catalogs = model.model_catalogs.clone().unwrap();
        assert!(catalogs.get("openai").await.unwrap().is_none());
        let store = SeaOrmSessionStore::connect(&path).await.unwrap();
        let recent = store
            .recent_sessions(eggplant_agent_storage::RecentQuery::default())
            .await
            .unwrap();
        assert!(recent.sessions.is_empty());
    }

    #[tokio::test]
    async fn startup_restores_latest_session_for_current_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.db");
        let store = SeaOrmSessionStore::connect(&path).await.unwrap();
        let workspace = directory.path().to_string_lossy().into_owned();
        persist_session(&store, "older", 1, 10, &workspace).await;
        persist_session(&store, "newer", 2, 20, &workspace).await;
        persist_session(&store, "other-workspace", 4, 30, "/other/workspace").await;
        drop(store);
        let mut config = Config::default();
        config.features.agent.session_db = Some(path);
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();

        configure(&config, &mut model).await.unwrap();

        assert_eq!(model.agent.session().id(), &SessionId::from("newer"));
        assert_eq!(model.agent.runtime().session_title(), Some("title-newer"));
        assert_eq!(model.agent.session().messages().len(), 2);
        assert_eq!(model.agent.runtime().provider_history().len(), 2);
        let next = model
            .agent
            .runtime_mut()
            .begin("next".to_string(), None, 40)
            .unwrap();
        assert_eq!(next.id, RunId(3));
        model.agent.session_mut().complete(next.id);
        let fresh = model.agent.runtime_mut().new_session().unwrap();
        assert_ne!(fresh.session_id, SessionId::from("newer"));
        assert!(model.agent.session().messages().is_empty());
    }

    #[tokio::test]
    async fn restore_read_failure_starts_empty_with_a_storage_warning() {
        #[derive(Debug)]
        struct FailingStore;

        impl SessionStore for FailingStore {
            fn finalize_run<'a>(
                &'a self,
                _run: FinalizedRun,
                _cancellation: CancellationToken,
            ) -> eggplant_agent_storage::StoreFuture<'a, ()> {
                Box::pin(async { Ok(()) })
            }

            fn recent_sessions<'a>(
                &'a self,
                _query: RecentQuery,
            ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::RecentPage>
            {
                Box::pin(async {
                    Err(eggplant_agent_storage::StorageError::Backend(
                        "read failed".to_string(),
                    ))
                })
            }

            fn transcript<'a>(
                &'a self,
                _session_id: SessionId,
                _query: eggplant_agent_storage::TranscriptQuery,
            ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::TranscriptPage>
            {
                unimplemented!()
            }

            fn search<'a>(
                &'a self,
                _query: eggplant_agent_storage::SearchQuery,
            ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::SearchPage>
            {
                unimplemented!()
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_session_store(Arc::new(FailingStore));

        restore_latest_workspace_session(&mut model).await;

        assert!(model.agent.session().messages().is_empty());
        assert!(
            model
                .agent
                .storage_error()
                .is_some_and(|error| error.contains("read failed"))
        );
    }

    #[tokio::test]
    async fn most_recently_activated_compatible_provider_is_restored() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent.db");
        let store = SeaOrmProviderCredentialStore::connect(&path).await.unwrap();
        for (provider_id, base_url, model, updated_at) in [
            ("openai", "https://api.openai.com/v1", "gpt-test", 10),
            (
                "kimi-for-coding",
                crate::provider_login::DEFAULT_KIMI_FOR_CODING_ENDPOINT,
                "k3-256k",
                20,
            ),
        ] {
            store
                .upsert(eggplant_agent_storage::ProviderCredential {
                    provider_id: provider_id.to_string(),
                    auth_type: eggplant_agent_storage::ProviderAuthType::ApiKey,
                    base_url: Some(base_url.to_string()),
                    selected_model: Some(model.to_string()),
                    api_key: Some("stored-secret".to_string()),
                    access_token: None,
                    refresh_token: None,
                    expires_at: None,
                    account_id: None,
                    created_at: 1,
                    updated_at,
                })
                .await
                .unwrap();
        }
        drop(store);
        let mut config = Config::default();
        config.features.agent.session_db = Some(path);
        let mut model = Model::default();

        configure(&config, &mut model).await.unwrap();

        assert_eq!(model.agent.provider_name(), "kimi-for-coding");
        assert_eq!(model.agent.model(), Some("k3-256k"));
        assert_eq!(
            model
                .provider_credentials
                .unwrap()
                .list()
                .await
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn explicit_openai_configuration_supplies_context_capacity() {
        let mut config = Config::default();
        config.features.agent.provider = Some("openai".to_string());
        config.features.agent.context_window = Some(64_000);

        let provider = agent_provider(&config).unwrap().unwrap();

        assert_eq!(provider.context_window(), Some(64_000));
    }

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
        config.features.agent.context_window = Some(128_000);
        let mut model = Model::default();

        configure(&config, &mut model).await.unwrap();

        assert!(model.agent.provider().is_some());
        assert_eq!(
            model.agent.provider().unwrap().context_window(),
            Some(128_000)
        );
        assert_eq!(model.agent.provider_name(), "openai");
        assert_eq!(model.agent.model(), Some("gpt-test"));

        let mut fake_config = Config::default();
        fake_config.features.agent.provider = Some("fake".to_string());
        fake_config.features.agent.session_db = config.features.agent.session_db.clone();
        let mut fake_model = Model::default();
        configure(&fake_config, &mut fake_model).await.unwrap();
        assert!(fake_model.agent.provider().is_none());
    }
}
