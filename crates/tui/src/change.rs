use crate::{
    buffer,
    data_object::{CommandId, Context, Model, StatusMessage},
    editing,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChangeState {
    pending: Option<PendingChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChange {
    Delete,
    Change,
}

impl PendingChange {
    fn key(self) -> char {
        match self {
            Self::Delete => 'd',
            Self::Change => 'c',
        }
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("delete_grapheme"),
            "delete grapheme under cursor",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_selection_x"),
            "delete selection",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_selection_d"),
            "delete selection",
            handle_command,
        ),
        Command::feature(
            CommandId("change_selection"),
            "change selection",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_line_pending"),
            "delete current line",
            handle_command,
        ),
        Command::feature(
            CommandId("change_line_pending"),
            "change current line",
            handle_command,
        ),
        Command::feature(
            CommandId("change_cancel"),
            "cancel pending delete/change",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::EditorChange, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "delete_grapheme" => return delete_grapheme(model),
        "delete_selection_x" | "delete_selection_d" => return delete_selection(model, false),
        "change_selection" => return delete_selection(model, true),
        "delete_line_pending" => start(model, PendingChange::Delete),
        "change_line_pending" => start(model, PendingChange::Change),
        "change_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown change command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    let pending = model.change.pending;
    match (pending, character) {
        (Some(PendingChange::Delete), 'd') => return delete_line(model),
        (Some(PendingChange::Change), 'c') => return change_line(model),
        (Some(_), _) => reset(model),
        (None, _) => {}
    }
    None
}

fn start(model: &mut Model, pending: PendingChange) {
    // Normal d/c are prefixes; Visual d/c use direct selection commands instead.
    let Some(Context::AppNormal) = model.context_stack.last() else {
        return;
    };
    model.change.pending = Some(pending);
    model.pending_input = Some(pending.key().to_string());
    model.context_stack.push(Context::EditorChange);
}

fn reset(model: &mut Model) {
    model.change.pending = None;
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorChange);
}

fn delete_grapheme(model: &mut Model) -> Option<Effect> {
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.delete_grapheme_forward();
    }
    buffer::auto_save_active(model)
}

fn delete_selection(model: &mut Model, insert: bool) -> Option<Effect> {
    if let Ok(mut editor) = model.editor.active_editor() {
        if insert {
            let _ = editor.change_selection();
        } else {
            let _ = editor.delete_selection();
        }
    }
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorVisual);
    if insert {
        editing::enter_insert(model);
        None
    } else {
        model.editor.enter_normal_mode();
        buffer::auto_save_active(model)
    }
}

fn delete_line(model: &mut Model) -> Option<Effect> {
    reset(model);
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.delete_current_line();
    }
    buffer::auto_save_active(model)
}

fn change_line(model: &mut Model) -> Option<Effect> {
    reset(model);
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.change_current_line();
    }
    editing::enter_insert(model);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    #[test]
    fn pending_dd_and_cc_use_status_and_cancel_cleanly() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        assert_eq!(model.pending_input.as_deref(), Some("d"));
        manager.update(&mut model, Message::TextInput('d'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "second"
        );
        assert_eq!(model.pending_input, None);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_line_pending")),
        );
        assert_eq!(model.pending_input.as_deref(), Some("c"));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_cancel")),
        );
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('c'));
        assert_eq!(model.editor.mode(), editor::EditMode::Insert);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            ""
        );

        assert_eq!(model.pending_input, None);
    }

    #[test]
    fn visual_delete_and_change_use_the_primary_selection() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_selection_d")),
        );

        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "llo"
        );
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn visual_change_enters_insert_and_undoes_as_one_edit() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_selection")),
        );
        manager.update(&mut model, Message::TextInput('X'));
        assert_eq!(model.editor.mode(), editor::EditMode::Insert);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "Xllo"
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));

        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "hello"
        );
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
    }
}
