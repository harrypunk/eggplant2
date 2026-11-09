use ratatui::{DefaultTerminal, backend::Backend as _};

use tui::{
    config::Config,
    data_object::{Message, Model, RunningState},
    manager::{
        effect::Effect, event_manager::EventManager, keyboard_input_registry::Registry,
        message_manager::MessageManager,
    },
    page::editor::Editor,
    startup::{initial_model, startup_path_from},
    terminal,
    theme::Theme,
};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    let initial_path = startup_path_from(std::env::args_os().skip(1))?;
    let mut model = initial_model(initial_path)?;
    let config = Config::load()?;
    model.theme = Theme::from_name(&config.theme)?;
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
    let message_manager = MessageManager::default();
    let editor_page = Editor;
    let (message_sender, mut message_receiver) = tokio::sync::mpsc::unbounded_channel();

    let initial_size = terminal.backend().size()?;
    message_manager.update(
        &mut model,
        Message::Resize(initial_size.width, initial_size.height),
    );

    while model.running_state != RunningState::Done {
        while let Ok(message) = message_receiver.try_recv() {
            apply_message(&mut model, &message_manager, message, &message_sender);
        }

        // Render the current view
        terminal.draw(|frame| editor_page.view(&model, frame))?;

        if let Some(message) = event_manager.next_message(&model)? {
            apply_message(&mut model, &message_manager, message, &message_sender);
        }
    }

    Ok(())
}

fn apply_message(
    model: &mut Model,
    manager: &MessageManager,
    message: Message,
    sender: &tokio::sync::mpsc::UnboundedSender<Message>,
) {
    let Some(effect) = manager.update(model, message) else {
        return;
    };
    match effect {
        Effect::Save(request) => {
            let sender = sender.clone();
            tokio::spawn(async move {
                let completion = request.perform().await;
                let _ = sender.send(Message::SaveFinished(completion));
            });
        }
        Effect::FilePicker(request) => {
            let sender = sender.clone();
            tokio::task::spawn_blocking(move || {
                let _ = sender.send(request.perform());
            });
        }
    }
}
