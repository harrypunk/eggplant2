use ratatui::{DefaultTerminal, backend::Backend as _};

use crate::{
    config::Config,
    data_object::{Message, Model, RunningState},
    manager::{
        effect::Effect, event_manager::EventManager, keyboard_input_registry::Registry,
        message_manager::MessageManager,
    },
    page::editor::Editor,
};

/// Run the application's message/update/view loop until the model requests exit.
pub fn run(
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
        Effect::OpenDocument(request) => {
            let sender = sender.clone();
            tokio::task::spawn_blocking(move || {
                let _ = sender.send(Message::FileOpened(request.perform()));
            });
        }
    }
}
