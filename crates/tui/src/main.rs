use ratatui::DefaultTerminal;

use tui::{
    config::Config,
    data_object::{Model, RunningState},
    manager::{event_manager::EventManager, message_manager::MessageManager},
    page::editor::Editor,
    terminal,
};

fn main() -> color_eyre::Result<()> {
    let config = Config::load()?;
    terminal::install_panic_hook();
    let mut terminal = terminal::init_terminal()?;
    let result = run(&mut terminal, &config);
    terminal::restore_terminal()?;
    result
}

fn run(terminal: &mut DefaultTerminal, config: &Config) -> color_eyre::Result<()> {
    let mut model = Model::default();
    let event_manager = EventManager::new(config);
    let message_manager = MessageManager;
    let editor_page = Editor;

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
