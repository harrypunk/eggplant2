mod component;
mod state;
mod work;

pub use component::BufferGrep;
pub use state::{BufferGrepState, GrepMatch, SearchStatus};
pub use work::BufferGrepRequest;

use crate::{
    command_menu::CommandMenuItem,
    component::text_input::TextInputEvent,
    data_object::{CommandId, Context, GeneralAction, Model, StatusMessage},
    focus,
    manager::{
        command_registry::Command,
        effect::Effect,
        general_action::GeneralActionSubscription,
        keymap_contribution::{ContextKeymap, KeymapCapability},
    },
    ui::UiRegistry,
};
use editor::DocumentPosition;

pub const KEYMAPS: &[ContextKeymap] = &[ContextKeymap::new("buffer_grep", Context::BufferGrep)
    .with_capabilities(&[
        KeymapCapability::ListNavigation,
        KeymapCapability::ModalActions,
    ])];

pub static CURRENT_FILE_MENU_ITEM: CommandMenuItem =
    CommandMenuItem::action('c', "current file", CommandId("buffer_grep_open"));

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("buffer_grep_open"),
            "search current file",
            open_command,
        ),
        Command::feature(
            CommandId("buffer_grep_close"),
            "close current-file search",
            close_command,
        ),
        Command::feature(CommandId("buffer_grep_next"), "next search match", next),
        Command::feature(
            CommandId("buffer_grep_previous"),
            "previous search match",
            previous,
        ),
        Command::feature(
            CommandId("buffer_grep_accept"),
            "jump to search match",
            accept_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("buffer_grep", component::overlay)
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![GeneralActionSubscription::new(
        Context::BufferGrep,
        handle_general_action,
    )]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => {
            model.buffer_grep.select_next();
            model.buffer_grep.refresh_preview(&model.editor);
        }
        GeneralAction::Previous => {
            model.buffer_grep.select_previous();
            model.buffer_grep.refresh_preview(&model.editor);
        }
        GeneralAction::Activate => accept(model),
        GeneralAction::Back => close(model),
        GeneralAction::FocusNext | GeneralAction::Paste => {}
    }
    None
}

fn open_command(model: &mut Model) -> Option<Effect> {
    open(model);
    None
}

fn close_command(model: &mut Model) -> Option<Effect> {
    close(model);
    None
}

fn next(model: &mut Model) -> Option<Effect> {
    model.buffer_grep.select_next();
    model.buffer_grep.refresh_preview(&model.editor);
    None
}

fn previous(model: &mut Model) -> Option<Effect> {
    model.buffer_grep.select_previous();
    model.buffer_grep.refresh_preview(&model.editor);
    None
}

fn accept_command(model: &mut Model) -> Option<Effect> {
    accept(model);
    None
}

pub fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    if model.buffer_grep.source_identity() != model.editor.active_revision() {
        close(model);
        model.status = Some(StatusMessage::info(
            "Buffer changed; reopen current-file search",
        ));
        return None;
    }
    match model
        .buffer_grep
        .query
        .handle_event(event)
        .content_changed()
    {
        true => model.buffer_grep.start_search().map(Effect::feature),
        false => None,
    }
}

fn open(model: &mut Model) {
    // Only Normal or Explorer may open grep; another modal keeps its input context.
    if !crate::page::editor::normal_or_explorer_input_available(model) {
        return;
    }
    let Some(source) = model.editor.active_text_snapshot() else {
        model.status = Some(StatusMessage::info("No active buffer"));
        return;
    };
    if !model.modal.open(Context::BufferGrep) {
        return;
    }
    model.status = None;
    model.buffer_grep.open(source);
}

fn close(model: &mut Model) {
    model.buffer_grep.close();
    model.modal.close(Context::BufferGrep);
}

fn accept(model: &mut Model) {
    let Some(source) = model.buffer_grep.source_identity() else {
        close(model);
        return;
    };
    let Some(result) = model.buffer_grep.selected_match() else {
        return;
    };
    let position = DocumentPosition {
        line: result.line,
        character: 0,
    };
    close(model);
    if model.editor.active_revision() != Some(source) {
        model.status = Some(StatusMessage::error(
            "Buffer changed before the search match could be opened",
        ));
        return;
    }
    match model
        .editor
        .active_editor()
        .and_then(|mut editor| editor.move_cursor_to(position))
    {
        Ok(()) => focus::focus_editor(model),
        Err(error) => model.status = Some(StatusMessage::error(error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    #[test]
    fn leader_live_search_selection_and_acceptance_jump_to_the_match_line() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello\nworld\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(100, 20));

        manager.update(&mut model, Message::CommandMenuStart);
        for key in ['s', 'c'] {
            manager.update(
                &mut model,
                Message::CommandMenuKey(KeyCode::Char(key).into()),
            );
        }
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::BufferGrep)
        );
        assert!(model.context_stack.is_empty());

        let mut effect = None;
        for character in "ell".chars() {
            effect = manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
        }
        let Some(Effect::Feature(request)) = effect else {
            panic!("a non-empty query should start a search effect");
        };
        manager.update(&mut model, request.perform());
        assert_eq!(model.buffer_grep.results.len(), 2);
        assert_eq!(model.buffer_grep.results[0].line, 0);
        assert_eq!(model.buffer_grep.results[1].line, 2);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_grep_next")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_grep_accept")),
        );

        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 2);
        assert_eq!(snapshot.cursor.unwrap().column, 0);
        assert_eq!(
            model.buffer_grep.visibility,
            crate::data_object::Visibility::Hidden
        );
        assert!(model.context_stack.is_empty());
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn cancelling_search_preserves_the_cursor() {
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(&mut model, Message::CommandMenuStart);
        for key in ['s', 'c'] {
            manager.update(
                &mut model,
                Message::CommandMenuKey(KeyCode::Char(key).into()),
            );
        }
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('e')));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_grep_close")),
        );

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
        assert!(model.context_stack.is_empty());
    }
}
