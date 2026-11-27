use crate::{
    buffer::{self, BufferMovement},
    data_object::{CommandId, Context, Model, PendingInput},
    manager::{command_registry::Command, effect::Effect, keymap_contribution::ContextKeymap},
};

pub const KEYMAPS: &[ContextKeymap] =
    &[
        ContextKeymap::new("buffer_navigation", Context::BufferNavigation)
            .with_commands(&[("buffer_navigation_cancel", "esc")]),
    ];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BufferNavigationState {
    pending: Option<BufferMovement>,
}

impl BufferNavigationState {
    fn begin(&mut self, movement: BufferMovement) -> &'static str {
        self.pending = Some(movement);
        match movement {
            BufferMovement::Next => "]",
            BufferMovement::Previous => "[",
        }
    }

    fn finish(&mut self, character: char) -> Option<BufferMovement> {
        let pending = self.pending.take();
        if character == 'b' { pending } else { None }
    }

    fn clear(&mut self) {
        self.pending = None;
    }
}

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("next_buffer_pending"),
            "select next buffer after b",
            next,
        ),
        Command::feature(
            CommandId("previous_buffer_pending"),
            "select previous buffer after b",
            previous,
        ),
        Command::feature(
            CommandId("buffer_navigation_cancel"),
            "cancel pending buffer navigation",
            cancel,
        ),
    ]
    .into_iter()
    .collect()
}

fn next(model: &mut Model) -> Option<Effect> {
    start(model, BufferMovement::Next);
    None
}

fn previous(model: &mut Model) -> Option<Effect> {
    start(model, BufferMovement::Previous);
    None
}

fn cancel(model: &mut Model) -> Option<Effect> {
    reset(model);
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    let movement = model.buffer_navigation.finish(character);
    reset(model);
    buffer::switch(model, movement?)
}

fn start(model: &mut Model, movement: BufferMovement) {
    // Only Normal mode starts these prefixes; the second key has its own context.
    if !crate::editing::target::main_normal_input_available(model) {
        return;
    }
    let pending = model.buffer_navigation.begin(movement);
    model.pending_input = Some(PendingInput::new(Context::BufferNavigation, pending));
    model.context_stack.push(Context::BufferNavigation);
}

fn reset(model: &mut Model) {
    model.buffer_navigation.clear();
    PendingInput::clear(&mut model.pending_input, Context::BufferNavigation);
    model
        .context_stack
        .pop_if(|context| context == &Context::BufferNavigation);
}

#[cfg(test)]
mod tests {
    use crate::component::text_input::TextInputEvent;
    use editor::SaveRequest;

    use super::*;
    use crate::{
        data_object::Message,
        manager::message_manager::MessageManager,
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        match effect {
            Some(Effect::Save(request)) => request,
            Some(Effect::Batch(effects)) => effects
                .into_iter()
                .map(|effect| expect_save(Some(effect)))
                .next()
                .expect("expected a save effect"),
            _ => panic!("expected a save effect"),
        }
    }

    #[test]
    fn state_parses_prefix_completion_without_a_model() {
        let mut state = BufferNavigationState::default();
        assert_eq!(state.begin(BufferMovement::Next), "]");
        assert_eq!(state.finish('x'), None);
        assert_eq!(state.pending, None);

        assert_eq!(state.begin(BufferMovement::Previous), "[");
        assert_eq!(state.finish('b'), Some(BufferMovement::Previous));
        assert_eq!(state.pending, None);
        assert_eq!(state.finish('b'), None);

        state.begin(BufferMovement::Next);
        state.clear();
        assert_eq!(state.pending, None);
    }

    #[test]
    fn bracket_commands_switch_buffers_and_cancel_cleanly() {
        let mut model = Model::default();
        let first = create_buffer(&mut model, "first");
        let second = create_buffer(&mut model, "second");
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("next_buffer_pending")),
        );
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("]")
        );
        assert_eq!(model.context_stack.last(), Some(&Context::BufferNavigation));
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('b')));
        assert_eq!(model.editor.active_buffer(), Some(first));
        assert_eq!(model.pending_input, None);
        assert!(model.context_stack.is_empty());

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("previous_buffer_pending")),
        );
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("[")
        );
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('b')));
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
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('x')));
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(model.pending_input, None);
        assert!(model.context_stack.is_empty());
    }

    #[tokio::test]
    async fn bracket_navigation_uses_the_shared_save_before_switch_path() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = Model::default();
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
        let request = expect_save(
            manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('b'))),
        );
        assert_eq!(model.editor.active_buffer(), Some(second));
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));

        assert_eq!(std::fs::read_to_string(first_path).unwrap(), "!first");
        assert!(!model.editor.is_modified(first).unwrap());
    }
}
