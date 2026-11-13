use tui::{
    config::Config,
    manager::{command_registry::CommandRegistry, keyboard_input_registry::Registry},
    runtime,
    startup::{initial_model, startup_path_from},
    terminal,
    theme::Theme,
    ui::UiRegistry,
};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let initial_path = startup_path_from(std::env::args_os().skip(1))?;
    let mut model = initial_model(initial_path)?;
    let config = Config::load()?;
    model.theme = Theme::from_name(&config.theme)?;
    let commands = CommandRegistry::builtin();
    let ui = UiRegistry::builtin();
    let keymap = Registry::from_config(&config.keymap, &commands)?;
    terminal::install_panic_hook();
    let mut terminal = terminal::init_terminal()?;
    let result = runtime::run(&mut terminal, &config, keymap, commands, ui, model);
    terminal::restore_terminal()?;
    result
}
