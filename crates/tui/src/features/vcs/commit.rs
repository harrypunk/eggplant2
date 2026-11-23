use std::path::PathBuf;

use editor::EditorSession;

use crate::{
    data_object::{AppPage, CommandId, Context, Message, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

use super::{ChangesState, report, repository_changed, repository_target};

#[derive(Debug, Default)]
pub(crate) struct CommitState {
    pub(crate) editor: EditorSession,
    phase: CommitPhase,
    revision: u64,
    viewport_width: u16,
    viewport_height: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommitAttempt {
    repository: PathBuf,
    message: String,
    revision: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
enum CommitPhase {
    #[default]
    Editing,
    Saving(CommitAttempt),
    Committing(CommitAttempt),
}

impl CommitState {
    pub(crate) fn is_editing(&self) -> bool {
        self.phase == CommitPhase::Editing
    }

    pub(crate) fn is_waiting_for_save(&self) -> bool {
        let CommitPhase::Saving(_) = self.phase else {
            return false;
        };
        true
    }

    pub(crate) fn is_in_progress(&self) -> bool {
        let (CommitPhase::Saving(_) | CommitPhase::Committing(_)) = self.phase else {
            return false;
        };
        true
    }

    pub(crate) fn resize_active_view(&mut self, width: u16, height: u16) {
        self.viewport_width = width;
        self.viewport_height = height;
        self.editor.resize_active_view(width, height);
    }

    fn clear(&mut self) {
        self.editor = EditorSession::default();
        self.phase = CommitPhase::Editing;
    }
}

pub(super) fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("vcs_commit_submit"),
            "commit staged changes",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_commit_close"),
            "close commit page and preserve the message",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_commit_discard"),
            "discard the commit message",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub(super) fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "vcs_commit_submit" => submit(model),
        "vcs_commit_close" => close(model),
        "vcs_commit_discard" => discard(model),
        _ => None,
    }
}

pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    if !model.vcs.commit.is_editing() {
        return None;
    }
    if repository_target(model).is_none() {
        return Some(report(model, StatusMessage::error("not a Git repository")));
    }
    if staged_count(model) == 0 {
        return Some(report(model, StatusMessage::info("No staged changes")));
    }
    if model.vcs.commit.editor.is_empty() {
        let document = model.vcs.documents.create("");
        model.vcs.commit.editor.insert_document(document);
    }
    let (width, height) = (
        model.vcs.commit.viewport_width,
        model.vcs.commit.viewport_height,
    );
    model.vcs.commit.editor.resize_active_view(width, height);
    model.vcs.notice = None;
    model.active_page = AppPage::VcsCommit;
    crate::editing::enter_insert(model);
    None
}

fn close(model: &mut Model) -> Option<Effect> {
    if !model.vcs.commit.is_editing() {
        return None;
    }
    model.vcs.commit.editor.enter_normal_mode();
    return_to_status(model);
    Some(report(
        model,
        StatusMessage::info("Commit message preserved"),
    ))
}

fn discard(model: &mut Model) -> Option<Effect> {
    if !model.vcs.commit.is_editing() {
        return None;
    }
    model.vcs.commit.clear();
    return_to_status(model);
    Some(report(
        model,
        StatusMessage::info("Commit message discarded"),
    ))
}

fn return_to_status(model: &mut Model) {
    model.active_page = AppPage::Vcs;
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsCommitInput);
}

fn submit(model: &mut Model) -> Option<Effect> {
    if !model.vcs.commit.is_editing() {
        return None;
    }
    if model.deferred_action.is_some() {
        return Some(report(
            model,
            StatusMessage::info("Wait for the pending close/quit operation to finish"),
        ));
    }
    let message = model
        .vcs
        .commit
        .editor
        .active_text_snapshot()
        .map(|snapshot| snapshot.text)
        .unwrap_or_default();
    if message.trim().is_empty() {
        return Some(report(
            model,
            StatusMessage::info("Commit message must not be empty"),
        ));
    }
    if staged_count(model) == 0 {
        return Some(report(model, StatusMessage::info("No staged changes")));
    }
    let Some(target) = repository_target(model) else {
        return Some(report(model, StatusMessage::error("not a Git repository")));
    };
    model.vcs.commit.revision = model.vcs.commit.revision.wrapping_add(1);
    model.vcs.commit.phase = CommitPhase::Saving(CommitAttempt {
        repository: target,
        message,
        revision: model.vcs.commit.revision,
    });
    model.vcs.commit.editor.enter_normal_mode();
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsCommitInput);
    model.vcs.notice = Some("Saving before commit…".to_string());
    continue_after_save(model)
}

/// Continue the save barrier after each ordinary file save completes.
pub(crate) fn continue_after_save(model: &mut Model) -> Option<Effect> {
    if !model.vcs.commit.is_waiting_for_save() || model.editor.has_saving_buffers() {
        return None;
    }
    if let Some(id) = model.editor.first_dirty_file_buffer() {
        return match model.editor.save_buffer(id) {
            Ok(request) => Some(Effect::Save(request)),
            Err(error) => {
                save_failed(model, &error.to_string());
                None
            }
        };
    }
    start_commit(model)
}

pub(crate) fn save_failed(model: &mut Model, error: &str) {
    if model.vcs.commit.is_waiting_for_save() {
        model.vcs.commit.phase = CommitPhase::Editing;
        model.vcs.notice = Some(format!("Commit cancelled: {error}"));
    }
}

fn start_commit(model: &mut Model) -> Option<Effect> {
    let CommitPhase::Saving(attempt) = &model.vcs.commit.phase else {
        return None;
    };
    let attempt = attempt.clone();
    model.vcs.commit.phase = CommitPhase::Committing(attempt.clone());
    model.vcs.notice = Some("Committing staged changes…".to_string());
    Some(Effect::feature(CommitRequest {
        target: attempt.repository,
        message: attempt.message,
        revision: attempt.revision,
    }))
}

struct CommitRequest {
    target: PathBuf,
    message: String,
    revision: u64,
}

impl FeatureEffect for CommitRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::commit(&self.target, &self.message)
            .map(|id| id.to_hex_with_len(7).to_string())
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(CommitCompletion {
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct CommitCompletion {
    target: PathBuf,
    revision: u64,
    result: Result<String, String>,
}

impl Completion for CommitCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let CommitPhase::Committing(attempt) = &model.vcs.commit.phase else {
            return None;
        };
        if attempt.revision != self.revision
            || attempt.repository.as_path() != self.target.as_path()
        {
            return None;
        }
        match self.result {
            Ok(id) => {
                model.vcs.commit.clear();
                return_to_status(model);
                let status = StatusMessage::info(format!("Created commit {id}"));
                Effect::combine(repository_changed(model), Some(report(model, status)))
            }
            Err(error) => {
                model.vcs.commit.phase = CommitPhase::Editing;
                Some(report(model, StatusMessage::error(error)))
            }
        }
    }
}

pub(crate) fn staged_count(model: &Model) -> usize {
    let ChangesState::Ready(changes) = model.vcs.changes() else {
        return 0;
    };
    changes.iter().filter(|change| change.staged).count()
}

#[cfg(test)]
mod tests {
    use editor::EditMode;

    use super::*;
    use crate::{
        data_object::{DeferredAction, Workspace},
        manager::message_manager::MessageManager,
        test_utils::{create_buffer, open_file},
        vcs::HistoryState,
    };
    use ::vcs::{ChangeKind, FileChange};

    fn model(root: PathBuf) -> Model {
        let mut model = Model {
            active_page: AppPage::Vcs,
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;
        model.vcs.changes.state = ChangesState::Ready(vec![FileChange {
            kind: ChangeKind::Modified,
            staged: true,
            path: root.join("note.txt"),
            previous_path: None,
        }]);
        model
    }

    fn message(model: &Model) -> String {
        model
            .vcs
            .commit
            .editor
            .active_text_snapshot()
            .map(|snapshot| snapshot.text)
            .unwrap_or_default()
    }

    fn type_text(model: &mut Model, text: &str) {
        for character in text.chars() {
            assert!(crate::editing::insert_char(model, character).is_none());
        }
    }

    fn submit(model: &mut Model) -> Option<Effect> {
        handle_command(model, CommandId("vcs_commit_submit"))
    }

    fn finish_save(model: &mut Model, completion: editor::SaveCompletion) -> Option<Effect> {
        MessageManager::default().update(model, Message::completion(completion))
    }

    #[test]
    fn scratch_editing_preserves_and_discards_drafts_without_auto_save() {
        let mut model = model(PathBuf::from("/workspace"));
        let main_id = create_buffer(&mut model, "main buffer");
        let main_revision = model.editor.buffer_version(main_id);

        open(&mut model);
        assert_eq!(model.active_page, AppPage::VcsCommit);
        assert_eq!(model.vcs.commit.editor.mode(), EditMode::Insert);
        type_text(&mut model, "subject");
        assert_eq!(message(&model), "subject");
        assert!(
            crate::editing::handle_command(&mut model, CommandId("exit_insert_mode")).is_none()
        );
        assert_eq!(model.vcs.commit.editor.mode(), EditMode::Normal);

        assert!(close(&mut model).is_some());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert_eq!(message(&model), "subject");
        open(&mut model);
        assert_eq!(message(&model), "subject");
        assert!(discard(&mut model).is_some());
        assert!(model.vcs.commit.editor.is_empty());
        assert_eq!(model.editor.buffer_version(main_id), main_revision);
        assert_eq!(
            model.editor.active_text_snapshot().unwrap().text,
            "main buffer"
        );
    }

    #[test]
    fn discard_and_reopen_restores_the_visible_scratch_viewport() {
        let mut model = model(PathBuf::from("/workspace"));
        model.vcs.commit.resize_active_view(60, 10);
        open(&mut model);
        type_text(&mut model, "first");
        assert!(discard(&mut model).is_some());

        open(&mut model);
        type_text(&mut model, "second");

        let snapshot = model.vcs.commit.editor.active_snapshot().unwrap();
        assert!(snapshot.cursor.is_some());
        assert!(snapshot.text.lines.iter().any(|line| line.text == "second"));
    }

    #[test]
    fn lifecycle_commands_are_rejected_while_commit_is_in_progress() {
        let mut model = model(PathBuf::from("/workspace"));
        open(&mut model);
        type_text(&mut model, "subject");
        assert!(matches!(submit(&mut model), Some(Effect::Feature(_))));

        for command in [
            "quit",
            "close_buffer",
            "close_other_buffers",
            "close_buffers_right",
            "close_all_buffers",
        ] {
            let effect = MessageManager::default()
                .update(&mut model, Message::InvokeCommand(CommandId(command)));
            assert!(matches!(effect, Some(Effect::Feature(_))));
            assert_eq!(model.deferred_action, None);
            assert!(matches!(
                model.running_state,
                crate::data_object::RunningState::Running
            ));
            assert!(matches!(model.vcs.commit.phase, CommitPhase::Committing(_)));
            assert_eq!(
                model.status.as_ref().unwrap().text,
                "Commit in progress; try again after it finishes"
            );
            assert_eq!(
                model.notifications.current().unwrap().title(),
                Some("Rejected")
            );
        }
    }

    #[test]
    fn pending_lifecycle_action_rejects_commit_submission() {
        let mut model = model(PathBuf::from("/workspace"));
        model.deferred_action = Some(DeferredAction::Quit);
        open(&mut model);
        type_text(&mut model, "subject");

        assert!(submit(&mut model).is_some());

        assert!(model.vcs.commit.is_editing());
        assert_eq!(message(&model), "subject");
        assert_eq!(model.deferred_action, Some(DeferredAction::Quit));
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Wait for the pending close/quit operation to finish"
        );
    }

    #[test]
    fn empty_message_is_rejected_before_saves_or_repository_work() {
        let mut model = model(PathBuf::from("/workspace"));
        open(&mut model);
        type_text(&mut model, "  ");

        assert!(submit(&mut model).is_some());
        assert!(model.vcs.commit.is_editing());
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Commit message must not be empty"
        );
    }

    #[test]
    fn failed_commit_preserves_the_message_and_allows_retry() {
        let root = PathBuf::from("/workspace");
        let mut model = model(root.clone());
        open(&mut model);
        type_text(&mut model, "subject\n\nbody");
        assert!(matches!(submit(&mut model), Some(Effect::Feature(_))));
        let revision = model.vcs.commit.revision;

        Box::new(CommitCompletion {
            target: root,
            revision,
            result: Err("identity missing".to_string()),
        })
        .update(&mut model);

        assert!(model.vcs.commit.is_editing());
        assert_eq!(message(&model), "subject\n\nbody");
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "identity missing"
        );
        assert!(matches!(submit(&mut model), Some(Effect::Feature(_))));
    }

    #[test]
    fn stale_completion_does_not_clear_a_newer_attempt() {
        let root = PathBuf::from("/workspace");
        let mut model = model(root.clone());
        open(&mut model);
        type_text(&mut model, "first");
        assert!(submit(&mut model).is_some());
        let first_revision = model.vcs.commit.revision;
        Box::new(CommitCompletion {
            target: root.clone(),
            revision: first_revision,
            result: Err("retry".to_string()),
        })
        .update(&mut model);
        crate::editing::handle_command(&mut model, CommandId("insert_at_line_end"));
        type_text(&mut model, " second");
        assert!(submit(&mut model).is_some());

        Box::new(CommitCompletion {
            target: root,
            revision: first_revision,
            result: Ok("stale".to_string()),
        })
        .update(&mut model);

        assert_eq!(model.active_page, AppPage::VcsCommit);
        assert_eq!(message(&model), "first second");
        assert!(matches!(model.vcs.commit.phase, CommitPhase::Committing(_)));
    }

    #[test]
    fn successful_commit_clears_the_draft_and_returns_to_status() {
        let root = PathBuf::from("/workspace");
        let mut model = model(root.clone());
        open(&mut model);
        type_text(&mut model, "subject");
        assert!(submit(&mut model).is_some());

        Box::new(CommitCompletion {
            target: root,
            revision: model.vcs.commit.revision,
            result: Ok("abc1234".to_string()),
        })
        .update(&mut model);

        assert_eq!(model.active_page, AppPage::Vcs);
        assert!(model.vcs.commit.editor.is_empty());
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Created commit abc1234"
        );
        assert_eq!(model.vcs.history.state, HistoryState::Loading);
    }

    #[tokio::test]
    async fn commit_saves_a_dirty_file_before_starting() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let path = root.join("note.txt");
        std::fs::write(&path, "saved\n").unwrap();
        let mut model = model(root);
        let id = open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("dirty ")
            .unwrap();
        model.editor.enter_normal_mode();
        create_buffer(&mut model, "unsaved scratch");
        open(&mut model);
        type_text(&mut model, "subject");

        let Some(Effect::Save(request)) = submit(&mut model) else {
            panic!("a dirty file should be saved before commit");
        };
        assert!(
            MessageManager::default()
                .update(&mut model, Message::InvokeCommand(CommandId("quit")))
                .is_some()
        );
        assert!(matches!(model.vcs.commit.phase, CommitPhase::Saving(_)));
        assert_eq!(model.deferred_action, None);
        let completion = request.perform().await;
        assert_eq!(completion.buffer_id(), id);
        assert!(matches!(
            finish_save(&mut model, completion),
            Some(Effect::Feature(_))
        ));
        assert_eq!(std::fs::read_to_string(path).unwrap(), "dirty saved\n");
        assert!(matches!(model.vcs.commit.phase, CommitPhase::Committing(_)));
    }

    #[tokio::test]
    async fn commit_waits_for_an_inflight_save() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().to_path_buf();
        let path = root.join("note.txt");
        std::fs::write(&path, "saved\n").unwrap();
        let mut model = model(root);
        let id = open_file(&mut model, &path);
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("dirty ")
            .unwrap();
        model.editor.enter_normal_mode();
        let request = model.editor.save_buffer(id).unwrap();
        open(&mut model);
        type_text(&mut model, "subject");

        assert!(submit(&mut model).is_none());
        assert!(matches!(model.vcs.commit.phase, CommitPhase::Saving(_)));
        let completion = request.perform().await;
        let effect = finish_save(&mut model, completion);
        assert!(
            matches!(effect, Some(Effect::Feature(_))),
            "status: {:?}, notice: {:?}, phase: {:?}, saving: {}, dirty: {:?}",
            model.status,
            model.vcs.notice,
            model.vcs.commit.phase,
            model.editor.has_saving_buffers(),
            model.editor.is_modified(id)
        );
        assert!(matches!(model.vcs.commit.phase, CommitPhase::Committing(_)));
    }
}
