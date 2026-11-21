use std::sync::Arc;

use color_eyre::eyre::bail;
use eggplant_agent::AgentProvider;
use eggplant_agent_openai::{OpenAiConfig, OpenAiProvider};
use eggplant_agent_storage::SessionStore;
#[cfg(feature = "agent-sqlite")]
use eggplant_agent_storage_seaorm::SeaOrmSessionStore;
use tui::{
    capabilities::page_scroll::PageScroll,
    config::{Config, load_language_servers},
    manager::{
        command_registry::CommandRegistry, keyboard_input_registry::Registry,
        text_input_registry::TextInputRegistry,
    },
    runtime,
    startup::{initial_model_with_languages, startup_path_from},
    terminal,
    ui::UiRegistry,
};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let initial_path = startup_path_from(std::env::args_os().skip(1))?;
    let config = Config::load()?;
    let languages = Arc::new(load_language_servers()?);
    let mut model = initial_model_with_languages(initial_path, languages)?;
    configure_agent_store(&config, &mut model).await?;
    if let Some(provider) = agent_provider(&config)? {
        model.agent.set_provider(provider);
        model.agent.set_provider_metadata(
            "openai".to_string(),
            Some(
                config
                    .features
                    .agent
                    .model
                    .clone()
                    .unwrap_or_else(|| "gpt-4o-mini".to_string()),
            ),
        );
    }
    model.theme = config.theme.fallback_theme()?;
    model.page_scroll = PageScroll::from_step(&config.features.page_scroll.step)?;
    let commands = CommandRegistry::builtin();
    let text_input = TextInputRegistry::builtin();
    let ui = UiRegistry::builtin();
    let keymap = Registry::from_config(&config.keymap, &commands)?;
    terminal::install_panic_hook();
    let mut terminal = terminal::init_terminal()?;
    let result = runtime::run(
        &mut terminal,
        &config,
        keymap,
        text_input,
        commands,
        ui,
        model,
    );
    terminal::restore_terminal()?;
    result
}

async fn configure_agent_store(
    config: &Config,
    model: &mut tui::data_object::Model,
) -> color_eyre::Result<()> {
    if let Some(path) = &config.features.agent.session_db {
        if !path.is_absolute() {
            bail!("agent session_db must be an absolute per-user path");
        }
        let store = connect_agent_store(path).await?;
        model.agent.set_session_store(store);
    }
    Ok(())
}

#[cfg(feature = "agent-sqlite")]
async fn connect_agent_store(path: &std::path::Path) -> color_eyre::Result<Arc<dyn SessionStore>> {
    let store = SeaOrmSessionStore::connect(path)
        .await
        .map_err(|error| color_eyre::eyre::eyre!(error.to_string()))?;
    Ok(Arc::new(store))
}

#[cfg(not(feature = "agent-sqlite"))]
async fn connect_agent_store(_path: &std::path::Path) -> color_eyre::Result<Arc<dyn SessionStore>> {
    bail!("agent session_db requires the `agent-sqlite` build feature")
}

fn agent_provider(config: &Config) -> color_eyre::Result<Option<Arc<dyn AgentProvider>>> {
    match config.features.agent.provider.as_deref() {
        None | Some("fake") => Ok(None),
        Some("openai") => {
            let mut provider_config = OpenAiConfig::default();
            if let Some(model) = &config.features.agent.model {
                provider_config.model = model.clone();
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
        let mut model = tui::data_object::Model::default();
        assert!(
            configure_agent_store(&Config::default(), &mut model)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn database_path_must_be_absolute() {
        let mut config = Config::default();
        config.features.agent.session_db = Some(std::path::PathBuf::from("relative.db"));
        let mut model = tui::data_object::Model::default();
        let error = configure_agent_store(&config, &mut model)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("absolute"));
    }

    #[cfg(feature = "agent-sqlite")]
    #[tokio::test]
    async fn sqlite_path_creates_and_migrates_the_opt_in_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested/agent.db");
        let mut config = Config::default();
        config.features.agent.session_db = Some(path.clone());
        let mut model = tui::data_object::Model::default();

        configure_agent_store(&config, &mut model).await.unwrap();

        assert!(path.is_file());
        let store = SeaOrmSessionStore::connect(&path).await.unwrap();
        let recent = store
            .recent_sessions(eggplant_agent_storage::RecentQuery::default())
            .await
            .unwrap();
        assert!(recent.sessions.is_empty());
    }

    #[cfg(not(feature = "agent-sqlite"))]
    #[tokio::test]
    async fn database_path_requires_the_opt_in_feature() {
        let mut config = Config::default();
        config.features.agent.session_db = Some(std::path::PathBuf::from("/tmp/agent.db"));
        let mut model = tui::data_object::Model::default();
        let error = configure_agent_store(&config, &mut model)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("agent-sqlite"));
    }
}
