use std::sync::Arc;

use tui::{
    capabilities::page_scroll::PageScroll,
    composition::agent::configure as configure_agent,
    config::{Config, load_language_servers},
    manager::{command_registry::CommandRegistry, text_input_registry::TextInputRegistry},
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
    configure_agent(&config, &mut model).await?;
    model.theme = config.theme.fallback_theme()?;
    model.page_scroll = PageScroll::from_step(&config.features.page_scroll.step)?;
    let commands = CommandRegistry::builtin();
    let text_input = TextInputRegistry::builtin();
    let ui = UiRegistry::builtin();
    let keymap = tui::composition::keymaps::from_config(&config.keymap, &commands)?;
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
