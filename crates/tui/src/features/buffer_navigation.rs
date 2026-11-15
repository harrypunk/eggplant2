use crate::{
    buffer::{self, BufferMovement},
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BufferNavigationState {
    pending: Option<BufferMovement>,
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("next_buffer_pending"),
            "select next buffer after b",
            handle_command,
        ),
        Command::feature(
            CommandId("previous_buffer_pending"),
            "select previous buffer after b",
            handle_command,
        ),
        Command::feature(
            CommandId("buffer_navigation_cancel"),
            "cancel pending buffer navigation",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::BufferNavigation, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "next_buffer_pending" => start(model, BufferMovement::Next),
        "previous_buffer_pending" => start(model, BufferMovement::Previous),
        "buffer_navigation_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown buffer-navigation command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    let pending = model.buffer_navigation.pending;
    reset(model);
    if character != 'b' {
        return None;
    }
    let movement = pending?;
    buffer::handle_command(model, buffer_command(movement))
}

fn start(model: &mut Model, movement: BufferMovement) {
    // Only Normal mode starts these prefixes; the second key has its own context.
    if model.context_stack.last() != Some(&Context::AppNormal) {
        return;
    }
    model.buffer_navigation.pending = Some(movement);
    model.pending_input = Some(
        match movement {
            BufferMovement::Next => ']',
            BufferMovement::Previous => '[',
        }
        .to_string(),
    );
    model.context_stack.push(Context::BufferNavigation);
}

fn reset(model: &mut Model) {
    model.buffer_navigation.pending = None;
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::BufferNavigation);
}

fn buffer_command(movement: BufferMovement) -> CommandId {
    match movement {
        BufferMovement::Next => CommandId("next_buffer"),
        BufferMovement::Previous => CommandId("previous_buffer"),
    }
}

#[cfg(test)]
mod tests {
    use editor::SaveRequest;

    use super::*;
    use crate::{
        data_object::Message,
        manager::message_manager::MessageManager,
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[test]
    fn bracket_commands_switch_buffers_and_cancel_cleanly() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        let first = create_buffer(&mut model, "first");
        let second = create_buffer(&mut model, "second");
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("next_buffer_pending")),
        );
        assert_eq!(model.pending_input.as_deref(), Some("]"));
        assert_eq!(model.context_stack.last(), Some(&Context::BufferNavigation));
        manager.update(&mut model, Message::TextInput('b'));
        assert_eq!(model.editor.active_buffer(), Some(first));
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("previous_buffer_pending")),
        );
        assert_eq!(model.pending_input.as_deref(), Some("["));
        manager.update(&mut model, Message::TextInput('b'));
        assert_eq!(model.editor.active_buffer(), Some(second));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("next_buffer_pending")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("buffer_navigation_cancel")),
        );
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(model.pending_input, None);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("previous_buffer_pending")),
        );
        manager.update(&mut model, Message::TextInput('x'));
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[tokio::test]
    async fn bracket_navigation_uses_the_shared_save_before_switch_path() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        let first = open_file(&mut model, &first_path);
        let second = open_file(&mut model, &second_path);
        model.editor.activate_buffer(first).unwrap();
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();
        model.editor.enter_normal_mode();
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("next_buffer_pending")),
        );
        let request = expect_save(manager.update(&mut model, Message::TextInput('b')));
        assert_eq!(model.editor.active_buffer(), Some(second));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(first_path).unwrap(), "!first");
        assert!(!model.editor.is_modified(first).unwrap());
    }
}
