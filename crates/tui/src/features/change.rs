use crate::{
    data_object::{CommandId, Context, Model, PendingInput, StatusMessage},
    editing::{self, EditingTarget, target},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ChangeState {
    pending: Option<PendingChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeAction {
    Ignored,
    Cancelled,
    DeleteLine,
    DeleteWord,
    ChangeLine,
    ChangeWord,
}

impl ChangeState {
    fn begin(&mut self, pending: PendingChange) -> char {
        self.pending = Some(pending);
        pending.key()
    }

    fn update(&mut self, character: char) -> ChangeAction {
        let Some(pending) = self.pending.take() else {
            return ChangeAction::Ignored;
        };
        match (pending, character) {
            (PendingChange::Delete, 'd') => ChangeAction::DeleteLine,
            (PendingChange::Delete, 'w') => ChangeAction::DeleteWord,
            (PendingChange::Change, 'c') => ChangeAction::ChangeLine,
            (PendingChange::Change, 'w') => ChangeAction::ChangeWord,
            _ => ChangeAction::Cancelled,
        }
    }

    fn clear(&mut self) {
        self.pending = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingChange {
    Delete,
    Change,
}

impl PendingChange {
    fn key(self) -> char {
        match self {
            Self::Delete => 'd',
            Self::Change => 'c',
        }
    }
}

#[derive(Clone, Copy)]
enum SelectionOperation {
    Delete,
    Change,
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("delete_grapheme"),
            "delete grapheme under cursor",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_selection_x"),
            "delete selection",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_selection_d"),
            "delete selection",
            handle_command,
        ),
        Command::feature(
            CommandId("change_selection"),
            "change selection",
            handle_command,
        ),
        Command::feature(
            CommandId("delete_line_pending"),
            "delete current line",
            handle_command,
        ),
        Command::feature(
            CommandId("change_line_pending"),
            "change current line",
            handle_command,
        ),
        Command::feature(
            CommandId("change_cancel"),
            "cancel pending delete/change",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::EditorChange, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "delete_grapheme" => return delete_grapheme(model),
        "delete_selection_x" | "delete_selection_d" => {
            return delete_selection(model, SelectionOperation::Delete);
        }
        "change_selection" => return delete_selection(model, SelectionOperation::Change),
        "delete_line_pending" => start(model, PendingChange::Delete),
        "change_line_pending" => start(model, PendingChange::Change),
        "change_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown change command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    match model.change.update(character) {
        ChangeAction::DeleteLine => return delete_line(model),
        ChangeAction::DeleteWord => return delete_word(model),
        ChangeAction::ChangeLine => return change_line(model),
        ChangeAction::ChangeWord => return change_word(model),
        ChangeAction::Cancelled => reset(model),
        ChangeAction::Ignored => {}
    }
    None
}

fn start(model: &mut Model, pending: PendingChange) {
    // Normal d/c are prefixes; Visual d/c use direct selection commands instead.
    let Some((normal_context, can_mutate)) = target::with_active(model, |target| {
        (target.normal_context(), target.can_mutate())
    }) else {
        return;
    };
    if !can_mutate || model.context_stack.last() != Some(&normal_context) {
        return;
    }
    let pending = model.change.begin(pending);
    model.pending_input = Some(PendingInput::new(
        Context::EditorChange,
        pending.to_string(),
    ));
    model.context_stack.push(Context::EditorChange);
}

fn reset(model: &mut Model) {
    model.change.clear();
    PendingInput::clear(&mut model.pending_input, Context::EditorChange);
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorChange);
}

fn delete_grapheme(model: &mut Model) -> Option<Effect> {
    target::with_active(model, |target| delete_grapheme_with(target)).flatten()
}

fn delete_grapheme_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.cursor_grapheme_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.delete_grapheme_forward();
    }
    Effect::combine(register, target.after_edit())
}

fn delete_selection(model: &mut Model, operation: SelectionOperation) -> Option<Effect> {
    target::with_active(model, |target| delete_selection_with(target, operation)).flatten()
}

fn delete_selection_with(
    target: &mut dyn EditingTarget,
    operation: SelectionOperation,
) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.selected_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        match operation {
            SelectionOperation::Delete => {
                let _ = editor.delete_selection();
            }
            SelectionOperation::Change => {
                let _ = editor.change_selection();
            }
        }
    }
    let context = target.visual_context();
    target.pop_context(context);
    match operation {
        SelectionOperation::Delete => {
            target.session().enter_normal_mode();
            Effect::combine(register, target.after_edit())
        }
        SelectionOperation::Change => {
            editing::enter_insert_with(target);
            register
        }
    }
}

fn delete_line(model: &mut Model) -> Option<Effect> {
    reset(model);
    target::with_active(model, |target| delete_line_with(target)).flatten()
}

fn delete_line_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.current_line_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.delete_current_line();
    }
    Effect::combine(register, target.after_edit())
}

fn change_line(model: &mut Model) -> Option<Effect> {
    reset(model);
    target::with_active(model, |target| change_line_with(target)).flatten()
}

fn change_line_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.current_line_change_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.change_current_line();
    }
    editing::enter_insert_with(target);
    register
}

fn delete_word(model: &mut Model) -> Option<Effect> {
    reset(model);
    target::with_active(model, |target| delete_word_with(target)).flatten()
}

fn delete_word_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.current_word_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.delete_word();
    }
    Effect::combine(register, target.after_edit())
}

fn change_word(model: &mut Model) -> Option<Effect> {
    reset(model);
    target::with_active(model, |target| change_word_with(target)).flatten()
}

fn change_word_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.can_mutate() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.current_word_text(),
        Err(_) => return None,
    };
    let register = target.record_deleted(text);
    if let Ok(mut editor) = target.session().active_editor() {
        let _ = editor.change_word();
    }
    editing::enter_insert_with(target);
    register
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{AppPage, Message, Workspace},
        manager::message_manager::MessageManager,
        test_utils::{create_buffer, open_file},
        vcs::ChangesState,
    };
    use ::vcs::{ChangeKind, FileChange};

    #[test]
    fn external_buffers_reject_delete_and_change_without_starting_a_prefix() {
        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let path = external.path().join("dependency.rs");
        std::fs::write(&path, "outside").unwrap();
        let mut model =
            crate::startup::initial_model(Some(workspace.path().to_path_buf())).unwrap();
        model.context_stack.push(Context::AppNormal);
        open_file(&mut model, &path);
        let revision = model.editor.active_revision();

        for command in [
            "delete_grapheme",
            "delete_line_pending",
            "change_line_pending",
        ] {
            handle_command(&mut model, CommandId(command));
        }

        assert_eq!(model.editor.active_revision(), revision);
        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "outside");
        assert!(!model.context_stack.contains(&Context::EditorChange));
    }

    fn commit_model(text: &str) -> Model {
        let root = std::path::PathBuf::from("/workspace");
        let mut model = Model {
            active_page: AppPage::Vcs,
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.context_stack.push(Context::VCS);
        model.vcs.changes.state = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: root.join("note.txt"),
            previous_path: None,
        }]);
        crate::vcs::commit::open(&mut model);
        for character in text.chars() {
            editing::insert_char(&mut model, character);
        }
        editing::handle_command(&mut model, CommandId("exit_insert_mode"));
        model
    }

    fn commit_text(model: &Model) -> String {
        model.vcs.commit.editor.active_text_snapshot().unwrap().text
    }

    #[test]
    fn state_parses_operator_actions_without_a_model() {
        let mut state = ChangeState::default();
        assert_eq!(state.update('d'), ChangeAction::Ignored);

        for (pending, prompt, input, expected) in [
            (PendingChange::Delete, 'd', 'd', ChangeAction::DeleteLine),
            (PendingChange::Delete, 'd', 'w', ChangeAction::DeleteWord),
            (PendingChange::Change, 'c', 'c', ChangeAction::ChangeLine),
            (PendingChange::Change, 'c', 'w', ChangeAction::ChangeWord),
        ] {
            assert_eq!(state.begin(pending), prompt);
            assert_eq!(state.update(input), expected);
            assert_eq!(state.pending, None);
        }

        state.begin(PendingChange::Delete);
        assert_eq!(state.update('x'), ChangeAction::Cancelled);
        state.begin(PendingChange::Change);
        state.clear();
        assert_eq!(state.pending, None);
    }

    #[test]
    fn commit_editor_reuses_grapheme_word_and_line_deletion() {
        let manager = MessageManager::default();
        let mut model = commit_model("hello");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_start")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_grapheme")),
        );
        assert_eq!(commit_text(&model), "ello");
        assert!(model.editor.is_empty());
        assert_eq!(model.register.unnamed(), "");

        let mut model = commit_model("hello world");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_start")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('w'));
        assert_eq!(commit_text(&model), "world");
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::VcsCommit));
        assert_eq!(model.register.unnamed(), "");

        let mut model = commit_model("first");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('d'));
        assert_eq!(commit_text(&model), "");
        assert!(model.editor.is_empty());
        assert_eq!(model.register.unnamed(), "");
    }

    #[test]
    fn pending_dd_and_cc_use_status_and_cancel_cleanly() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("d")
        );
        manager.update(&mut model, Message::TextInput('d'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "second"
        );
        assert_eq!(model.pending_input, None);
        assert_eq!(model.register.unnamed(), "first\n");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_line_pending")),
        );
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("c")
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_cancel")),
        );
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('c'));
        assert_eq!(model.editor.mode(), editor::EditMode::Insert);
        assert_eq!(model.register.unnamed(), "second");
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            ""
        );

        assert_eq!(model.pending_input, None);
    }

    #[test]
    fn dw_and_cw_capture_the_word_and_preserve_undo_behavior() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "héllo world");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('w'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "world"
        );
        assert_eq!(model.register.unnamed(), "héllo ");
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert_eq!(model.pending_input, None);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));

        manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('w'));
        assert_eq!(model.editor.mode(), editor::EditMode::Insert);
        assert_eq!(model.register.unnamed(), "héllo ");
        manager.update(&mut model, Message::TextInput('!'));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "!world"
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "héllo world"
        );
    }

    #[test]
    fn visual_delete_and_change_use_the_primary_selection() {
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
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_selection_d")),
        );

        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "llo"
        );
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert_eq!(model.register.unnamed(), "he");
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
    }

    #[test]
    fn visual_change_enters_insert_and_undoes_as_one_edit() {
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
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("change_selection")),
        );
        manager.update(&mut model, Message::TextInput('X'));
        assert_eq!(model.editor.mode(), editor::EditMode::Insert);
        assert_eq!(model.register.unnamed(), "he");
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "Xllo"
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("exit_insert_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("undo")));

        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "hello"
        );
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
    }
}
