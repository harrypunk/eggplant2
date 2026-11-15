use crate::{
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct YankState {
    pending: bool,
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("yank_selection"),
            "yank selection",
            handle_command,
        ),
        Command::feature(
            CommandId("yank_line_pending"),
            "yank current line",
            handle_command,
        ),
        Command::feature(
            CommandId("yank_cancel"),
            "cancel pending yank",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::EditorYank, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "yank_selection" => return yank_selection(model),
        "yank_line_pending" => start(model),
        "yank_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown yank command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if !model.yank.pending {
        return None;
    }
    match character {
        'y' => return yank_current_line(model),
        _ => reset(model),
    }
    None
}

fn yank_selection(model: &mut Model) -> Option<Effect> {
    let text = match model.editor.active_editor() {
        Ok(editor) => editor.selected_text(),
        Err(_) => return None,
    };
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorVisual);
    model.editor.enter_normal_mode();
    model.register.record(text)
}

fn yank_current_line(model: &mut Model) -> Option<Effect> {
    let text = match model.editor.active_editor() {
        Ok(editor) => editor.current_line_text(),
        Err(_) => {
            reset(model);
            return None;
        }
    };
    reset(model);
    model.register.record(text)
}

fn start(model: &mut Model) {
    // Normal y waits for the second y; Visual y is a direct selection command.
    let Some(Context::AppNormal) = model.context_stack.last() else {
        return;
    };
    model.yank.pending = true;
    model.pending_input = Some("y".to_string());
    model.context_stack.push(Context::EditorYank);
}

fn reset(model: &mut Model) {
    model.yank.pending = false;
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorYank);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    #[test]
    fn visual_yank_updates_register_without_changing_text() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nworld");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        let revision = model.editor.active_revision();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_selection")),
        );

        assert_eq!(model.register.unnamed(), "he");
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "hello"
        );
        assert_eq!(model.editor.active_revision(), revision);
        assert!(effect.is_some());
    }

    #[test]
    fn yy_yanks_line_and_escape_cancels_pending_yank() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nworld");
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_line_pending")),
        );
        assert_eq!(model.pending_input.as_deref(), Some("y"));
        manager.update(&mut model, Message::InvokeCommand(CommandId("yank_cancel")));
        assert_eq!(model.pending_input, None);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('y'));
        assert_eq!(model.register.unnamed(), "hello\n");
        assert_eq!(model.pending_input, None);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn linewise_visual_yank_includes_selected_line_endings() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond\nthird");
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_line_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_down")));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_selection")),
        );

        assert_eq!(model.register.unnamed(), "first\nsecond\n");
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert!(!model.editor.active_is_modified());
    }
}
