pub(crate) mod target;

use editor::{CursorMovement, InsertPosition, LineBoundary, OpenLineDirection, WordMovement};

use crate::{
    data_object::{CommandId, Model, StatusMessage},
    manager::effect::Effect,
};

pub(crate) use self::target::EditingTarget;

/// Dispatch physical-page-independent editing commands through the active adapter.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    let result = target::with_active(model, |target| handle_command_with(target, id))?;
    match result {
        Ok(effect) => effect,
        Err(id) => {
            model.status = Some(StatusMessage::error(format!(
                "unknown editing command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

fn handle_command_with(
    target: &mut dyn EditingTarget,
    id: CommandId,
) -> Result<Option<Effect>, CommandId> {
    if !target.is_editing_enabled() {
        return Ok(None);
    }
    let effect = match id.as_str() {
        "enter_visual_mode" => enter_visual(target),
        "enter_visual_line_mode" => enter_visual_line(target),
        "exit_visual_mode" => {
            let context = target.visual_context();
            target.pop_context(context);
            target.session().enter_normal_mode();
            None
        }
        "enter_insert_mode" => enter_insert_with(target),
        "insert_after_cursor" => enter_insert_at(target, InsertPosition::AfterCursor),
        "insert_at_line_end" => enter_insert_at(target, InsertPosition::LineEnd),
        "insert_at_first_nonblank" => enter_insert_at(target, InsertPosition::FirstNonBlank),
        "open_line_below" => open_line(target, OpenLineDirection::Below),
        "open_line_above" => open_line(target, OpenLineDirection::Above),
        "exit_insert_mode" => {
            let context = target.input_context();
            target.pop_context(context);
            target.session().enter_normal_mode();
            target.after_edit()
        }
        "move_left" => move_cursor(target, CursorMovement::Left),
        "move_right" => move_cursor(target, CursorMovement::Right),
        "move_up" => move_cursor(target, CursorMovement::Up),
        "move_down" => move_cursor(target, CursorMovement::Down),
        "move_line_start" => move_line_boundary(target, LineBoundary::Start),
        "move_line_end" => move_line_boundary(target, LineBoundary::End),
        "move_next_word_start" => move_word(target, WordMovement::NextStart),
        "move_previous_word_start" => move_word(target, WordMovement::PreviousStart),
        "move_next_word_end" => move_word(target, WordMovement::NextEnd),
        "backspace" => {
            if target
                .session()
                .active_editor()
                .and_then(|mut editor| editor.backspace())
                .is_ok()
            {
                return Ok(target.after_edit());
            }
            None
        }
        "insert_newline" => insert(target, "\n"),
        "undo" => {
            if target
                .session()
                .active_editor()
                .and_then(|mut editor| editor.undo())
                .unwrap_or(false)
            {
                return Ok(target.after_edit());
            }
            None
        }
        "redo" => {
            if target
                .session()
                .active_editor()
                .and_then(|mut editor| editor.redo())
                .unwrap_or(false)
            {
                return Ok(target.after_edit());
            }
            None
        }
        _ => return Err(id),
    };
    Ok(effect)
}

pub(crate) fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    target::with_active(model, |target| insert_char_with(target, character)).flatten()
}

fn insert_char_with(target: &mut dyn EditingTarget, character: char) -> Option<Effect> {
    if !target.is_editing_enabled() {
        return None;
    }
    insert(target, &character.to_string())
}

fn enter_visual(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.session().active_buffer().is_some() {
        let context = target.visual_context();
        target.push_context(context);
        target.session().enter_visual_mode();
    }
    None
}

fn enter_visual_line(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.session().active_buffer().is_some() {
        let context = target.visual_context();
        target.push_context(context);
        target.session().enter_visual_line_mode();
    }
    None
}

pub(crate) fn enter_insert(model: &mut Model) -> Option<Effect> {
    target::with_active(model, |target| enter_insert_with(target)).flatten()
}

pub(crate) fn enter_insert_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.session().active_buffer().is_some() {
        let context = target.input_context();
        target.push_context(context);
        target.session().enter_insert_mode();
    }
    None
}

fn enter_insert_at(target: &mut dyn EditingTarget, position: InsertPosition) -> Option<Effect> {
    if target
        .session()
        .active_editor()
        .and_then(|mut editor| editor.move_cursor_to_insert(position))
        .is_ok()
    {
        enter_insert_with(target);
    }
    None
}

fn open_line(target: &mut dyn EditingTarget, direction: OpenLineDirection) -> Option<Effect> {
    if target
        .session()
        .active_editor()
        .and_then(|mut editor| editor.open_line(direction))
        .is_ok()
    {
        enter_insert_with(target);
        return target.after_edit();
    }
    None
}

fn move_cursor(target: &mut dyn EditingTarget, movement: CursorMovement) -> Option<Effect> {
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.move_cursor(movement);
    }
    None
}

fn move_line_boundary(target: &mut dyn EditingTarget, boundary: LineBoundary) -> Option<Effect> {
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.move_cursor_to_line_boundary(boundary);
    }
    None
}

fn move_word(target: &mut dyn EditingTarget, movement: WordMovement) -> Option<Effect> {
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.move_cursor_word(movement);
    }
    None
}

fn insert(target: &mut dyn EditingTarget, text: &str) -> Option<Effect> {
    if target
        .session()
        .active_editor()
        .and_then(|mut editor| editor.insert_text(text))
        .is_ok()
    {
        return target.after_edit();
    }
    None
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditMode, EditorSession, SaveRequest};

    use super::*;
    use crate::{
        data_object::{AppPage, Context, Message},
        manager::message_manager::MessageManager,
        test_utils::{create_buffer, open_file},
    };

    fn expect_save(effect: Option<Effect>) -> SaveRequest {
        let Some(Effect::Save(request)) = effect else {
            panic!("expected a save effect");
        };
        request
    }

    #[derive(Default)]
    struct TestTarget {
        session: EditorSession,
        contexts: Vec<Context>,
        after_edits: usize,
    }

    impl EditingTarget for TestTarget {
        fn session(&mut self) -> &mut EditorSession {
            &mut self.session
        }

        fn input_context(&self) -> Context {
            Context::EditorInput
        }

        fn push_context(&mut self, context: Context) {
            self.contexts.push(context);
        }

        fn pop_context(&mut self, context: Context) {
            self.contexts.pop_if(|active| *active == context);
        }

        fn after_edit(&mut self) -> Option<Effect> {
            self.after_edits += 1;
            None
        }
    }

    #[test]
    fn non_editing_pages_do_not_compose_an_editing_target() {
        for page in [AppPage::Vcs, AppPage::VcsHistory] {
            let mut model = Model::default();
            create_buffer(&mut model, "abc");
            model.active_page = page;
            let revision = model.editor.active_revision();

            assert!(handle_command(&mut model, CommandId("enter_insert_mode")).is_none());
            assert!(insert_char(&mut model, 'X').is_none());
            assert_eq!(model.editor.active_text_snapshot().unwrap().text, "abc");
            assert_eq!(model.editor.active_revision(), revision);
            assert_eq!(model.editor.mode(), EditMode::Normal);
        }
    }

    #[test]
    fn editing_operations_compose_without_the_application_model() {
        let mut target = TestTarget::default();
        target
            .session
            .insert_document(DocumentFactory::new().create(""));

        assert!(
            handle_command_with(&mut target, CommandId("enter_insert_mode"))
                .unwrap()
                .is_none()
        );
        assert!(insert_char_with(&mut target, 'b').is_none());

        assert_eq!(target.session.active_text_snapshot().unwrap().text, "b");
        assert_eq!(target.contexts, [Context::EditorInput]);
        assert_eq!(target.after_edits, 1);
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
        assert_eq!(model.status, None);
    }
}
