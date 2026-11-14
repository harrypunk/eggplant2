mod component;
mod state;
mod work;

pub use component::BufferGrep;
pub use state::{BufferGrepState, GrepMatch, SearchStatus};
pub use work::BufferGrepRequest;

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
use editor::DocumentPosition;

pub static SEARCH_CONTEXT: CommandMenuContext = CommandMenuContext::new(
    "search",
    &[CommandMenuItem::action(
        'c',
        "current file",
        CommandId("buffer_grep_open"),
    )],
);

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("buffer_grep_open"),
            "search current file",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_grep_close"),
            "close current-file search",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_grep_backspace"),
            "delete search character",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_grep_next"),
            "next search match",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_grep_previous"),
            "previous search match",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_grep_accept"),
            "jump to search match",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::BufferGrep, handle_input)
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("buffer_grep", component::overlay)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "buffer_grep_open" => open(model),
        "buffer_grep_close" => close(model),
        "buffer_grep_backspace" => {
            return model.buffer_grep.backspace().map(Effect::feature);
        }
        "buffer_grep_next" => model.buffer_grep.select_next(),
        "buffer_grep_previous" => model.buffer_grep.select_previous(),
        "buffer_grep_accept" => accept(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown buffer-grep command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if model.buffer_grep.source_identity() != model.editor.active_revision() {
        close(model);
        model.status = Some(StatusMessage::info(
            "Buffer changed; reopen current-file search",
        ));
        return None;
    }
    model.buffer_grep.insert(character).map(Effect::feature)
}

fn open(model: &mut Model) {
    if !matches!(
        model.context_stack.last(),
        Some(Context::AppNormal | Context::Explorer)
    ) {
        return;
    }
    let Some(source) = model.editor.active_text_snapshot() else {
        model.status = Some(StatusMessage::info("No active buffer"));
        return;
    };
    model.status = None;
    model.buffer_grep.open(source);
    model.context_stack.push(Context::BufferGrep);
}

fn close(model: &mut Model) {
    model.buffer_grep.close();
    model
        .context_stack
        .pop_if(|context| context == &Context::BufferGrep);
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
    match model.editor.move_cursor_to(position) {
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
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nworld\nyellow\nelse");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(100, 20));

        for key in [' ', 's', 'c'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        assert_eq!(model.context_stack.last(), Some(&Context::BufferGrep));

        let mut effect = None;
        for character in "ell".chars() {
            effect = manager.update(&mut model, Message::TextInput(character));
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
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn cancelling_search_preserves_the_cursor() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        for key in [' ', 's', 'c'] {
            manager.update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)));
        }
        manager.update(&mut model, Message::TextInput('e'));
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
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }
}
