mod component;
mod state;

pub use state::BufferJumpState;

use crate::{
    command_menu::{CommandMenuContext, CommandMenuItem},
    data_object::{CommandId, Context, Model, StatusMessage},
    focus,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
    ui::UiRegistry,
};

pub static CODE_CONTEXT: CommandMenuContext = CommandMenuContext::new(
    "code",
    &[CommandMenuItem::action(
        'j',
        "jump in visible buffer",
        CommandId("buffer_jump_start"),
    )],
);

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("buffer_jump_start"),
            "jump in visible buffer",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_jump_cancel"),
            "cancel buffer jump",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_jump_backspace"),
            "edit buffer jump input",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::BufferJump, handle_input)
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("buffer_jump", component::overlay)?;
    ui.register_editor_overlay("buffer_jump", component::editor_overlay)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "buffer_jump_start" => start(model),
        "buffer_jump_cancel" => close(model),
        "buffer_jump_backspace" => model.buffer_jump.backspace(),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown buffer-jump command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

/// Handle pattern or label text without adding feature-specific messages to the shell.
pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if model.buffer_jump.buffer() != model.editor.active_buffer() {
        close(model);
        return None;
    }

    if !model.buffer_jump.is_label_phase() {
        if !model.buffer_jump.push_pattern(character) {
            return None;
        }
        let width = model.editor.active_view_width().unwrap_or(0);
        if let Some(snapshot) = model.editor.active_snapshot() {
            model.buffer_jump.assign_targets(&snapshot.text, width);
        }
        if model.buffer_jump.targets().is_empty() {
            let pattern = model.buffer_jump.pattern().to_string();
            close(model);
            model.status = Some(StatusMessage::info(format!(
                "No visible matches for `{pattern}`"
            )));
        }
        return None;
    }

    model.buffer_jump.push_label(character);
    let target = model.buffer_jump.complete_label()?;
    match target {
        Some(position) => {
            close(model);
            match model.editor.move_cursor_to(position) {
                Ok(()) => focus::focus_editor(model),
                Err(error) => model.status = Some(StatusMessage::error(error.to_string())),
            }
        }
        None => {
            model.buffer_jump.clear_label();
            model.status = Some(StatusMessage::info("Unknown jump label"));
        }
    }
    None
}

fn start(model: &mut Model) {
    if !matches!(
        model.context_stack.last(),
        Some(Context::AppNormal | Context::Explorer)
    ) {
        return;
    }
    let Some(buffer) = model.editor.active_buffer() else {
        model.status = Some(StatusMessage::info("No active buffer"));
        return;
    };
    model.status = None;
    model.buffer_jump.start(buffer);
    model.context_stack.push(Context::BufferJump);
}

fn close(model: &mut Model) {
    model.buffer_jump.close();
    model
        .context_stack
        .pop_if(|context| context == &Context::BufferJump);
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, page::editor::Editor,
        test_utils::create_buffer,
    };

    fn start_jump(model: &mut Model, manager: &MessageManager) {
        for key in [' ', 'c', 'j'] {
            manager.update(model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        assert!(model.buffer_jump.is_active());
        assert_eq!(model.context_stack.last(), Some(&Context::BufferJump));
    }

    #[test]
    fn leader_pattern_and_label_jump_to_the_first_visible_match() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        start_jump(&mut model, &manager);
        for character in ['e', 'l'] {
            manager.update(&mut model, Message::TextInput(character));
        }
        assert_eq!(model.buffer_jump.targets().len(), 3);
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            0
        );

        for character in ['z', 'x'] {
            manager.update(&mut model, Message::TextInput(character));
        }
        assert!(!model.buffer_jump.is_active());
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            1
        );
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn cancelling_does_not_move_the_cursor() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        start_jump(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('e'));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_jump_cancel")),
        );

        assert!(!model.buffer_jump.is_active());
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            0
        );
    }

    #[test]
    fn no_visible_matches_closes_without_changing_the_document() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        start_jump(&mut model, &manager);

        for character in ['z', 'z'] {
            manager.update(&mut model, Message::TextInput(character));
        }

        assert!(!model.buffer_jump.is_active());
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            0
        );
        assert_eq!(
            model.status,
            Some(StatusMessage::info("No visible matches for `zz`"))
        );
    }

    #[test]
    fn renders_dimmed_text_with_bright_labels_and_hides_the_cursor() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 6));
        start_jump(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('e'));
        manager.update(&mut model, Message::TextInput('l'));

        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Editor::default().view(&model, frame))
            .unwrap();

        let content = terminal.backend().buffer().content();
        let text = content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(text.contains("zx"));
        assert!(text.contains("Jump: label"));
        assert!(!terminal.backend().cursor_visible());
        assert!(
            content
                .iter()
                .any(|cell| cell.symbol() == "h" && cell.modifier.contains(Modifier::DIM))
        );
        assert!(
            content
                .iter()
                .any(|cell| cell.symbol() == "z" && !cell.modifier.contains(Modifier::DIM))
        );

        manager.update(&mut model, Message::Resize(4, 2));
        let backend = TestBackend::new(4, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Editor::default().view(&model, frame))
            .unwrap();
        assert!(!terminal.backend().cursor_visible());
    }
}
