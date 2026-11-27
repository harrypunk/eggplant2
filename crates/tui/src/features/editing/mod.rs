pub(crate) mod target;

use editor::{CursorMovement, InsertPosition, LineBoundary, OpenLineDirection, WordMovement};

use crate::{
    data_object::{Context, Model},
    manager::{effect::Effect, keymap_contribution::ContextKeymap},
};

pub const KEYMAPS: &[ContextKeymap] = &[
    ContextKeymap::new("editor_input", Context::EditorInput).with_commands(&[
        ("exit_insert_mode", "esc"),
        ("backspace", "backspace"),
        ("insert_newline", "enter"),
        ("move_left", "left"),
        ("move_right", "right"),
        ("move_up", "up"),
        ("move_down", "down"),
    ]),
    ContextKeymap::new("visual", Context::EditorVisual)
        .with_commands(&[("exit_visual_mode", "esc")]),
];

pub(crate) use self::target::EditingTarget;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditPolicy {
    Navigation,
    Mutation,
}

fn with_target(
    model: &mut Model,
    policy: EditPolicy,
    mut operation: impl FnMut(&mut dyn EditingTarget) -> Option<Effect>,
) -> Option<Effect> {
    target::with_active(model, |target| {
        if !target.is_editing_enabled() || (policy == EditPolicy::Mutation && !target.can_mutate())
        {
            return None;
        }
        operation(target)
    })?
}

pub fn enter_visual_mode(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, enter_visual)
}

pub fn enter_visual_line_mode(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, enter_visual_line)
}

pub fn exit_visual_mode(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, |target| {
        target.session().enter_normal_mode();
        None
    })
}

pub fn enter_insert_mode(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, enter_insert_with)
}

pub fn insert_after_cursor(model: &mut Model) -> Option<Effect> {
    insert_at(model, InsertPosition::AfterCursor)
}

pub fn insert_at_line_end(model: &mut Model) -> Option<Effect> {
    insert_at(model, InsertPosition::LineEnd)
}

pub fn insert_at_first_nonblank(model: &mut Model) -> Option<Effect> {
    insert_at(model, InsertPosition::FirstNonBlank)
}

fn insert_at(model: &mut Model, position: InsertPosition) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| {
        enter_insert_at(target, position)
    })
}

pub fn open_line_below(model: &mut Model) -> Option<Effect> {
    open_line_in(model, OpenLineDirection::Below)
}

pub fn open_line_above(model: &mut Model) -> Option<Effect> {
    open_line_in(model, OpenLineDirection::Above)
}

fn open_line_in(model: &mut Model, direction: OpenLineDirection) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| {
        open_line(target, direction)
    })
}

pub fn exit_insert_mode(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, |target| {
        target.session().enter_normal_mode();
        target.after_edit()
    })
}

pub fn move_left(model: &mut Model) -> Option<Effect> {
    move_in(model, CursorMovement::Left)
}

pub fn move_right(model: &mut Model) -> Option<Effect> {
    move_in(model, CursorMovement::Right)
}

pub fn move_up(model: &mut Model) -> Option<Effect> {
    move_in(model, CursorMovement::Up)
}

pub fn move_down(model: &mut Model) -> Option<Effect> {
    move_in(model, CursorMovement::Down)
}

fn move_in(model: &mut Model, movement: CursorMovement) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, |target| {
        move_cursor(target, movement)
    })
}

pub fn move_line_start(model: &mut Model) -> Option<Effect> {
    move_to_boundary(model, LineBoundary::Start)
}

pub fn move_line_end(model: &mut Model) -> Option<Effect> {
    move_to_boundary(model, LineBoundary::End)
}

fn move_to_boundary(model: &mut Model, boundary: LineBoundary) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, |target| {
        move_line_boundary(target, boundary)
    })
}

pub fn move_next_word_start(model: &mut Model) -> Option<Effect> {
    move_by_word(model, WordMovement::NextStart)
}

pub fn move_previous_word_start(model: &mut Model) -> Option<Effect> {
    move_by_word(model, WordMovement::PreviousStart)
}

pub fn move_next_word_end(model: &mut Model) -> Option<Effect> {
    move_by_word(model, WordMovement::NextEnd)
}

fn move_by_word(model: &mut Model, movement: WordMovement) -> Option<Effect> {
    with_target(model, EditPolicy::Navigation, |target| {
        move_word(target, movement)
    })
}

pub fn backspace(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| {
        target
            .session()
            .active_editor()
            .and_then(|mut editor| editor.backspace())
            .ok()?;
        target.after_edit()
    })
}

pub fn insert_newline(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| insert(target, "\n"))
}

pub fn undo(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| {
        let changed = target
            .session()
            .active_editor()
            .and_then(|mut editor| editor.undo())
            .unwrap_or(false);
        if changed { target.after_edit() } else { None }
    })
}

pub fn redo(model: &mut Model) -> Option<Effect> {
    with_target(model, EditPolicy::Mutation, |target| {
        let changed = target
            .session()
            .active_editor()
            .and_then(|mut editor| editor.redo())
            .unwrap_or(false);
        if changed { target.after_edit() } else { None }
    })
}

pub(crate) fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    target::with_active(model, |target| insert_char_with(target, character)).flatten()
}

fn insert_char_with(target: &mut dyn EditingTarget, character: char) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    insert(target, &character.to_string())
}

fn enter_visual(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.session().active_buffer().is_some() {
        target.session().enter_visual_mode();
    }
    None
}

fn enter_visual_line(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.session().active_buffer().is_some() {
        target.session().enter_visual_line_mode();
    }
    None
}

pub(crate) fn enter_insert(model: &mut Model) -> Option<Effect> {
    target::with_active(model, |target| enter_insert_with(target)).flatten()
}

pub(crate) fn enter_insert_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if target.can_mutate() && target.session().active_buffer().is_some() {
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
    use crate::component::text_input::TextInputEvent;
    use editor::{DocumentFactory, EditMode, EditorSession, SaveRequest};

    use super::*;
    use crate::{
        data_object::{AppPage, CommandId, Context, Message},
        manager::message_manager::MessageManager,
        test_utils::input_context,
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
        after_edits: usize,
    }

    impl EditingTarget for TestTarget {
        fn session(&mut self) -> &mut EditorSession {
            &mut self.session
        }

        fn normal_input_available(&self) -> bool {
            true
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

            assert!(enter_insert_mode(&mut model).is_none());
            assert!(insert_char(&mut model, 'X').is_none());
            assert_eq!(model.editor.active_text_snapshot().unwrap().text, "abc");
            assert_eq!(model.editor.active_revision(), revision);
            assert_eq!(model.editor.mode(), EditMode::Normal);
        }
    }

    #[test]
    fn external_buffers_reject_mutation_but_allow_navigation_and_visual_selection() {
        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let inside = workspace.path().join("main.rs");
        let outside = external.path().join("dependency.rs");
        std::fs::write(&inside, "inside").unwrap();
        std::fs::write(&outside, "outside").unwrap();
        let mut model =
            crate::startup::initial_model(Some(workspace.path().to_path_buf())).unwrap();
        open_file(&mut model, &outside);
        let revision = model.editor.active_revision();

        assert!(crate::capabilities::readonly::active_buffer_is_read_only(
            &model
        ));
        for command in [
            enter_insert_mode as fn(&mut Model) -> Option<Effect>,
            insert_after_cursor,
            insert_at_line_end,
            insert_at_first_nonblank,
            open_line_below,
            open_line_above,
            backspace,
            insert_newline,
            undo,
            redo,
        ] {
            command(&mut model);
        }
        insert_char(&mut model, 'X');
        assert_eq!(model.editor.active_revision(), revision);
        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "outside");
        assert_eq!(model.editor.mode(), EditMode::Normal);

        move_right(&mut model);
        enter_visual_mode(&mut model);
        assert_eq!(model.editor.mode(), EditMode::Visual);
        exit_visual_mode(&mut model);

        open_file(&mut model, &inside);
        assert!(!crate::capabilities::readonly::active_buffer_is_read_only(
            &model
        ));
        enter_insert_mode(&mut model);
        insert_char(&mut model, 'X');
        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "Xinside");
    }

    #[test]
    fn editing_operations_compose_without_the_application_model() {
        let mut target = TestTarget::default();
        target
            .session
            .insert_document(DocumentFactory::new().create(""));

        assert!(enter_insert_with(&mut target).is_none());
        assert!(insert_char_with(&mut target, 'b').is_none());

        assert_eq!(target.session.active_text_snapshot().unwrap().text, "b");
        assert_eq!(target.session.mode(), EditMode::Insert);
        assert_eq!(target.after_edits, 1);
    }

    #[test]
    fn mode_commands_derive_context_without_stack_entries() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        create_buffer(&mut model, "");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Insert);
        assert_eq!(input_context(&model), Some(Context::EditorInput));
        assert!(model.context_stack.is_empty());

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert_eq!(input_context(&model), Some(Context::AppNormal));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn insert_mode_requires_an_active_buffer() {
        let manager = MessageManager::default();
        let mut model = Model::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );

        assert_eq!(model.editor.mode(), EditMode::Normal);
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn editing_commands_update_text_history_and_viewport() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        create_buffer(&mut model, "a");
        manager.update(&mut model, Message::Resize(120, 20));

        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_insert_mode")),
        );
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('b')));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ab"
        );

        manager.update(&mut model, Message::InvokeCommand(CommandId("backspace")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "a"
        );
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('c')));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "ac"
        );
        assert_eq!(
            model
                .editor
                .active_snapshot()
                .unwrap()
                .cursor
                .unwrap()
                .column,
            1
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
        create_buffer(&mut model, "hello");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));
        assert_eq!(model.editor.mode(), EditMode::Visual);
        assert_eq!(input_context(&model), Some(Context::EditorVisual));
        assert!(model.context_stack.is_empty());
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
        assert!(model.context_stack.is_empty());
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn visual_word_and_counted_line_motions_extend_the_same_selection() {
        let mut model = Model::default();
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
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('2')));
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('G')));
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
            create_buffer(&mut model, text);
            let manager = MessageManager::default();
            manager.update(&mut model, Message::Resize(40, 4));

            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('x')));

            assert_eq!(model.editor.mode(), EditMode::Insert);
            assert_eq!(input_context(&model), Some(Context::EditorInput));
            assert!(model.context_stack.is_empty());
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
            create_buffer(&mut model, "first");
            let manager = MessageManager::default();
            manager.update(&mut model, Message::Resize(40, 4));

            manager.update(&mut model, Message::InvokeCommand(CommandId(command)));
            manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('x')));
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
        manager.update(&mut model, Message::completion(completion));

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
        manager.update(&mut model, Message::TextInput(TextInputEvent::Insert('!')));
        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("exit_insert_mode")),
            )
            .unwrap();
        let request = expect_save(Some(effect));
        let completion = request.perform().await;
        manager.update(&mut model, Message::completion(completion));

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello!");
        assert!(!model.editor.active_is_modified());
        assert_eq!(model.status, None);
    }
}
