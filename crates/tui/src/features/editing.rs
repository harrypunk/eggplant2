use editor::{CursorMovement, InsertPosition, LineBoundary, OpenLineDirection, WordMovement};

use crate::{
    buffer,
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::effect::Effect,
};

/// Editing feature commands, independent of physical keybindings.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "enter_visual_mode" => enter_visual(model),
        "enter_visual_line_mode" => enter_visual_line(model),
        "exit_visual_mode" => {
            model
                .context_stack
                .pop_if(|context| context == &Context::EditorVisual);
            model.editor.enter_normal_mode();
            None
        }
        "enter_insert_mode" => enter_insert(model),
        "insert_after_cursor" => enter_insert_at(model, InsertPosition::AfterCursor),
        "insert_at_line_end" => enter_insert_at(model, InsertPosition::LineEnd),
        "insert_at_first_nonblank" => enter_insert_at(model, InsertPosition::FirstNonBlank),
        "open_line_below" => open_line(model, OpenLineDirection::Below),
        "open_line_above" => open_line(model, OpenLineDirection::Above),
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
        "move_line_start" => move_line_boundary(model, LineBoundary::Start),
        "move_line_end" => move_line_boundary(model, LineBoundary::End),
        "move_next_word_start" => move_word(model, WordMovement::NextStart),
        "move_previous_word_start" => move_word(model, WordMovement::PreviousStart),
        "move_next_word_end" => move_word(model, WordMovement::NextEnd),
        "backspace" => {
            if model
                .editor
                .active_editor()
                .and_then(|mut editor| editor.backspace())
                .is_ok()
            {
                return buffer::auto_save_active(model);
            }
            None
        }
        "insert_newline" => insert(model, "\n"),
        "undo" => {
            if model
                .editor
                .active_editor()
                .and_then(|mut editor| editor.undo())
                .unwrap_or(false)
            {
                return buffer::auto_save_active(model);
            }
            None
        }
        "redo" => {
            if model
                .editor
                .active_editor()
                .and_then(|mut editor| editor.redo())
                .unwrap_or(false)
            {
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

fn enter_visual(model: &mut Model) -> Option<Effect> {
    if model.editor.active_buffer().is_some() {
        model.context_stack.push(Context::EditorVisual);
        model.editor.enter_visual_mode();
    }
    None
}

fn enter_visual_line(model: &mut Model) -> Option<Effect> {
    if model.editor.active_buffer().is_some() {
        model.context_stack.push(Context::EditorVisual);
        model.editor.enter_visual_line_mode();
    }
    None
}

pub(crate) fn enter_insert(model: &mut Model) -> Option<Effect> {
    if model.editor.active_buffer().is_some() {
        model.context_stack.push(Context::EditorInput);
        model.editor.enter_insert_mode();
    }
    None
}

fn enter_insert_at(model: &mut Model, position: InsertPosition) -> Option<Effect> {
    if model
        .editor
        .active_editor()
        .and_then(|mut editor| editor.move_cursor_to_insert(position))
        .is_ok()
    {
        enter_insert(model);
    }
    None
}

fn open_line(model: &mut Model, direction: OpenLineDirection) -> Option<Effect> {
    if model
        .editor
        .active_editor()
        .and_then(|mut editor| editor.open_line(direction))
        .is_ok()
    {
        enter_insert(model);
        return buffer::auto_save_active(model);
    }
    None
}

fn move_cursor(model: &mut Model, movement: CursorMovement) -> Option<Effect> {
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.move_cursor(movement);
    }
    None
}

fn move_line_boundary(model: &mut Model, boundary: LineBoundary) -> Option<Effect> {
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.move_cursor_to_line_boundary(boundary);
    }
    None
}

fn move_word(model: &mut Model, movement: WordMovement) -> Option<Effect> {
    if let Ok(mut editor) = model.editor.active_editor() {
        let _ = editor.move_cursor_word(movement);
    }
    None
}

fn insert(model: &mut Model, text: &str) -> Option<Effect> {
    if model
        .editor
        .active_editor()
        .and_then(|mut editor| editor.insert_text(text))
        .is_ok()
    {
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
        manager.update(&mut model, Message::TextInput('b'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ab"
        );

        manager.update(&mut model, Message::InvokeCommand(CommandId("backspace")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "a"
        );
        manager.update(&mut model, Message::TextInput('c'));
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

    #[test]
    fn word_motion_commands_move_the_normal_mode_cursor() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello world");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_next_word_start")),
        );
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            6
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_next_word_end")),
        );
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            10
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_previous_word_start")),
        );
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            6
        );
    }

    #[test]
    fn visual_mode_extends_snapshot_selection_and_escape_collapses_it() {
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
        assert_eq!(model.editor.mode(), EditMode::Visual);
        assert_eq!(model.context_stack.last(), Some(&Context::EditorVisual));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].selection,
            Some(0..2)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_visual_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].selection,
            None
        );
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn visual_word_and_counted_line_motions_extend_the_same_selection() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello world\nthird");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_next_word_start")),
        );
        assert_eq!(
            model.editor.active_editor().unwrap().selected_text(),
            "hello w"
        );
        manager.update(&mut model, Message::TextInput('2'));
        manager.update(&mut model, Message::TextInput('G'));
        assert!(
            model
                .editor
                .active_editor()
                .unwrap()
                .selected_text()
                .contains("hello world\n")
        );
        assert_eq!(model.editor.mode(), EditMode::Visual);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn visual_line_mode_selects_whole_lines_and_escape_preserves_text() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond\nthird");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_line_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_down")));

        assert_eq!(model.editor.mode(), EditMode::VisualLine);
        assert_eq!(
            model.editor.active_editor().unwrap().selected_text(),
            "first\nsecond\n"
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].selection,
            Some(0..5)
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[1].selection,
            Some(0..6)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_visual_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "first"
        );
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn line_boundary_commands_move_without_changing_text() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, " hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(120, 4));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_end")),
        );
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            5
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_start")),
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
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn insert_variants_accept_text_at_the_expected_positions() {
        for (command, text, expected) in [
            ("insert_after_cursor", "hello", "hxello"),
            ("insert_at_line_end", "hello", "hellox"),
            ("insert_at_first_nonblank", "  hello", "  xhello"),
        ] {
            let mut model = Model::default();
            model.context_stack.push(Context::AppNormal);
            create_buffer(&mut model, text);
            let manager = MessageManager::default();
            manager.update(&mut model, Message::Resize(40, 4));

            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            manager.update(&mut model, Message::TextInput('x'));

            assert_eq!(model.editor.mode(), EditMode::Insert);
            assert_eq!(model.context_stack.last(), Some(&Context::EditorInput));
            assert_eq!(
                model.editor.active_snapshot().unwrap().text.lines[0].text,
                expected
            );
        }
    }

    #[test]
    fn open_line_and_typing_share_one_undoable_insert_session() {
        for command in ["open_line_below", "open_line_above"] {
            let mut model = Model::default();
            model.context_stack.push(Context::AppNormal);
            create_buffer(&mut model, "first");
            let manager = MessageManager::default();
            manager.update(&mut model, Message::Resize(40, 4));

            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            manager.update(&mut model, Message::TextInput('x'));
            assert_eq!(model.editor.mode(), EditMode::Insert);
            manager.update(
                &mut model,
                Message::InvokeCommand(CommandId("exit_insert_mode")),
            );
            manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));

            assert_eq!(
                model.editor.active_snapshot().unwrap().text.lines[0].text,
                "first"
            );
            assert_eq!(model.editor.active_snapshot().unwrap().text.line_count, 1);
        }
    }

    #[tokio::test]
    async fn open_line_auto_saves_when_leaving_insert() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "first\nsecond").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        open_file(&mut model, &path);
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_line_below")),
        );
        let request = expect_save(manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        ));
        let completion = request.perform().await;
        manager.update(&mut model, Message::SaveFinished(completion));

        assert_eq!(std::fs::read_to_string(path).unwrap(), "first\n\nsecond");
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert!(!model.editor.active_is_modified());
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
            model
                .editor
                .active_editor()
                .unwrap()
                .move_cursor(CursorMovement::Right)
                .unwrap();
        }
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::TextInput('!'));
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
