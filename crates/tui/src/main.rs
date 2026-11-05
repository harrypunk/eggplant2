use ratatui::{DefaultTerminal, backend::Backend as _};

use tui::{
    config::Config,
    data_object::{Message, Model, RunningState},
    manager::{
        event_manager::EventManager, keyboard_input_registry::Registry,
        message_manager::MessageManager,
    },
    page::editor::Editor,
    startup::{initial_model, startup_path_from},
    terminal,
};

fn main() -> color_eyre::Result<()> {
    let initial_path = startup_path_from(std::env::args_os().skip(1))?;
    let model = initial_model(initial_path)?;
    let config = Config::load()?;
    let keymap = Registry::from_config(&config.keymap)?;
    terminal::install_panic_hook();
    let mut terminal = terminal::init_terminal()?;
    let result = run(&mut terminal, &config, keymap, model);
    terminal::restore_terminal()?;
    result
}

fn run(
    terminal: &mut DefaultTerminal,
    config: &Config,
    keymap: Registry,
    mut model: Model,
) -> color_eyre::Result<()> {
    let event_manager = EventManager::new(config, keymap);
    let message_manager = MessageManager;
    let editor_page = Editor;

    let initial_size = terminal.backend().size()?;
    message_manager.update(
        &mut model,
        Message::Resize(initial_size.width, initial_size.height),
    );

    while model.running_state != RunningState::Done {
        // Render the current view
        terminal.draw(|frame| editor_page.view(&model, frame))?;

        // Handle events and map to a Message
        let mut current_msg = event_manager.next_message(&model)?;

        // Process updates as long as they return a non-None message
        while let Some(msg) = current_msg {
            current_msg = message_manager.update(&mut model, msg);
        }
    }

    Ok(())
}
