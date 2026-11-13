use editor::CursorMovement;

use crate::{
    buffer,
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::effect::Effect,
};

/// Editing feature commands, independent of physical keybindings.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "enter_insert_mode" => {
            if model.editor.active_buffer().is_some() {
                model.context_stack.push(Context::EditorInput);
                model.editor.enter_insert_mode();
            }
            None
        }
        "exit_insert_mode" => {
            model
                .context_stack
                .pop_if(|context| context == &Context::EditorInput);
            model.editor.enter_normal_mode();
            buffer::auto_save_active(model)
        }
        "move_left" => move_cursor(model, CursorMovement::Left),
        "move_right" => move_cursor(model, CursorMovement::Right),
        "move_up" => move_cursor(model, CursorMovement::Up),
        "move_down" => move_cursor(model, CursorMovement::Down),
        "backspace" => {
            if model.editor.backspace().is_ok() {
                return buffer::auto_save_active(model);
            }
            None
        }
        "insert_newline" => insert(model, "\n"),
        "undo" => {
            if model.editor.undo().unwrap_or(false) {
                return buffer::auto_save_active(model);
            }
            None
        }
        "redo" => {
            if model.editor.redo().unwrap_or(false) {
                return buffer::auto_save_active(model);
            }
            None
        }
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown editing command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

pub(crate) fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    insert(model, &character.to_string())
}

fn move_cursor(model: &mut Model, movement: CursorMovement) -> Option<Effect> {
    let _ = model.editor.move_cursor(movement);
    None
}

fn insert(model: &mut Model, text: &str) -> Option<Effect> {
    if model.editor.insert_text(text).is_ok() {
        return buffer::auto_save_active(model);
    }
    None
}

#[cfg(test)]
mod tests {
    use editor::{EditMode, SaveRequest};

    use super::*;
    use crate::{
        data_object::{Message, StatusKind},
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
    fn mode_commands_update_editor_session_and_context() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(model.context_stack.last(), Some(&Context::EditorInput));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn insert_mode_requires_an_active_buffer() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );

        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn editing_commands_update_text_history_and_viewport() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "a");
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('b'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ab"
        );

        manager.update(&mut model, Message::InvokeCommand(CommandId("backspace")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "a"
        );
        manager.update(&mut model, Message::InsertChar('c'));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ac"
        );

        manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "a"
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("redo")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ac"
        );
    }

    #[tokio::test]
    async fn edit_auto_save_executes_effect_and_applies_completion() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        open_file(&mut model, &path);
        for _ in 0..5 {
            model.editor.move_cursor(CursorMovement::Right).unwrap();
        }
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::InsertChar('!'));
        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("exit_insert_mode")),
            )
            .unwrap();
        let request = expect_save(Some(effect));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello!");
        assert!(!model.editor.active_is_modified());
        assert_eq!(model.status.unwrap().kind, StatusKind::Info);
    }
}
