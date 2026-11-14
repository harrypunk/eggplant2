use crate::{
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LineJumpState {
    count: Option<usize>,
    pending_g: bool,
}

impl LineJumpState {
    fn is_active(self) -> bool {
        self.count.is_some() || self.pending_g
    }

    fn pending_text(self) -> Option<String> {
        match (self.count, self.pending_g) {
            (Some(count), true) => Some(format!("{count}g")),
            (Some(count), false) => Some(count.to_string()),
            (None, true) => Some("g".to_string()),
            (None, false) => None,
        }
    }

    fn push_digit(&mut self, digit: usize) {
        self.count = Some(
            self.count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit),
        );
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("line_jump_cancel"),
        "cancel pending line jump",
        handle_command,
    ))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::AppNormal, handle_input)?;
    input.register(Context::LineJump, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "line_jump_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown line-jump command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

/// Consume only count and gg/G input; unrelated printable input remains a no-op.
pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    if !model.line_jump.is_active() {
        match character {
            '1'..='9' => {
                model
                    .line_jump
                    .push_digit(character as usize - '0' as usize);
                activate(model);
            }
            'g' => {
                model.line_jump.pending_g = true;
                activate(model);
            }
            'G' => complete(model, false),
            _ => {}
        }
        return None;
    }

    if model.line_jump.pending_g {
        match character {
            'g' => complete(model, true),
            _ => reset(model),
        }
        return None;
    }

    match character {
        '0'..='9' => {
            model
                .line_jump
                .push_digit(character as usize - '0' as usize);
            update_pending(model);
        }
        'g' => {
            model.line_jump.pending_g = true;
            update_pending(model);
        }
        'G' => complete(model, false),
        _ => reset(model),
    }
    None
}

fn activate(model: &mut Model) {
    if model.context_stack.last() != Some(&Context::LineJump) {
        model.context_stack.push(Context::LineJump);
    }
    update_pending(model);
}

fn update_pending(model: &mut Model) {
    model.pending_input = model.line_jump.pending_text();
}

fn complete(model: &mut Model, first_line: bool) {
    let line = match (first_line, model.line_jump.count) {
        (true, None) => 0,
        (false, None) => usize::MAX,
        (_, Some(count)) => count.saturating_sub(1),
    };
    reset(model);
    let _ = model.editor.move_cursor_to_line(line);
}

fn reset(model: &mut Model) {
    model.line_jump.clear();
    model.pending_input = None;
    model
        .context_stack
        .pop_if(|context| context == &Context::LineJump);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

    fn input(model: &mut Model, manager: &MessageManager, text: &str) {
        for character in text.chars() {
            manager.update(model, Message::TextInput(character));
        }
    }

    fn cursor_line(model: &Model) -> usize {
        let snapshot = model.editor.active_snapshot().unwrap();
        snapshot.text.first_line + snapshot.cursor.unwrap().row
    }

    #[test]
    fn counted_and_uncounted_jumps_show_pending_input() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "  éirst\nsecond\nthird\n\n\tfourth");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "g");
        assert_eq!(model.pending_input.as_deref(), Some("g"));
        assert_eq!(model.context_stack.last(), Some(&Context::LineJump));
        input(&mut model, &manager, "g");
        assert_eq!(cursor_line(&model), 0);
        assert_eq!(model.pending_input, None);

        input(&mut model, &manager, "3g");
        input(&mut model, &manager, "g");
        assert_eq!(cursor_line(&model), 2);
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

        input(&mut model, &manager, "2");
        assert_eq!(model.pending_input.as_deref(), Some("2"));
        input(&mut model, &manager, "G");
        let snapshot = model.editor.active_snapshot().unwrap();
        assert_eq!(snapshot.cursor.unwrap().column, 0);

        input(&mut model, &manager, "5G");
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            4
        );
        input(&mut model, &manager, "99G");
        assert_eq!(cursor_line(&model), 4);

        input(&mut model, &manager, "G");
        assert_eq!(cursor_line(&model), 4);
    }

    #[test]
    fn empty_buffer_clamps_large_counts_without_changes() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "999999999999999999999999G");

        assert_eq!(cursor_line(&model), 0);
        assert_eq!(model.pending_input, None);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn escape_invalid_input_and_zero_do_not_move() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(40, 4));

        input(&mut model, &manager, "0");
        assert_eq!(model.pending_input, None);
        input(&mut model, &manager, "2");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("line_jump_cancel")),
        );
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(cursor_line(&model), 0);

        input(&mut model, &manager, "2g");
        input(&mut model, &manager, "x");
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert_eq!(cursor_line(&model), 0);
    }
}
