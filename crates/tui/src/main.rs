use std::collections::HashMap;

use crossterm::event::KeyCode;
use ratatui::DefaultTerminal;

use tui::{
    config::Config,
    data_object::{Context, Message, Model, RunningState},
    manager::{
        context::ContextBindings, event_manager::EventManager, keyboard_input_registry::Registry,
        message_manager::MessageManager,
    },
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
    // push app normal
    model.context_stack.push(Context::AppNormal);

    let event_manager = EventManager::new(config);
    let message_manager = MessageManager;
    let editor_page = Editor;

    while model.running_state != RunningState::Done {
        // Render the current view
        terminal.draw(|frame| editor_page.view(&model, frame))?;

        let keymap_test1 = example_keymap();
        // Handle events and map to a Message
        let mut current_msg = event_manager.next_message(&model, keymap_test1)?;

        // Process updates as long as they return a non-None message
        while let Some(msg) = current_msg {
            current_msg = message_manager.update(&mut model, msg);
        }
    }

    Ok(())
}

// todo only for testing
fn example_keymap() -> Registry {
    let normal_binding = ContextBindings::new_with_mapping(
        Context::AppNormal,
        HashMap::from([
            (KeyCode::Char('q'), Message::Quit),
            (KeyCode::Char('i'), Message::EnterInsertMode),
        ]),
    );

    let edit_binding = ContextBindings::new_with_mapping(
        Context::EditorInput,
        HashMap::from([(KeyCode::Esc, Message::ExitInsertMode)]),
    );

    Registry::default()
        .add_binding(normal_binding)
        .add_binding(edit_binding)
}
