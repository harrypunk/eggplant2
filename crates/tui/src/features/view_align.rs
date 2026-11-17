use editor::ViewAlignment;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ViewAlignSession;

impl ViewAlignSession {
    pub fn new() -> Self {
        Self
    }

    fn finish(self, character: char) -> Option<ViewAlignment> {
        match character {
            'z' => Some(ViewAlignment::Center),
            't' => Some(ViewAlignment::Top),
            'b' => Some(ViewAlignment::Bottom),
            _ => None,
        }
    }
}

use crate::{
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("view_align_start"),
            "align cursor line in viewport",
            handle_command,
        ),
        Command::feature(
            CommandId("view_align_cancel"),
            "cancel pending viewport alignment",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::ViewAlign, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "view_align_start" => start(model),
        "view_align_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown view-alignment command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    let alignment = model
        .view_align
        .take()
        .and_then(|session| session.finish(character));
    reset(model);
    if let Some(alignment) = alignment
        && let Ok(mut editor) = model.editor.active_editor()
    {
        let _ = editor.align_cursor_line(alignment);
    }
    None
}

fn start(model: &mut Model) {
    // The normal keymap owns the initial z; other contexts cannot start this prefix.
    let Some(Context::AppNormal) = model.context_stack.last() else {
        return;
    };
    model.pending_input = Some("z".to_string());
    model.view_align = Some(ViewAlignSession::new());
    model.context_stack.push(Context::ViewAlign);
}

fn reset(model: &mut Model) {
    model.view_align = None;
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::ViewAlign);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    fn start_alignment(model: &mut Model, manager: &MessageManager) {
        manager.update(model, Message::InvokeCommand(CommandId("view_align_start")));
        assert_eq!(model.pending_input.as_deref(), Some("z"));
        assert!(model.view_align.is_some());
        assert_eq!(model.context_stack.last(), Some(&Context::ViewAlign));
    }

    #[test]
    fn session_parses_alignment_without_a_model() {
        for (character, expected) in [
            ('z', ViewAlignment::Center),
            ('t', ViewAlignment::Top),
            ('b', ViewAlignment::Bottom),
        ] {
            assert_eq!(ViewAlignSession::new().finish(character), Some(expected));
        }
        assert_eq!(ViewAlignSession::new().finish('x'), None);
    }

    #[test]
    fn zt_zz_and_zb_align_without_changing_the_document() {
        let text = (0..30)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, &text);
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(50, 10));
        model
            .editor
            .active_editor()
            .unwrap()
            .move_cursor_to_line(10)
            .unwrap();
        let revision = model.editor.active_revision();

        start_alignment(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('t'));
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line, 10);
        assert_eq!(snapshot.cursor.unwrap().row, 0);
        assert_eq!(model.pending_input, None);

        start_alignment(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('z'));
        assert_eq!(model.editor.active_snapshot().unwrap().text.first_line, 7);

        start_alignment(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('b'));
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.text.first_line, 3);
        assert_eq!(snapshot.cursor.unwrap().row, 7);
        assert_eq!(model.editor.active_revision(), revision);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn escape_and_invalid_input_clear_pending_state_without_alignment() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));
        let offset = model.editor.active_snapshot().unwrap().text.first_line;

        start_alignment(&mut model, &manager);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("view_align_cancel")),
        );
        assert_eq!(model.pending_input, None);
        assert!(model.view_align.is_none());
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        start_alignment(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('x'));
        assert_eq!(model.pending_input, None);
        assert!(model.view_align.is_none());
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.first_line,
            offset
        );

        manager.update(&mut model, Message::Resize(0, 0));
        start_alignment(&mut model, &manager);
        manager.update(&mut model, Message::TextInput('z'));
        assert_eq!(model.pending_input, None);
    }
}
