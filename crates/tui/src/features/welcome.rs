use crate::{
    data_object::{CommandId, Context, Model},
    manager::{
        command_registry::Command,
        effect::Effect,
        keymap_contribution::{ContextKeymap, KeymapCapability},
    },
};

pub const KEYMAPS: &[ContextKeymap] = &[ContextKeymap::new("welcome", Context::Welcome)
    .with_commands(&[
        ("welcome_previous", "up"),
        ("welcome_next", "down"),
        ("welcome_activate", "enter"),
        ("focus_left", "ctrl-h"),
        ("focus_right", "ctrl-l"),
    ])
    .with_capabilities(&[KeymapCapability::ListNavigation])];

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("welcome_next"),
            "select next welcome command",
            handle_command,
        ),
        Command::feature(
            CommandId("welcome_previous"),
            "select previous welcome command",
            handle_command,
        ),
        Command::dispatch(
            CommandId("welcome_activate"),
            "run selected welcome command",
            selected_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if !available(model) {
        return None;
    }
    match id.as_str() {
        "welcome_next" => model.welcome.select_next(),
        "welcome_previous" => model.welcome.select_previous(),
        _ => return None,
    }
    None
}

fn available(model: &Model) -> bool {
    crate::page::editor::welcome_available(model)
        && model.modal.active_context().is_none()
        && model.context_stack.is_empty()
}

fn selected_command(model: &Model) -> Option<CommandId> {
    available(model).then(|| model.welcome.selected_command())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{Context, FocusTarget, Message, Visibility},
        manager::message_manager::MessageManager,
        startup::initial_model,
    };

    fn welcome_model() -> Model {
        Model::default()
    }

    #[test]
    fn navigation_and_activation_use_existing_commands() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.current_focus = FocusTarget::Editor;
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("welcome_next")),
        );
        assert_eq!(model.welcome.selected(), 1);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("welcome_activate")),
        );
        assert_eq!(model.explorer.visibility, Visibility::Hidden);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("welcome_previous")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("welcome_activate")),
        );
        assert_eq!(model.file_picker.visibility, Visibility::Displayed);
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::FilePicker)
        );
        assert!(model.modal.is_active(Context::FilePicker));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn commands_do_nothing_when_a_buffer_is_active() {
        let mut model = welcome_model();
        crate::test_utils::create_buffer(&mut model, "text");
        let selected = model.welcome.selected();
        handle_command(&mut model, CommandId("welcome_next"));
        assert_eq!(model.welcome.selected(), selected);
    }

    #[test]
    fn availability_is_derived_without_mutating_the_context_stack() {
        let mut model = welcome_model();
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::Welcome)
        );
        assert!(model.context_stack.is_empty());

        crate::test_utils::create_buffer(&mut model, "text");
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::AppNormal)
        );
        assert!(model.context_stack.is_empty());
    }
}
