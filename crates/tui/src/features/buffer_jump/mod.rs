mod component;
mod matching;
mod state;

pub use state::BufferJumpState;

use crate::{
    command_menu::CommandMenuItem,
    data_object::{CommandId, Context, Model, StatusMessage},
    focus,
    manager::{command_registry::Command, effect::Effect, keymap_contribution::ContextKeymap},
    ui::UiRegistry,
};

pub const KEYMAPS: &[ContextKeymap] = &[ContextKeymap::new("buffer_jump", Context::BufferJump)
    .with_commands(&[("buffer_jump_cancel", "esc")])];

pub const SEARCH_MENU_ITEM: CommandMenuItem = CommandMenuItem::action(
    'j',
    "jump in visible buffer",
    CommandId("buffer_jump_start"),
);

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("buffer_jump_start"),
            "jump in visible buffer",
            start_command,
        ),
        Command::feature(
            CommandId("buffer_jump_cancel"),
            "cancel buffer jump",
            cancel,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("buffer_jump", component::overlay)?;
    ui.register_editor_overlay("buffer_jump", component::editor_overlay)
}

fn start_command(model: &mut Model) -> Option<Effect> {
    start(model);
    None
}

fn cancel(model: &mut Model) -> Option<Effect> {
    close(model);
    None
}

/// Handle pattern or label text without adding feature-specific messages to the shell.
pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if model.buffer_jump.source_identity() != model.editor.active_revision() {
        close(model);
        return None;
    }

    if !model.buffer_jump.is_label_phase() {
        model.buffer_jump.push_pattern(character);
        refresh_matches(model);
        if model.buffer_jump.is_label_phase() && model.buffer_jump.targets().is_empty() {
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
            match model
                .editor
                .active_editor()
                .and_then(|mut editor| editor.move_cursor_to(position))
            {
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

fn refresh_matches(model: &mut Model) {
    let width = model.editor.active_view_width().unwrap_or(0);
    if let Some(snapshot) = model.editor.active_snapshot() {
        model.buffer_jump.refresh_matches(&snapshot.text, width);
    }
}

fn start(model: &mut Model) {
    if !crate::page::editor::normal_or_explorer_input_available(model) {
        return;
    }
    let Some(source) = model.editor.active_revision() else {
        model.status = Some(StatusMessage::info("No active buffer"));
        return;
    };
    if !model.modal.open(Context::BufferJump) {
        return;
    }
    model.status = None;
    model.buffer_jump.start(source);
}

fn close(model: &mut Model) {
    model.buffer_jump.close();
    model.modal.close(Context::BufferJump);
}

#[cfg(test)]
mod tests {
    use crate::component::text_input::TextInputEvent;
    use crossterm::event::KeyCode;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, page::editor::Editor,
        test_utils::create_buffer,
    };

    fn start_jump(model: &mut Model, manager: &MessageManager) {
        manager.update(model, Message::CommandMenuStart);
        for key in ['s', 'j'] {
            manager.update(model, Message::CommandMenuKey(KeyCode::Char(key).into()));
        }
        assert!(model.buffer_jump.is_active());
        assert_eq!(
            crate::test_utils::input_context(model),
            Some(Context::BufferJump)
        );
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn leader_pattern_and_label_jump_to_the_first_visible_match() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        start_jump(&mut model, &manager);
        for character in ['e', 'l'] {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
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

        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('z')));
        assert!(!model.buffer_jump.is_active());
        assert!(model.context_stack.is_empty());
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
    fn revision_change_cancels_instead_of_accepting_stale_targets() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        start_jump(&mut model, &manager);
        for character in ['e', 'l'] {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
        }
        assert!(!model.buffer_jump.targets().is_empty());

        {
            let mut editor = model.editor.active_editor().unwrap();
            editor
                .move_cursor_to(editor::DocumentPosition {
                    line: 2,
                    character: 0,
                })
                .unwrap();
            editor.insert_text("!").unwrap();
        }
        let cursor_before_label = model.editor.active_snapshot().unwrap().cursor;

        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('z')));

        assert!(!model.buffer_jump.is_active());
        assert!(model.context_stack.is_empty());
        assert_eq!(
            model.editor.active_snapshot().unwrap().cursor,
            cursor_before_label
        );
        assert_eq!(
            model.editor.active_text_snapshot().unwrap().text,
            "hello\nyellow\n!else"
        );
    }

    #[test]
    fn cancelling_does_not_move_the_cursor() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        start_jump(&mut model, &manager);
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('e')));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_jump_cancel")),
        );

        assert!(!model.buffer_jump.is_active());
        assert!(model.context_stack.is_empty());
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
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        start_jump(&mut model, &manager);

        for character in ['z', 'z'] {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
        }

        assert!(!model.buffer_jump.is_active());
        assert!(model.context_stack.is_empty());
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
    fn one_character_input_highlights_following_characters_before_labels() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 8));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_jump_start")),
        );
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('e')));

        assert_eq!(model.buffer_jump.matches().len(), 4);
        assert!(model.buffer_jump.targets().is_empty());
        assert!(!model.buffer_jump.is_label_phase());
        let backend = TestBackend::new(40, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Editor::default().view(&model, frame))
            .unwrap();

        let content = terminal.backend().buffer().content();
        let text = content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(text.contains("Jump: one more"));
        let accent = model.theme.ui().action_hint.fg;
        assert!(content.iter().any(|cell| {
            cell.symbol() == "l"
                && Some(cell.fg) == accent
                && !cell.modifier.contains(Modifier::DIM)
        }));
        assert!(!content.iter().any(|cell| {
            cell.symbol() == "e"
                && Some(cell.fg) == accent
                && !cell.modifier.contains(Modifier::DIM)
        }));
        assert!(
            content
                .iter()
                .any(|cell| { cell.symbol() == "h" && cell.modifier.contains(Modifier::DIM) })
        );
    }

    #[test]
    fn renders_only_bright_labels_after_two_characters_and_hides_the_cursor() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 6));
        start_jump(&mut model, &manager);
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('e')));
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('l')));

        let backend = TestBackend::new(40, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Editor::default().view(&model, frame))
            .unwrap();

        let content = terminal.backend().buffer().content();
        let text = content.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(text.contains("Jump: label"));
        assert!(!terminal.backend().cursor_visible());
        assert!(content.iter().any(|cell| {
            cell.symbol() == "h"
                && cell.modifier.contains(Modifier::DIM)
                && cell.fg == Color::DarkGray
        }));
        assert!(
            content
                .iter()
                .any(|cell| cell.symbol() == "z" && !cell.modifier.contains(Modifier::DIM))
        );
        let match_color = model.theme.ui().action_hint.fg;
        assert!(!content.iter().any(|cell| {
            matches!(cell.symbol(), "e" | "l")
                && Some(cell.fg) == match_color
                && !cell.modifier.contains(Modifier::DIM)
        }));

        manager.update(&mut model, Message::Resize(4, 2));
        let backend = TestBackend::new(4, 2);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| Editor::default().view(&model, frame))
            .unwrap();
        assert!(!terminal.backend().cursor_visible());
    }
}
