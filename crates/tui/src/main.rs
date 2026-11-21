use std::sync::Arc;

use color_eyre::eyre::bail;
use eggplant_agent::AgentProvider;
use eggplant_agent_openai::{OpenAiConfig, OpenAiProvider};
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
    if let Some(provider) = agent_provider(&config)? {
        model.agent.set_provider(provider);
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
