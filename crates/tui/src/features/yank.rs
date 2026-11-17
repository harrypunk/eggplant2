use editor::{BufferId, PastePosition};
use helix_view::clipboard::{ClipboardProvider, ClipboardType};

use crate::{
    data_object::{CommandId, Context, Message, Model, PendingInput, StatusMessage},
    editing::{EditingTarget, target},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct YankState {
    pending: bool,
    pending_paste: Option<PendingPaste>,
    paste_generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum YankPrefixAction {
    Ignored,
    Cancelled,
    YankLine,
}

impl YankState {
    fn begin_prefix(&mut self) {
        self.pending = true;
    }

    fn finish_prefix(&mut self, character: char) -> YankPrefixAction {
        if !self.pending {
            return YankPrefixAction::Ignored;
        }
        self.pending = false;
        if character == 'y' {
            YankPrefixAction::YankLine
        } else {
            YankPrefixAction::Cancelled
        }
    }

    fn clear_prefix(&mut self) {
        self.pending = false;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PasteTarget {
    Main { buffer_id: BufferId, revision: i32 },
    Commit { buffer_id: BufferId, revision: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingPaste {
    target: PasteTarget,
    generation: u64,
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("yank_selection"),
            "yank selection",
            handle_command,
        ),
        Command::feature(
            CommandId("yank_line_pending"),
            "yank current line",
            handle_command,
        ),
        Command::feature(
            CommandId("paste_after"),
            "paste after cursor",
            handle_command,
        ),
        Command::feature(
            CommandId("paste_before"),
            "paste before cursor",
            handle_command,
        ),
        Command::feature(
            CommandId("yank_cancel"),
            "cancel pending yank",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::EditorYank, handle_input)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "yank_selection" => return yank_selection(model),
        "yank_line_pending" => start(model),
        "paste_after" => return paste(model, PastePosition::After),
        "paste_before" => return paste(model, PastePosition::Before),
        "yank_cancel" => reset(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown yank command `{}`",
                id.as_str()
            )));
        }
    }
    None
}

pub fn handle_input(model: &mut Model, character: char) -> Option<Effect> {
    match model.yank.finish_prefix(character) {
        YankPrefixAction::YankLine => return yank_current_line(model),
        YankPrefixAction::Cancelled => reset(model),
        YankPrefixAction::Ignored => {}
    }
    None
}

fn yank_selection(model: &mut Model) -> Option<Effect> {
    target::with_active(model, |target| yank_selection_with(target)).flatten()
}

fn yank_selection_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.is_editing_enabled() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.selected_text(),
        Err(_) => return None,
    };
    let context = target.visual_context();
    target.pop_context(context);
    target.session().enter_normal_mode();
    target.record_yanked(text)
}

fn yank_current_line(model: &mut Model) -> Option<Effect> {
    reset(model);
    target::with_active(model, |target| yank_current_line_with(target)).flatten()
}

fn yank_current_line_with(target: &mut dyn EditingTarget) -> Option<Effect> {
    if !target.is_editing_enabled() {
        return None;
    }
    let text = match target.session().active_editor() {
        Ok(editor) => editor.current_line_text(),
        Err(_) => return None,
    };
    target.record_yanked(text)
}

fn paste(model: &mut Model, position: PastePosition) -> Option<Effect> {
    if !paste_enabled(model) {
        return None;
    }
    if !model.register.has_system_provider() {
        return paste_direct(model, position);
    }
    let target = current_paste_target(model)?;
    model.yank.paste_generation = model.yank.paste_generation.wrapping_add(1);
    let generation = model.yank.paste_generation;
    model.yank.pending_paste = Some(PendingPaste { target, generation });
    Some(Effect::feature(PasteRequest {
        provider: model.register.provider(),
        position,
        target,
        generation,
    }))
}

fn paste_direct(model: &mut Model, position: PastePosition) -> Option<Effect> {
    target::with_active(model, |target| paste_with(target, position)).flatten()
}

fn paste_enabled(model: &mut Model) -> bool {
    target::with_active(model, |target| target.is_editing_enabled()).unwrap_or(false)
}

fn current_paste_target(model: &mut Model) -> Option<PasteTarget> {
    let kind = target::active_kind(model)?;
    let (buffer_id, revision) =
        target::with_active(model, |target| target.session().active_revision()).flatten()?;
    Some(match kind {
        target::EditingTargetKind::Commit => PasteTarget::Commit {
            buffer_id,
            revision,
        },
        target::EditingTargetKind::Main => PasteTarget::Main {
            buffer_id,
            revision,
        },
    })
}

fn target_is_current(model: &mut Model, target: PasteTarget) -> bool {
    let (kind, buffer_id, revision) = match target {
        PasteTarget::Main {
            buffer_id,
            revision,
        } => (target::EditingTargetKind::Main, buffer_id, revision),
        PasteTarget::Commit {
            buffer_id,
            revision,
        } => (target::EditingTargetKind::Commit, buffer_id, revision),
    };
    if target::active_kind(model) != Some(kind) {
        return false;
    }
    target::with_active(model, |target| {
        target.is_editing_enabled()
            && target.session().active_revision() == Some((buffer_id, revision))
    })
    .unwrap_or(false)
}

struct PasteRequest {
    provider: ClipboardProvider,
    position: PastePosition,
    target: PasteTarget,
    generation: u64,
}

impl FeatureEffect for PasteRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = self
            .provider
            .get_contents(&ClipboardType::Clipboard)
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(PasteCompletion {
            position: self.position,
            target: self.target,
            generation: self.generation,
            result,
        }))
    }
}

struct PasteCompletion {
    position: PastePosition,
    target: PasteTarget,
    generation: u64,
    result: Result<String, String>,
}

impl Completion for PasteCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.yank.pending_paste
            != Some(PendingPaste {
                target: self.target,
                generation: self.generation,
            })
        {
            return None;
        }
        model.yank.pending_paste = None;
        let text = match self.result {
            Ok(text) => text,
            Err(error) => {
                model.status = Some(StatusMessage::error(format!(
                    "System clipboard unavailable: {error}"
                )));
                return None;
            }
        };
        if !target_is_current(model, self.target) {
            return None;
        }
        model.register.update_from_clipboard(text);
        paste_direct(model, self.position)
    }
}

fn paste_with(target: &mut dyn EditingTarget, position: PastePosition) -> Option<Effect> {
    if !target.is_editing_enabled() {
        return None;
    }
    let text = target.register_text();
    if text.is_empty() {
        return None;
    }
    target
        .session()
        .active_editor()
        .and_then(|mut editor| editor.paste_text(&text, position))
        .ok()?;
    target.after_edit()
}

fn start(model: &mut Model) {
    // Normal y waits for the second y; Visual y is a direct selection command.
    let Some(normal_context) = target::with_active(model, |target| target.normal_context()) else {
        return;
    };
    if model.context_stack.last() != Some(&normal_context) {
        return;
    }
    model.yank.begin_prefix();
    model.pending_input = Some(PendingInput::new(Context::EditorYank, "y"));
    model.context_stack.push(Context::EditorYank);
}

fn reset(model: &mut Model) {
    model.yank.clear_prefix();
    PendingInput::clear(&mut model.pending_input, Context::EditorYank);
    model
        .context_stack
        .pop_if(|context| context == &Context::EditorYank);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{AppPage, Message, Workspace},
        manager::message_manager::MessageManager,
        test_utils::create_buffer,
        vcs::ChangesState,
    };
    use ::vcs::{ChangeKind, FileChange};

    fn commit_model(text: &str) -> Model {
        let root = std::path::PathBuf::from("/workspace");
        let mut model = Model {
            active_page: AppPage::Vcs,
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.context_stack.push(Context::VCS);
        model.register.set_provider(ClipboardProvider::None);
        model.vcs.changes.state = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: root.join("note.txt"),
            previous_path: None,
        }]);
        crate::vcs::commit::open(&mut model);
        for character in text.chars() {
            crate::editing::insert_char(&mut model, character);
        }
        crate::editing::handle_command(&mut model, CommandId("exit_insert_mode"));
        model
    }

    fn commit_text(model: &Model) -> String {
        model.vcs.commit.editor.active_text_snapshot().unwrap().text
    }

    #[test]
    fn state_parses_yank_prefix_without_a_model() {
        let mut state = YankState::default();
        assert_eq!(state.finish_prefix('y'), YankPrefixAction::Ignored);

        state.begin_prefix();
        assert_eq!(state.finish_prefix('y'), YankPrefixAction::YankLine);
        state.begin_prefix();
        assert_eq!(state.finish_prefix('x'), YankPrefixAction::Cancelled);
        state.begin_prefix();
        state.clear_prefix();
        assert_eq!(state.finish_prefix('y'), YankPrefixAction::Ignored);
    }

    #[test]
    fn cancel_preserves_another_workflows_pending_input() {
        let mut model = Model {
            pending_input: Some(PendingInput::new(Context::LineJump, "2")),
            ..Model::default()
        };

        reset(&mut model);

        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("2")
        );
        assert!(
            model
                .pending_input
                .as_ref()
                .is_some_and(|pending| pending.is_owner(Context::LineJump))
        );
    }

    #[test]
    fn non_editing_pages_do_not_compose_paste_targets() {
        for page in [AppPage::Vcs, AppPage::VcsHistory] {
            let mut model = Model::default();
            create_buffer(&mut model, "ab");
            model.active_page = page;
            model.register.set_provider(ClipboardProvider::None);
            model.register.record("X".to_string());
            let revision = model.editor.active_revision();

            assert!(paste(&mut model, PastePosition::After).is_none());
            assert!(current_paste_target(&mut model).is_none());
            assert_eq!(model.editor.active_text_snapshot().unwrap().text, "ab");
            assert_eq!(model.editor.active_revision(), revision);
        }
    }

    #[test]
    fn paste_after_and_before_use_the_current_register() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "ab");
        model.register.set_provider(ClipboardProvider::None);
        model.register.record("X".to_string());
        let manager = MessageManager::default();

        manager.update(&mut model, Message::InvokeCommand(CommandId("paste_after")));
        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "aXb");
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_start")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("paste_before")),
        );
        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "XaXb");
    }

    #[test]
    fn clipboard_completion_updates_register_and_pastes_into_the_original_target() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "ab");
        let target = current_paste_target(&mut model).unwrap();
        model.yank.pending_paste = Some(PendingPaste {
            target,
            generation: 1,
        });

        Box::new(PasteCompletion {
            position: PastePosition::After,
            target,
            generation: 1,
            result: Ok("X".to_string()),
        })
        .update(&mut model);

        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "aXb");
        assert_eq!(model.register.unnamed(), "X");
        assert_eq!(model.yank.pending_paste, None);
    }

    #[test]
    fn clipboard_completion_is_ignored_after_the_target_revision_changes() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "ab");
        let target = current_paste_target(&mut model).unwrap();
        model.yank.pending_paste = Some(PendingPaste {
            target,
            generation: 1,
        });
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("!")
            .unwrap();

        Box::new(PasteCompletion {
            position: PastePosition::After,
            target,
            generation: 1,
            result: Ok("stale".to_string()),
        })
        .update(&mut model);

        assert_eq!(model.editor.active_text_snapshot().unwrap().text, "!ab");
        assert_eq!(model.register.unnamed(), "");
    }

    #[test]
    fn commit_yank_and_paste_share_the_register_without_delete_overwrite() {
        let mut model = commit_model("subject");
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("move_line_start")),
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('y'));
        assert_eq!(model.register.unnamed(), "subject");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("delete_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('d'));
        assert_eq!(commit_text(&model), "");
        assert_eq!(model.register.unnamed(), "subject");

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("paste_before")),
        );
        assert_eq!(commit_text(&model), "subject");
        assert!(model.editor.is_empty());
    }

    #[test]
    fn visual_yank_updates_register_without_changing_text() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nworld");
        let manager = MessageManager::default();
        manager.update(&mut model, Message::Resize(80, 10));
        let revision = model.editor.active_revision();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_right")));

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_selection")),
        );

        assert_eq!(model.register.unnamed(), "he");
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert_eq!(
            model.editor.active_snapshot().unwrap().text.lines[0].text,
            "hello"
        );
        assert_eq!(model.editor.active_revision(), revision);
        assert!(effect.is_some());
    }

    #[test]
    fn yy_yanks_line_and_escape_cancels_pending_yank() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "hello\nworld");
        let manager = MessageManager::default();

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_line_pending")),
        );
        assert_eq!(
            model.pending_input.as_ref().map(PendingInput::text),
            Some("y")
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("yank_cancel")));
        assert_eq!(model.pending_input, None);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_line_pending")),
        );
        manager.update(&mut model, Message::TextInput('y'));
        assert_eq!(model.register.unnamed(), "hello\n");
        assert_eq!(model.pending_input, None);
        assert!(!model.editor.active_is_modified());
    }

    #[test]
    fn linewise_visual_yank_includes_selected_line_endings() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        create_buffer(&mut model, "first\nsecond\nthird");
        let manager = MessageManager::default();
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("enter_visual_line_mode")),
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("move_down")));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("yank_selection")),
        );

        assert_eq!(model.register.unnamed(), "first\nsecond\n");
        assert_eq!(model.editor.mode(), editor::EditMode::Normal);
        assert!(!model.editor.active_is_modified());
    }
}
