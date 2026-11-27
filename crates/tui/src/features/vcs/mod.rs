use ::vcs::FileChange;
use editor::DocumentFactory;

use crate::{
    data_object::{AppPage, CommandId, Context, Model, StatusMessage},
    manager::{
        command_registry::Command,
        effect::Effect,
        keymap_contribution::{ContextKeymap, KeymapCapability},
    },
};

pub const KEYMAPS: &[ContextKeymap] = &[
    ContextKeymap::new("vcs", Context::VCS)
        .with_commands(&[("open_vcs_change", "enter"), ("close_vcs_page", "esc")]),
    ContextKeymap::new("vcs_history", Context::VcsHistory).with_commands(&[
        ("open_vcs_commit_detail", "enter"),
        ("close_vcs_history", "esc"),
    ]),
    ContextKeymap::new("vcs_commit", Context::VcsCommit),
    ContextKeymap::new("vcs_commit_input", Context::VcsCommitInput).with_commands(&[
        ("exit_insert_mode", "esc"),
        ("backspace", "backspace"),
        ("insert_newline", "enter"),
        ("move_left", "left"),
        ("move_right", "right"),
        ("move_up", "up"),
        ("move_down", "down"),
    ]),
    ContextKeymap::new("vcs_merge", Context::VcsMerge)
        .with_commands(&[
            ("vcs_merge_submit", "enter"),
            ("vcs_merge_next", "down"),
            ("vcs_merge_previous", "up"),
            ("vcs_merge_cancel", "esc"),
        ])
        .with_capabilities(&[KeymapCapability::ListNavigation]),
    ContextKeymap::new("vcs_reset_menu", Context::VcsResetMenu)
        .with_commands(&[("vcs_reset_menu_close", "esc")]),
    ContextKeymap::new("vcs_reset", Context::VcsReset)
        .with_commands(&[
            ("vcs_reset_submit", "enter"),
            ("vcs_reset_next", "down"),
            ("vcs_reset_previous", "up"),
            ("vcs_reset_cancel", "esc"),
        ])
        .with_capabilities(&[KeymapCapability::ListNavigation]),
    ContextKeymap::new("vcs_branch_menu", Context::VcsBranchMenu)
        .with_commands(&[("vcs_branch_menu_close", "esc")]),
    ContextKeymap::new("vcs_branch", Context::VcsBranch)
        .with_commands(&[
            ("vcs_branch_submit", "enter"),
            ("vcs_branch_next", "down"),
            ("vcs_branch_previous", "up"),
            ("vcs_branch_cancel", "esc"),
        ])
        .with_capabilities(&[KeymapCapability::ListNavigation]),
];

pub(crate) mod branch;
pub(crate) mod buffer_diff;
mod change_workflow;
pub(crate) mod changes;
pub(crate) mod commit;
pub(crate) mod diff;
pub(crate) mod history;
pub(crate) mod merge;
pub(crate) mod navigation;
mod repository;
pub(crate) mod reset;

use branch::BranchState;
use buffer_diff::BufferDiffState;
use change_workflow::{
    discard_selected_change, mutate_all_changes, mutate_selected_change, open_selected_change,
    select_change, toggle_diff,
};
use changes::ChangesController;
use commit::CommitState;
use history::{
    HistoryController, close_history, copy_history_hash, load_history_more, open_history,
    open_history_detail, open_history_file, refresh_history, select_history,
    toggle_history_file_diff, toggle_history_files,
};
use merge::MergeState;
use repository::RepositoryState;
use reset::ResetState;

pub(crate) use change_workflow::{refresh_changes, sync_changes};
pub use changes::ChangesState;
pub use diff::DiffState;
pub(crate) use history::HistoryPane;
pub use history::{CommitDetailsState, HistoryMoreState, HistoryState};
pub(crate) use repository::repository_target;
pub use repository::{refresh, refresh_buffer_base, repository_changed, sync_active_buffer};

#[cfg(test)]
use change_workflow::{
    BulkOperationCompletion, ChangesCompletion, DiffCompletion, OperationCompletion, OperationKind,
};
#[cfg(test)]
use history::{
    HistoryCompletion, HistoryDetailCompletion, HistoryFileDiffCompletion, HistoryFilesCompletion,
};

#[cfg(test)]
use repository::{BufferDiffCompletion, HeadCompletion};

const HISTORY_PAGE_SIZE: usize = 20;
const DIFF_LIMIT: usize = 200;

#[derive(Debug)]
pub struct VcsState {
    repository: RepositoryState,
    pub(crate) history: HistoryController,
    pub(crate) changes: ChangesController,
    buffer_diff: BufferDiffState,
    pub(crate) notice: Option<String>,
    pub(crate) help_visible: bool,
    pub(crate) branch: BranchState,
    pub(crate) commit: CommitState,
    pub(crate) merge: MergeState,
    pub(crate) reset: ResetState,
    documents: DocumentFactory,
}

impl Default for VcsState {
    fn default() -> Self {
        Self {
            repository: RepositoryState::default(),
            history: HistoryController::default(),
            changes: ChangesController::default(),
            buffer_diff: BufferDiffState::default(),
            notice: None,
            help_visible: false,
            branch: BranchState::default(),
            commit: CommitState::default(),
            merge: MergeState::default(),
            reset: ResetState::default(),
            documents: DocumentFactory::new(),
        }
    }
}

impl VcsState {
    pub fn head(&self) -> Option<&str> {
        self.repository.head()
    }

    pub fn history(&self) -> &HistoryState {
        self.history.state()
    }

    pub fn changes(&self) -> &ChangesState {
        self.changes.state()
    }

    pub fn selected_change(&self) -> Option<&FileChange> {
        self.changes.selected_change()
    }

    pub fn documents(&self) -> &DocumentFactory {
        &self.documents
    }
}

pub(crate) fn repository_mutation_completed(model: &mut Model) -> Option<Effect> {
    let effect = repository_changed(model);
    let explorer = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
        .and_then(|root| crate::explorer::refresh_changed(model, &root));
    Effect::combine(
        Effect::combine(effect, explorer),
        crate::external_sync::reload_open_buffers(model),
    )
}

pub fn commands() -> Vec<Command> {
    let mut commands = [
        Command::feature(CommandId("open_vcs_page"), "open Git history", open_page),
        Command::feature(
            CommandId("open_vcs_history"),
            "open Git history",
            open_history,
        ),
        Command::feature(
            CommandId("close_vcs_history"),
            "close Git history",
            close_history,
        ),
        Command::feature(
            CommandId("close_vcs_history_q"),
            "close Git history",
            close_history,
        ),
        Command::feature(
            CommandId("vcs_history_next"),
            "select next commit",
            history_next,
        ),
        Command::feature(
            CommandId("vcs_history_previous"),
            "select previous commit",
            history_previous,
        ),
        Command::feature(
            CommandId("vcs_history_toggle_diff"),
            "toggle commit files or detail diff",
            toggle_history,
        ),
        Command::feature(
            CommandId("open_vcs_commit_detail"),
            "open commit detail",
            open_commit,
        ),
        Command::feature(
            CommandId("vcs_history_copy_short"),
            "copy short commit hash",
            copy_short,
        ),
        Command::feature(
            CommandId("vcs_history_copy_full"),
            "copy full commit hash",
            copy_full,
        ),
        Command::feature(
            CommandId("vcs_history_load_more"),
            "load more commits",
            load_history_more,
        ),
        Command::feature(
            CommandId("close_vcs_page"),
            "close Git page",
            close_status_page,
        ),
        Command::feature(CommandId("close_vcs_page_q"), "close Git page", close_page),
        Command::feature(
            CommandId("vcs_next_change"),
            "select next changed file",
            next_change,
        ),
        Command::feature(
            CommandId("vcs_previous_change"),
            "select previous changed file",
            previous_change,
        ),
        Command::feature(
            CommandId("open_vcs_change"),
            "open selected changed file",
            open_selected_change,
        ),
        Command::feature(CommandId("vcs_stage"), "stage changed file", stage),
        Command::feature(CommandId("vcs_unstage"), "unstage changed file", unstage),
        Command::feature(CommandId("vcs_stage_all"), "stage all changes", stage_all),
        Command::feature(
            CommandId("vcs_unstage_all"),
            "unstage all changes",
            unstage_all,
        ),
        Command::feature(
            CommandId("vcs_toggle_diff"),
            "toggle changed-file diff",
            toggle_diff,
        ),
        Command::feature(
            CommandId("vcs_discard"),
            "discard changed file",
            discard_selected_change,
        ),
        Command::feature(
            CommandId("vcs_commit_placeholder"),
            "commit staged files",
            commit::open,
        ),
        Command::feature(
            CommandId("vcs_help_placeholder"),
            "Git help",
            toggle_vcs_help,
        ),
    ]
    .into_iter()
    .collect::<Vec<_>>();
    commands.extend(commit::commands());
    commands.extend(branch::commands());
    commands.extend(merge::commands());
    commands.extend(reset::commands());
    commands
}

fn history_next(model: &mut Model) -> Option<Effect> {
    select_history(model, SelectionDirection::Next)
}

fn history_previous(model: &mut Model) -> Option<Effect> {
    select_history(model, SelectionDirection::Previous)
}

fn toggle_history(model: &mut Model) -> Option<Effect> {
    match model.vcs.history.pane {
        HistoryPane::Detail => toggle_history_file_diff(model),
        HistoryPane::List => toggle_history_files(model),
    }
}

fn open_commit(model: &mut Model) -> Option<Effect> {
    match model.vcs.history.pane {
        HistoryPane::Detail => open_history_file(model),
        HistoryPane::List => open_history_detail(model),
    }
}

fn copy_short(model: &mut Model) -> Option<Effect> {
    copy_history_hash(model, crate::copy_hash::HashFormat::Short)
}

fn copy_full(model: &mut Model) -> Option<Effect> {
    copy_history_hash(model, crate::copy_hash::HashFormat::Full)
}

fn close_status_page(model: &mut Model) -> Option<Effect> {
    if model.vcs.help_visible {
        model.vcs.help_visible = false;
        None
    } else {
        close_page(model)
    }
}

fn next_change(model: &mut Model) -> Option<Effect> {
    select_change(model, SelectionDirection::Next)
}

fn previous_change(model: &mut Model) -> Option<Effect> {
    select_change(model, SelectionDirection::Previous)
}

fn stage(model: &mut Model) -> Option<Effect> {
    mutate_selected_change(model, true)
}

fn unstage(model: &mut Model) -> Option<Effect> {
    mutate_selected_change(model, false)
}

fn stage_all(model: &mut Model) -> Option<Effect> {
    mutate_all_changes(model, true)
}

fn unstage_all(model: &mut Model) -> Option<Effect> {
    mutate_all_changes(model, false)
}

fn toggle_vcs_help(model: &mut Model) -> Option<Effect> {
    model.vcs.help_visible = !model.vcs.help_visible;
    None
}

fn open_page(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Editor {
        return None;
    }
    model.active_page = AppPage::Vcs;
    Effect::combine(
        Effect::combine(refresh(model), refresh_history(model)),
        refresh_changes(model),
    )
}

fn close_page(model: &mut Model) -> Option<Effect> {
    model.vcs.help_visible = false;
    model.active_page = AppPage::Editor;
    None
}

pub(crate) fn report(model: &mut Model, status: StatusMessage) -> Effect {
    model.vcs.notice = None;
    crate::notifications::publish_status(model, status, Some("Git"))
}

fn placeholder(model: &mut Model, message: &str) -> Option<Effect> {
    Some(report(model, StatusMessage::info(message)))
}

#[derive(Clone, Copy)]
enum SelectionDirection {
    Next,
    Previous,
}

fn wrapped_index(
    selected: Option<usize>,
    len: usize,
    direction: SelectionDirection,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let selected = selected.unwrap_or(0);
    Some(match direction {
        SelectionDirection::Next => (selected + 1) % len,
        SelectionDirection::Previous => (selected + len - 1) % len,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ::vcs::{ChangeKind, CommitDetails, CommitFile, CommitSummary, Diff, HistoryPage};

    use super::*;
    use crate::{
        data_object::{Message, Workspace},
        manager::{effect::Completion, message_manager::MessageManager},
        navigation::VerticalNavigation,
        test_utils::open_file,
    };

    #[test]
    fn refresh_coalesces_and_completion_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.repository.head = Some("old".to_string());

        assert!(refresh(&mut model).is_some());
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), Some("old"));
        let revision = model.vcs.repository.revision;

        Box::new(HeadCompletion {
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            head: Some("stale".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("old"));
        assert!(model.vcs.repository.refreshing);

        Box::new(HeadCompletion {
            target: root,
            revision,
            head: Some("main".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("main"));
        assert!(!model.vcs.repository.refreshing);

        model.workspace = None;
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), None);
        assert_eq!(model.vcs.repository.target, None);
    }

    #[test]
    fn page_open_and_close_restores_the_editor_context() {
        let mut model = Model::default();

        assert!(open_page(&mut model).is_none());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert!(model.context_stack.is_empty());
        assert_eq!(
            model.vcs.history(),
            &HistoryState::Failed("not a Git repository".to_string())
        );

        assert!(close_status_page(&mut model).is_none());
        assert_eq!(model.active_page, AppPage::Editor);
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn history_refresh_coalesces_and_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        assert!(refresh_history(&mut model).is_some());
        assert!(refresh_history(&mut model).is_none());
        let revision = model.vcs.history.revision;

        Box::new(HistoryCompletion {
            target: root.clone(),
            offset: 0,
            revision: revision.wrapping_sub(1),
            result: Ok(HistoryPage {
                commits: Vec::new(),
                has_more: false,
            }),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Loading);

        let commits = vec![CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        }];
        Box::new(HistoryCompletion {
            target: root,
            offset: 0,
            revision,
            result: Ok(HistoryPage {
                commits: commits.clone(),
                has_more: false,
            }),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Ready(commits));
    }

    #[test]
    fn history_load_more_appends_and_preserves_selection() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let commit = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit("first")]);
        model.vcs.history.selected = Some(0);
        model.vcs.history.has_more = true;

        assert!(load_history_more(&mut model).is_some());
        assert_eq!(model.vcs.history.more, HistoryMoreState::Loading);
        assert!(load_history_more(&mut model).is_none());
        let revision = model.vcs.history.revision;
        Box::new(HistoryCompletion {
            target: root.clone(),
            offset: 1,
            revision: revision.wrapping_sub(1),
            result: Ok(HistoryPage {
                commits: vec![commit("stale")],
                has_more: false,
            }),
        })
        .update(&mut model);
        Box::new(HistoryCompletion {
            target: root,
            offset: 1,
            revision,
            result: Ok(HistoryPage {
                commits: vec![commit("second")],
                has_more: false,
            }),
        })
        .update(&mut model);

        let HistoryState::Ready(commits) = model.vcs.history() else {
            panic!("history should remain ready");
        };
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[1].id, "second");
        assert_eq!(model.vcs.history.selected, Some(0));
        assert_eq!(model.vcs.history.more, HistoryMoreState::Idle);
        assert!(!model.vcs.history.has_more);
        assert!(load_history_more(&mut model).is_none());
    }

    #[test]
    fn history_load_more_can_retry_without_replacing_loaded_commits() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.history.state = HistoryState::Ready(vec![CommitSummary {
            id: "first".to_string(),
            subject: "first".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        }]);
        model.vcs.history.selected = Some(0);
        model.vcs.history.has_more = true;

        assert!(load_history_more(&mut model).is_some());
        Box::new(HistoryCompletion {
            target: root,
            offset: 1,
            revision: model.vcs.history.revision,
            result: Err("network unavailable".to_string()),
        })
        .update(&mut model);

        assert_eq!(
            model.vcs.history.more,
            HistoryMoreState::Failed("network unavailable".to_string())
        );
        assert!(matches!(model.vcs.history.state, HistoryState::Ready(_)));
        assert!(load_history_more(&mut model).is_some());
        assert_eq!(model.vcs.history.more, HistoryMoreState::Loading);
    }

    fn change(path: PathBuf, kind: ChangeKind) -> FileChange {
        FileChange {
            kind,
            staged: false,
            path,
            previous_path: None,
        }
    }

    fn complete_changes(model: &mut Model, target: PathBuf, changes: Vec<FileChange>) {
        Box::new(ChangesCompletion {
            target,
            revision: model.vcs.changes.revision,
            result: Ok(changes),
        })
        .update(model);
    }

    #[test]
    fn changes_refresh_selects_and_wraps_and_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        assert!(refresh_changes(&mut model).is_some());
        assert!(refresh_changes(&mut model).is_none());
        let revision = model.vcs.changes.revision;

        Box::new(ChangesCompletion {
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.changes(), &ChangesState::Loading);

        let changes = vec![
            change(root.join("first"), ChangeKind::Modified),
            change(root.join("second"), ChangeKind::Untracked),
        ];
        Box::new(ChangesCompletion {
            target: root,
            revision,
            result: Ok(changes.clone()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.changes(), &ChangesState::Ready(changes));
        assert_eq!(model.vcs.changes.selected, Some(0));

        select_change(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.changes.selected, Some(1));
        select_change(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.changes.selected, Some(0));
        select_change(&mut model, SelectionDirection::Previous);
        assert_eq!(model.vcs.changes.selected, Some(1));
    }

    #[test]
    fn external_status_refreshes_coalesce_with_one_trailing_request() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        assert!(sync_changes(&mut model).is_some());
        let first_revision = model.vcs.changes.revision;
        assert!(sync_changes(&mut model).is_none());
        assert!(model.vcs.changes.dirty);

        let trailing = Box::new(ChangesCompletion {
            target: root.clone(),
            revision: first_revision,
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert!(matches!(trailing, Some(Effect::Feature(_))));
        assert!(!model.vcs.changes.dirty);
        assert!(model.vcs.changes.refreshing);
        assert!(model.vcs.changes.revision > first_revision);

        Box::new(ChangesCompletion {
            target: root,
            revision: model.vcs.changes.revision,
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert!(!model.vcs.changes.refreshing);
    }

    #[test]
    fn selected_change_opens_through_the_safe_document_flow() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut model = Model {
            active_page: AppPage::Vcs,
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;
        model.vcs.changes.state =
            ChangesState::Ready(vec![change(path.clone(), ChangeKind::Modified)]);
        model.vcs.changes.selected = Some(0);

        let Some(Effect::OpenDocument(request)) = open_selected_change(&mut model) else {
            panic!("expected an open-document effect");
        };
        assert_eq!(model.active_page, AppPage::Editor);
        let manager = MessageManager::default();
        manager.update(&mut model, Message::completion(request.perform()));
        assert_eq!(
            model
                .editor
                .buffer_path(model.editor.active_buffer().unwrap()),
            Some(std::fs::canonicalize(&path).unwrap().as_path())
        );
    }

    #[test]
    fn overlapping_index_mutations_are_rejected_until_the_current_operation_completes() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes.state = ChangesState::Ready(vec![
            change(root.join("a.txt"), ChangeKind::Modified),
            change(root.join("b.txt"), ChangeKind::Modified),
        ]);
        model.vcs.changes.selected = Some(0);

        assert!(stage(&mut model).is_some());
        let revision = model.vcs.changes.operation_revision();
        assert!(model.vcs.changes.operation_in_progress());
        model.vcs.changes.selected = Some(1);

        assert!(stage(&mut model).is_some());
        assert!(stage_all(&mut model).is_some());
        assert_eq!(model.vcs.changes.operation_revision(), revision);
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Git operation in progress"
        );
        assert!(model.vcs.changes.operation_in_progress());

        let refresh = Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root,
            revision,
            result: Ok(()),
        })
        .update(&mut model);
        assert!(matches!(refresh, Some(Effect::Batch(_))));
        assert!(!model.vcs.changes.operation_in_progress());
        assert!(stage(&mut model).is_some());
    }

    #[test]
    fn stage_completion_refreshes_changes_and_stale_results_are_ignored() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes.state =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);
        model.vcs.changes.selected = Some(0);

        assert!(stage(&mut model).is_some());
        let revision = model.vcs.changes.operation_revision();
        let stale = Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            result: Ok(()),
        })
        .update(&mut model);
        assert!(stale.is_none());
        assert_eq!(model.vcs.notice, None);
        assert!(model.vcs.changes.operation_in_progress());

        let refresh = Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root.clone(),
            revision,
            result: Ok(()),
        })
        .update(&mut model);
        assert!(matches!(refresh, Some(Effect::Batch(_))));
        assert_eq!(model.vcs.notice, None);
        assert_eq!(model.notifications.current().unwrap().title(), Some("Git"));
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Staged file"
        );
        assert!(!model.vcs.changes.operation_in_progress());
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("previous status should remain visible during refresh");
        };
        assert!(!changes[0].staged, "status must not be inferred in the UI");

        let mut staged = change(root.join("note.txt"), ChangeKind::Modified);
        staged.staged = true;
        complete_changes(&mut model, root, vec![staged]);
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("expected authoritative changed-file state");
        };
        assert!(changes[0].staged);
    }

    #[test]
    fn bulk_completions_refresh_all_changes_and_preserve_selection() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let staged = |path: &str, kind| {
            let mut change = change(root.join(path), kind);
            change.staged = true;
            change
        };
        model.vcs.changes.state = ChangesState::Ready(vec![
            staged("a.txt", ChangeKind::Added),
            change(root.join("b.txt"), ChangeKind::Modified),
            change(root.join("c.txt"), ChangeKind::Untracked),
        ]);
        model.vcs.changes.selected = Some(1);

        assert!(stage_all(&mut model).is_some());
        let refresh = Box::new(BulkOperationCompletion {
            stage: true,
            target: root.clone(),
            revision: model.vcs.changes.operation_revision(),
            result: Ok(()),
        })
        .update(&mut model);
        assert!(matches!(refresh, Some(Effect::Batch(_))));
        complete_changes(
            &mut model,
            root.clone(),
            vec![
                staged("a.txt", ChangeKind::Added),
                staged("b.txt", ChangeKind::Modified),
                staged("c.txt", ChangeKind::Added),
            ],
        );
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("expected ready changes");
        };
        assert!(changes.iter().all(|change| change.staged));
        assert_eq!(changes[2].kind, ChangeKind::Added);
        assert_eq!(model.vcs.changes.selected, Some(1));
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Staged all changes"
        );

        assert!(unstage_all(&mut model).is_some());
        let refresh = Box::new(BulkOperationCompletion {
            stage: false,
            target: root.clone(),
            revision: model.vcs.changes.operation_revision(),
            result: Ok(()),
        })
        .update(&mut model);
        assert!(matches!(refresh, Some(Effect::Batch(_))));
        complete_changes(
            &mut model,
            root.clone(),
            vec![
                change(root.join("a.txt"), ChangeKind::Untracked),
                change(root.join("b.txt"), ChangeKind::Modified),
                change(root.join("c.txt"), ChangeKind::Untracked),
            ],
        );
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("expected ready changes");
        };
        assert!(changes.iter().all(|change| !change.staged));
        assert_eq!(changes[0].kind, ChangeKind::Untracked);
        assert_eq!(changes[2].kind, ChangeKind::Untracked);
        assert_eq!(model.vcs.changes.selected, Some(1));
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Unstaged all changes"
        );
    }

    #[test]
    fn failed_index_operation_releases_the_single_flight_guard() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes.state =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);
        model.vcs.changes.selected = Some(0);
        assert!(stage(&mut model).is_some());

        let effect = Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root,
            revision: model.vcs.changes.operation_revision(),
            result: Err("index locked".to_string()),
        })
        .update(&mut model);

        assert!(effect.is_some());
        assert!(!model.vcs.changes.operation_in_progress());
        assert_eq!(model.vcs.notice, None);
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "index locked"
        );
        assert!(stage(&mut model).is_some());
    }

    #[test]
    fn refresh_keeps_ready_content_visible() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes.target = Some(root.clone());
        model.vcs.changes.state =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);

        assert!(refresh_changes(&mut model).is_some());
        assert!(matches!(model.vcs.changes(), ChangesState::Ready(_)));
        assert!(model.vcs.changes.refreshing);
        assert!(refresh_changes(&mut model).is_none());

        Box::new(ChangesCompletion {
            target: root,
            revision: model.vcs.changes.revision,
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert!(!model.vcs.changes.refreshing);
    }

    #[test]
    fn tab_loads_and_closes_an_inline_diff() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let selected = change(root.join("note.txt"), ChangeKind::Modified);
        model.vcs.changes.state = ChangesState::Ready(vec![selected.clone()]);
        model.vcs.changes.selected = Some(0);

        assert!(toggle_diff(&mut model).is_some());
        let revision = model.vcs.changes.diff_revision;
        Box::new(DiffCompletion {
            change: selected.clone(),
            target: root,
            revision,
            result: Ok(Diff {
                lines: vec!["+changed".to_string()],
                truncated: false,
            }),
        })
        .update(&mut model);
        assert_eq!(
            model.vcs.changes.diff,
            DiffState::Ready {
                lines: vec!["+changed".to_string()],
                truncated: false,
            }
        );

        assert!(toggle_diff(&mut model).is_none());
        assert_eq!(model.vcs.changes.expanded, None);
        assert_eq!(model.vcs.changes.diff, DiffState::Idle);

        assert!(toggle_diff(&mut model).is_none());
        assert_eq!(
            model.vcs.changes.expanded,
            Some((selected.path.clone(), selected.staged))
        );
        assert!(matches!(model.vcs.changes.diff, DiffState::Ready { .. }));
    }

    #[tokio::test]
    async fn active_buffer_base_is_cached_and_invalidated_on_external_reload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        let revision = model.editor.buffer_version(id).unwrap();

        assert!(sync_active_buffer(&mut model).is_some());
        assert!(sync_active_buffer(&mut model).is_none());
        Box::new(BufferDiffCompletion {
            buffer_id: id,
            revision,
            generation: model.vcs.buffer_diff.generation(),
            result: Ok(Some(b"hello\n".to_vec())),
        })
        .update(&mut model);
        assert!(model.vcs.buffer_diff.is_loaded(id));
        assert!(sync_active_buffer(&mut model).is_none());

        assert!(refresh_buffer_base(&mut model, id).is_some());
        assert!(!model.vcs.buffer_diff.is_loaded(id));
        assert_eq!(model.vcs.buffer_diff.pending(), Some((id, revision)));
    }

    #[test]
    fn repository_change_invalidates_cached_bases_and_refreshes_the_active_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        model.vcs.buffer_diff.mark_loaded(id);

        assert!(repository_changed(&mut model).is_some());
        assert!(!model.vcs.buffer_diff.is_loaded(id));
        assert_eq!(model.vcs.buffer_diff.pending().map(|(id, _)| id), Some(id));
    }

    #[test]
    fn history_page_opens_closes_and_wraps_selection() {
        let mut model = Model {
            active_page: AppPage::Vcs,
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;

        assert!(open_history(&mut model).is_none());
        assert_eq!(model.active_page, AppPage::VcsHistory);
        assert!(model.context_stack.is_empty());
        assert!(close_history(&mut model).is_none());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert!(model.context_stack.is_empty());

        let commit = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit("first"), commit("second")]);
        model.vcs.history.selected = Some(0);
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history.selected, Some(1));
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history.selected, Some(0));
        select_history(&mut model, SelectionDirection::Previous);
        assert_eq!(model.vcs.history.selected, Some(1));
    }

    #[test]
    fn loading_and_failed_details_keep_commands_out_of_the_commit_list() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root)),
            ..Model::default()
        };
        let commit = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 0,
            branches: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit("first"), commit("second")]);
        model.vcs.history.selected = Some(0);
        assert!(open_history_detail(&mut model).is_some());

        for detail in [
            CommitDetailsState::Loading,
            CommitDetailsState::Failed("failed".to_string()),
        ] {
            model.vcs.history.detail = detail;
            assert!(toggle_history(&mut model).is_none());
            select_history(&mut model, SelectionDirection::Next);
            {
                let mut navigation = navigation::HistoryNavigation::new(&mut model.vcs);
                navigation.move_to_end();
            }
            assert_eq!(model.vcs.history.selected, Some(0));
            assert_eq!(model.vcs.history.expanded, None);
        }
    }

    #[test]
    fn history_tab_loads_and_reuses_changed_files() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let commit = CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit.clone()]);
        model.vcs.history.selected = Some(0);
        let details = CommitDetails {
            full_id: commit.id.clone(),
            message: "complete message".to_string(),
            author: commit.author.clone(),
            date: "2026-11-16 10:00:00 +0000".to_string(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        };

        assert!(toggle_history(&mut model).is_some());
        Box::new(HistoryFilesCompletion {
            commit_id: commit.id.clone(),
            target: root,
            revision: model.vcs.history.files_revision,
            result: Ok(details),
        })
        .update(&mut model);
        assert!(matches!(
            model.vcs.history.files,
            CommitDetailsState::Ready(_)
        ));

        assert!(toggle_history(&mut model).is_none());
        assert_eq!(model.vcs.history.expanded, None);
        assert!(toggle_history(&mut model).is_none());
        assert_eq!(model.vcs.history.expanded, Some(commit.id));
        assert!(matches!(
            model.vcs.history.files,
            CommitDetailsState::Ready(_)
        ));
    }

    #[test]
    fn enter_opens_commit_detail_and_tab_toggles_its_selected_file_diff() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let commit = CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit]);
        model.vcs.history.selected = Some(0);
        let details = CommitDetails {
            full_id: "full-commit-id".to_string(),
            message: "complete message".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:00:00 +0000".to_string(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        };

        assert!(open_commit(&mut model).is_some());
        Box::new(HistoryDetailCompletion {
            commit_id: "abc1234".to_string(),
            target: root.clone(),
            revision: model.vcs.history.detail_revision,
            result: Ok(details),
        })
        .update(&mut model);
        assert!(matches!(
            model.vcs.history.detail,
            CommitDetailsState::Ready(_)
        ));
        assert_eq!(model.vcs.history.selected_file, Some(0));

        assert!(toggle_history(&mut model).is_some());
        Box::new(HistoryFileDiffCompletion {
            commit_id: "full-commit-id".to_string(),
            file_path: "note.txt".to_string(),
            target: root,
            revision: model.vcs.history.detail_revision,
            result: Ok(Diff {
                lines: vec!["+changed".to_string()],
                truncated: false,
            }),
        })
        .update(&mut model);
        assert!(matches!(model.vcs.history.diff, DiffState::Ready { .. }));
    }

    #[test]
    fn stale_history_detail_completion_cannot_replace_a_cached_commit() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let summary = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        let details = |id: &str| CommitDetails {
            full_id: format!("full-{id}"),
            message: id.to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:00:00 +0000".to_string(),
            files: Vec::new(),
        };
        model.vcs.history.state = HistoryState::Ready(vec![summary("a"), summary("b")]);
        model.vcs.history.selected = Some(0);
        model
            .vcs
            .history
            .commit_cache
            .insert("b".to_string(), details("b"));

        assert!(open_history_detail(&mut model).is_some());
        let stale_revision = model.vcs.history.detail_revision;
        model.vcs.history.selected = Some(1);
        assert!(open_history_detail(&mut model).is_none());
        assert!(matches!(
            &model.vcs.history.detail,
            CommitDetailsState::Ready(details) if details.full_id == "full-b"
        ));

        Box::new(HistoryDetailCompletion {
            commit_id: "a".to_string(),
            target: root,
            revision: stale_revision,
            result: Ok(details("a")),
        })
        .update(&mut model);

        assert!(matches!(
            &model.vcs.history.detail,
            CommitDetailsState::Ready(details) if details.full_id == "full-b"
        ));
        assert!(!model.vcs.history.commit_cache.contains_key("a"));
        assert!(model.vcs.history.detail_revision > stale_revision);
    }

    #[test]
    fn history_detail_navigation_loads_the_selected_file_diff() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let commit = CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            timestamp: 1_800_000_000,
            branches: Vec::new(),
        };
        let file = |path: &str| CommitFile {
            kind: ChangeKind::Modified,
            path: path.to_string(),
            previous_path: None,
        };
        model.vcs.history.state = HistoryState::Ready(vec![commit.clone()]);
        model.vcs.history.selected = Some(0);
        model.vcs.history.commit_cache.insert(
            commit.id,
            CommitDetails {
                full_id: "full-commit-id".to_string(),
                message: "complete message".to_string(),
                author: "Eggplant".to_string(),
                date: "2026-11-16 10:00:00 +0000".to_string(),
                files: vec![file("first.txt"), file("second.txt")],
            },
        );

        assert!(open_commit(&mut model).is_none());
        assert_eq!(model.vcs.history.selected_file, Some(0));
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history.selected_file, Some(1));
        assert!(toggle_history(&mut model).is_some());
        Box::new(HistoryFileDiffCompletion {
            commit_id: "full-commit-id".to_string(),
            file_path: "second.txt".to_string(),
            target: root,
            revision: model.vcs.history.detail_revision,
            result: Ok(Diff {
                lines: vec!["+second".to_string()],
                truncated: false,
            }),
        })
        .update(&mut model);

        assert_eq!(
            model.vcs.history.expanded_file.as_deref(),
            Some("second.txt")
        );
        assert!(matches!(model.vcs.history.diff, DiffState::Ready { .. }));
    }

    #[test]
    fn help_toggles_and_escape_dismisses_it_without_closing_the_page() {
        let mut model = Model {
            active_page: AppPage::Vcs,
            ..Model::default()
        };
        model.active_page = AppPage::Vcs;

        assert!(toggle_vcs_help(&mut model).is_none());
        assert!(model.vcs.help_visible);
        assert!(close_status_page(&mut model).is_none());
        assert!(!model.vcs.help_visible);
        assert_eq!(model.active_page, AppPage::Vcs);

        assert!(toggle_vcs_help(&mut model).is_none());
        assert!(close_page(&mut model).is_none());
        assert_eq!(model.active_page, AppPage::Editor);
        assert!(!model.vcs.help_visible);
    }

    #[test]
    fn discard_validates_the_selection_and_uses_the_shared_operation_workflow() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let mut staged = change(root.join("note.txt"), ChangeKind::Modified);
        staged.staged = true;
        model.vcs.changes.state = ChangesState::Ready(vec![staged]);
        model.vcs.changes.selected = Some(0);
        assert!(discard_selected_change(&mut model).is_some());
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Select an unstaged change to discard"
        );
        assert!(!model.vcs.changes.operation_in_progress());

        for (kind, message) in [
            (ChangeKind::Untracked, "Cannot restore an untracked file"),
            (ChangeKind::Conflict, "Conflicted files cannot be restored"),
            (
                ChangeKind::Renamed,
                "This unstaged change cannot be restored",
            ),
        ] {
            model.vcs.changes.state =
                ChangesState::Ready(vec![change(root.join("note.txt"), kind)]);
            model.vcs.changes.selected = Some(0);
            discard_selected_change(&mut model);
            assert_eq!(model.notifications.current().unwrap().message(), message);
            assert!(!model.vcs.changes.operation_in_progress());
        }

        model.vcs.changes.state =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);
        model.vcs.changes.selected = Some(0);
        assert!(discard_selected_change(&mut model).is_some());
        assert!(model.vcs.changes.operation_in_progress());
        let completion = Box::new(OperationCompletion {
            kind: OperationKind::Discard,
            path: root.join("note.txt"),
            target: root,
            revision: model.vcs.changes.operation_revision(),
            result: Ok(()),
        });
        assert!(matches!(
            completion.update(&mut model),
            Some(Effect::Batch(_))
        ));
        assert!(!model.vcs.changes.operation_in_progress());
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Discarded unstaged changes"
        );
    }

    #[test]
    fn deleted_change_does_not_create_a_buffer() {
        let mut model = Model::default();
        model.vcs.changes.state = ChangesState::Ready(vec![change(
            PathBuf::from("/workspace/deleted.txt"),
            ChangeKind::Deleted,
        )]);
        model.vcs.changes.selected = Some(0);

        assert!(open_selected_change(&mut model).is_none());
        assert!(model.editor.is_empty());
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Cannot open a deleted file")
        );
    }
}
