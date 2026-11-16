use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use ::vcs::{ChangeKind, CommitDetails, CommitSummary, Diff, FileChange, HistoryPage};
use editor::{BufferId, DocumentFactory};

use crate::{
    data_object::{AppPage, CommandId, Context, Message, Model, StatusMessage},
    document,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

pub(crate) mod commit;

use commit::CommitState;

const HISTORY_PAGE_SIZE: usize = 20;
const DIFF_LIMIT: usize = 200;

#[derive(Debug)]
pub struct VcsState {
    head: Option<String>,
    target: Option<PathBuf>,
    revision: u64,
    refreshing: bool,
    pub(crate) history: HistoryState,
    pub(crate) history_has_more: bool,
    pub(crate) history_more: HistoryMoreState,
    pub(crate) history_selected: Option<usize>,
    pub(crate) history_expanded: Option<String>,
    pub(crate) history_files: CommitDetailsState,
    history_files_target: Option<PathBuf>,
    history_files_revision: u64,
    history_commit_cache: HashMap<String, CommitDetails>,
    pub(crate) history_detail: CommitDetailsState,
    pub(crate) history_detail_selected_file: Option<usize>,
    pub(crate) history_detail_expanded_file: Option<String>,
    pub(crate) history_detail_diff: DiffState,
    history_detail_target: Option<PathBuf>,
    history_detail_revision: u64,
    history_detail_diff_cache: HashMap<(String, String), Diff>,
    history_target: Option<PathBuf>,
    history_revision: u64,
    pub(crate) changes: ChangesState,
    pub(crate) selected_change: Option<usize>,
    changes_target: Option<PathBuf>,
    changes_revision: u64,
    changes_refreshing: bool,
    operation_target: Option<PathBuf>,
    operation_revision: u64,
    pub(crate) expanded_change: Option<(PathBuf, bool)>,
    pub(crate) diff: DiffState,
    diff_target: Option<PathBuf>,
    diff_revision: u64,
    diff_cache: HashMap<(PathBuf, bool), Diff>,
    diff_base_loaded: HashSet<BufferId>,
    diff_base_pending: Option<(BufferId, i32)>,
    diff_base_generation: u64,
    pub(crate) notice: Option<String>,
    pub(crate) commit: CommitState,
    documents: DocumentFactory,
}

impl Default for VcsState {
    fn default() -> Self {
        Self {
            head: None,
            target: None,
            revision: 0,
            refreshing: false,
            history: HistoryState::Idle,
            history_has_more: false,
            history_more: HistoryMoreState::Idle,
            history_selected: None,
            history_expanded: None,
            history_files: CommitDetailsState::Idle,
            history_files_target: None,
            history_files_revision: 0,
            history_commit_cache: HashMap::new(),
            history_detail: CommitDetailsState::Idle,
            history_detail_selected_file: None,
            history_detail_expanded_file: None,
            history_detail_diff: DiffState::Idle,
            history_detail_target: None,
            history_detail_revision: 0,
            history_detail_diff_cache: HashMap::new(),
            history_target: None,
            history_revision: 0,
            changes: ChangesState::Idle,
            selected_change: None,
            changes_target: None,
            changes_revision: 0,
            changes_refreshing: false,
            operation_target: None,
            operation_revision: 0,
            expanded_change: None,
            diff: DiffState::Idle,
            diff_target: None,
            diff_revision: 0,
            diff_cache: HashMap::new(),
            diff_base_loaded: HashSet::new(),
            diff_base_pending: None,
            diff_base_generation: 0,
            notice: None,
            commit: CommitState::default(),
            documents: DocumentFactory::new(),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum HistoryState {
    #[default]
    Idle,
    Loading,
    Ready(Vec<CommitSummary>),
    Failed(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum HistoryMoreState {
    #[default]
    Idle,
    Loading,
    Failed(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ChangesState {
    #[default]
    Idle,
    Loading,
    Ready(Vec<FileChange>),
    Failed(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum DiffState {
    #[default]
    Idle,
    Loading,
    Ready {
        lines: Vec<String>,
        truncated: bool,
    },
    Failed(String),
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum CommitDetailsState {
    #[default]
    Idle,
    Loading,
    Ready(CommitDetails),
    Failed(String),
}

impl VcsState {
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    pub fn history(&self) -> &HistoryState {
        &self.history
    }

    pub fn changes(&self) -> &ChangesState {
        &self.changes
    }

    pub fn selected_change(&self) -> Option<&FileChange> {
        let ChangesState::Ready(changes) = &self.changes else {
            return None;
        };
        changes.get(self.selected_change?)
    }

    pub fn documents(&self) -> &DocumentFactory {
        &self.documents
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("open_vcs_page"),
            "open Git history",
            handle_command,
        ),
        Command::feature(
            CommandId("open_vcs_history"),
            "open Git history",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_history"),
            "close Git history",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_history_q"),
            "close Git history",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_history_next"),
            "select next commit",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_history_previous"),
            "select previous commit",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_history_toggle_diff"),
            "toggle commit files or detail diff",
            handle_command,
        ),
        Command::feature(
            CommandId("open_vcs_commit_detail"),
            "open commit detail",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_history_load_more"),
            "load more commits",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_page"),
            "close Git page",
            handle_command,
        ),
        Command::feature(
            CommandId("close_vcs_page_q"),
            "close Git page",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_next_change"),
            "select next changed file",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_previous_change"),
            "select previous changed file",
            handle_command,
        ),
        Command::feature(
            CommandId("open_vcs_change"),
            "open selected changed file",
            handle_command,
        ),
        Command::feature(CommandId("vcs_stage"), "stage changed file", handle_command),
        Command::feature(
            CommandId("vcs_unstage"),
            "unstage changed file",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_toggle_diff"),
            "toggle changed-file diff",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_discard_placeholder"),
            "discard changed file",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_commit_placeholder"),
            "commit staged files",
            handle_command,
        ),
        Command::feature(
            CommandId("vcs_help_placeholder"),
            "Git help",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))?;
    commit::register_commands(commands)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "open_vcs_page" => open_page(model),
        "open_vcs_history" => open_history(model),
        "close_vcs_history" | "close_vcs_history_q" => close_history(model),
        "vcs_history_next" => select_history(model, SelectionDirection::Next),
        "vcs_history_previous" => select_history(model, SelectionDirection::Previous),
        "vcs_history_toggle_diff" => match model.vcs.history_detail {
            CommitDetailsState::Ready(_) => toggle_history_file_diff(model),
            _ => toggle_history_files(model),
        },
        "open_vcs_commit_detail" => open_history_detail(model),
        "vcs_history_load_more" => load_history_more(model),
        "close_vcs_page" | "close_vcs_page_q" => close_page(model),
        "vcs_next_change" => select_change(model, SelectionDirection::Next),
        "vcs_previous_change" => select_change(model, SelectionDirection::Previous),
        "open_vcs_change" => open_selected_change(model),
        "vcs_stage" => mutate_selected_change(model, true),
        "vcs_unstage" => mutate_selected_change(model, false),
        "vcs_toggle_diff" => toggle_diff(model),
        "vcs_discard_placeholder" => placeholder(model, "Discard is not implemented"),
        "vcs_commit_placeholder" => commit::open(model),
        "vcs_help_placeholder" => placeholder(model, "Help is not implemented"),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown VCS command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

fn open_page(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Editor {
        return None;
    }
    model.active_page = AppPage::Vcs;
    model.context_stack.push(Context::VCS);
    Effect::combine(
        Effect::combine(refresh(model), refresh_history(model)),
        refresh_changes(model),
    )
}

fn close_page(model: &mut Model) -> Option<Effect> {
    model.active_page = AppPage::Editor;
    model
        .context_stack
        .pop_if(|context| context == &Context::VCS);
    None
}

fn open_history(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Vcs {
        return None;
    }
    model.active_page = AppPage::VcsHistory;
    model.context_stack.push(Context::VcsHistory);
    refresh_history(model)
}

fn close_history(model: &mut Model) -> Option<Effect> {
    if model.vcs.history_detail != CommitDetailsState::Idle {
        model.vcs.history_detail = CommitDetailsState::Idle;
        model.vcs.history_detail_selected_file = None;
        model.vcs.history_detail_expanded_file = None;
        model.vcs.history_detail_diff = DiffState::Idle;
        model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
        return None;
    }
    model.active_page = AppPage::Vcs;
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsHistory);
    None
}

#[derive(Clone, Copy)]
enum SelectionDirection {
    Next,
    Previous,
}

fn select_history(model: &mut Model, direction: SelectionDirection) -> Option<Effect> {
    if let CommitDetailsState::Ready(details) = &model.vcs.history_detail {
        model.vcs.history_detail_selected_file = wrapped_index(
            model.vcs.history_detail_selected_file,
            details.files.len(),
            direction,
        );
        return None;
    }
    let HistoryState::Ready(commits) = &model.vcs.history else {
        return None;
    };
    model.vcs.history_selected =
        wrapped_index(model.vcs.history_selected, commits.len(), direction);
    None
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

fn load_history_more(model: &mut Model) -> Option<Effect> {
    let HistoryState::Ready(commits) = &model.vcs.history else {
        return None;
    };
    if !model.vcs.history_has_more || model.vcs.history_more == HistoryMoreState::Loading {
        return None;
    }
    let offset = commits.len();
    let target = repository_target(model)?;
    model.vcs.history_target = Some(target.clone());
    model.vcs.history_more = HistoryMoreState::Loading;
    model.vcs.history_revision = model.vcs.history_revision.wrapping_add(1);
    Some(Effect::feature(HistoryRequest {
        target,
        offset,
        revision: model.vcs.history_revision,
    }))
}

fn toggle_history_files(model: &mut Model) -> Option<Effect> {
    let commit = selected_history_commit(model)?;
    let commit_id = commit.id;
    if model.vcs.history_expanded.as_ref() == Some(&commit_id) {
        model.vcs.history_expanded = None;
        model.vcs.history_files = CommitDetailsState::Idle;
        model.vcs.history_files_revision = model.vcs.history_files_revision.wrapping_add(1);
        return None;
    }
    if let Some(details) = model.vcs.history_commit_cache.get(&commit_id).cloned() {
        model.vcs.history_expanded = Some(commit_id);
        model.vcs.history_files = CommitDetailsState::Ready(details);
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.history_expanded = Some(commit_id.clone());
    model.vcs.history_files = CommitDetailsState::Loading;
    model.vcs.history_files_target = Some(target.clone());
    model.vcs.history_files_revision = model.vcs.history_files_revision.wrapping_add(1);
    Some(Effect::feature(HistoryFilesRequest {
        commit_id,
        target,
        revision: model.vcs.history_files_revision,
    }))
}

fn open_history_detail(model: &mut Model) -> Option<Effect> {
    let commit = selected_history_commit(model)?;
    if matches!(model.vcs.history_detail, CommitDetailsState::Ready(_)) {
        return None;
    }
    model.vcs.history_detail_selected_file = None;
    model.vcs.history_detail_expanded_file = None;
    model.vcs.history_detail_diff = DiffState::Idle;
    if let Some(details) = model.vcs.history_commit_cache.get(&commit.id).cloned() {
        model.vcs.history_detail_selected_file = (!details.files.is_empty()).then_some(0);
        model.vcs.history_detail = CommitDetailsState::Ready(details);
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.history_detail = CommitDetailsState::Loading;
    model.vcs.history_detail_target = Some(target.clone());
    model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
    Some(Effect::feature(HistoryDetailRequest {
        commit_id: commit.id,
        target,
        revision: model.vcs.history_detail_revision,
    }))
}

fn toggle_history_file_diff(model: &mut Model) -> Option<Effect> {
    let CommitDetailsState::Ready(details) = &model.vcs.history_detail else {
        return None;
    };
    let file = details
        .files
        .get(model.vcs.history_detail_selected_file?)?
        .clone();
    let commit_id = details.full_id.clone();
    let file_path = file.path.clone();
    if model.vcs.history_detail_expanded_file.as_ref() == Some(&file_path) {
        model.vcs.history_detail_expanded_file = None;
        model.vcs.history_detail_diff = DiffState::Idle;
        model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
        return None;
    }
    let cache_key = (commit_id.clone(), file_path.clone());
    if let Some(diff) = model.vcs.history_detail_diff_cache.get(&cache_key).cloned() {
        model.vcs.history_detail_expanded_file = Some(file_path);
        model.vcs.history_detail_diff = DiffState::Ready {
            lines: diff.lines,
            truncated: diff.truncated,
        };
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.history_detail_expanded_file = Some(file_path.clone());
    model.vcs.history_detail_diff = DiffState::Loading;
    model.vcs.history_detail_target = Some(target.clone());
    model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
    Some(Effect::feature(HistoryFileDiffRequest {
        commit_id,
        file_path,
        target,
        revision: model.vcs.history_detail_revision,
    }))
}

fn selected_history_commit(model: &Model) -> Option<CommitSummary> {
    let HistoryState::Ready(commits) = &model.vcs.history else {
        return None;
    };
    commits.get(model.vcs.history_selected?).cloned()
}

fn select_change(model: &mut Model, direction: SelectionDirection) -> Option<Effect> {
    let ChangesState::Ready(changes) = &model.vcs.changes else {
        return None;
    };
    model.vcs.selected_change = wrapped_index(model.vcs.selected_change, changes.len(), direction);
    None
}

fn open_selected_change(model: &mut Model) -> Option<Effect> {
    let change = model.vcs.selected_change()?.clone();
    if change.kind == ChangeKind::Deleted {
        model.status = Some(StatusMessage::info("Cannot open a deleted file"));
        return None;
    }
    let documents = model.vcs.documents().clone();
    close_page(model);
    document::open_file(&documents, model, &change.path)
}

fn toggle_diff(model: &mut Model) -> Option<Effect> {
    let change = model.vcs.selected_change()?.clone();
    let expanded = (change.path.clone(), change.staged);
    if model.vcs.expanded_change.as_ref() == Some(&expanded) {
        model.vcs.expanded_change = None;
        model.vcs.diff = DiffState::Idle;
        model.vcs.diff_revision = model.vcs.diff_revision.wrapping_add(1);
        return None;
    }
    if let Some(diff) = model.vcs.diff_cache.get(&expanded).cloned() {
        model.vcs.expanded_change = Some(expanded);
        model.vcs.diff = DiffState::Ready {
            lines: diff.lines,
            truncated: diff.truncated,
        };
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.expanded_change = Some(expanded);
    model.vcs.diff = DiffState::Loading;
    model.vcs.diff_target = Some(target.clone());
    model.vcs.diff_revision = model.vcs.diff_revision.wrapping_add(1);
    Some(Effect::feature(DiffRequest {
        change,
        target,
        revision: model.vcs.diff_revision,
    }))
}

fn placeholder(model: &mut Model, message: &str) -> Option<Effect> {
    model.vcs.notice = Some(message.to_string());
    None
}

fn mutate_selected_change(model: &mut Model, stage: bool) -> Option<Effect> {
    let change = model.vcs.selected_change()?.clone();
    if stage == change.staged {
        let message = if stage {
            "File is already staged"
        } else {
            "File is not staged"
        };
        return placeholder(model, message);
    }
    if model.editor.has_modified_buffers() || model.editor.has_saving_buffers() {
        return placeholder(model, "Wait for pending saves before changing the index");
    }
    let target = repository_target(model)?;
    model.vcs.changes_revision = model.vcs.changes_revision.wrapping_add(1);
    model.vcs.changes_refreshing = false;
    model.vcs.operation_target = Some(target.clone());
    model.vcs.operation_revision = model.vcs.operation_revision.wrapping_add(1);
    model.vcs.notice = None;
    Some(Effect::feature(OperationRequest {
        kind: if stage {
            OperationKind::Stage
        } else {
            OperationKind::Unstage
        },
        path: change.path,
        target,
        revision: model.vcs.operation_revision,
    }))
}

/// Load the active buffer's HEAD base once, without blocking the editor.
pub fn sync_active_buffer(model: &mut Model) -> Option<Effect> {
    let (buffer_id, revision) = model.editor.active_revision()?;
    if model.vcs.diff_base_loaded.contains(&buffer_id)
        || model.vcs.diff_base_pending == Some((buffer_id, revision))
    {
        return None;
    }
    let path = model.editor.buffer_path(buffer_id)?.to_path_buf();
    model.vcs.diff_base_pending = Some((buffer_id, revision));
    model.vcs.diff_base_generation = model.vcs.diff_base_generation.wrapping_add(1);
    Some(Effect::feature(BufferDiffRequest {
        buffer_id,
        revision,
        generation: model.vcs.diff_base_generation,
        path,
    }))
}

/// Invalidate one externally reloaded buffer and refresh it if active.
pub fn refresh_buffer_base(model: &mut Model, buffer_id: BufferId) -> Option<Effect> {
    model.vcs.diff_base_loaded.remove(&buffer_id);
    if model
        .vcs
        .diff_base_pending
        .is_some_and(|(id, _)| id == buffer_id)
    {
        model.vcs.diff_base_pending = None;
    }
    if model.editor.active_buffer() == Some(buffer_id) {
        return sync_active_buffer(model);
    }
    None
}

/// Invalidate Git metadata and schedule fresh HEAD, status, and active-buffer state.
pub fn repository_changed(model: &mut Model) -> Option<Effect> {
    model.vcs.diff_base_loaded.clear();
    model.vcs.diff_base_pending = None;
    model.vcs.refreshing = false;
    model.vcs.changes_refreshing = false;
    if model.vcs.history == HistoryState::Loading {
        model.vcs.history = HistoryState::Idle;
    }
    Effect::combine(
        Effect::combine(refresh(model), refresh_history(model)),
        Effect::combine(refresh_changes(model), sync_active_buffer(model)),
    )
}

/// Start one background HEAD lookup, preserving the previous label until completion.
pub fn refresh(model: &mut Model) -> Option<Effect> {
    let target = repository_target(model);
    let Some(target) = target else {
        model.vcs.target = None;
        model.vcs.head = None;
        model.vcs.refreshing = false;
        model.vcs.revision = model.vcs.revision.wrapping_add(1);
        return None;
    };
    if model.vcs.target.as_ref() == Some(&target) && model.vcs.refreshing {
        return None;
    }

    model.vcs.target = Some(target.clone());
    model.vcs.refreshing = true;
    model.vcs.revision = model.vcs.revision.wrapping_add(1);
    Some(Effect::feature(HeadRequest {
        target,
        revision: model.vcs.revision,
    }))
}

fn repository_target(model: &Model) -> Option<PathBuf> {
    model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_path_buf())
        .or_else(|| {
            model
                .editor
                .active_buffer()
                .and_then(|id| model.editor.buffer_path(id))
                .map(PathBuf::from)
        })
}

fn refresh_history(model: &mut Model) -> Option<Effect> {
    model.vcs.history_has_more = false;
    model.vcs.history_more = HistoryMoreState::Idle;
    model.vcs.history_expanded = None;
    model.vcs.history_files = CommitDetailsState::Idle;
    model.vcs.history_files_revision = model.vcs.history_files_revision.wrapping_add(1);
    model.vcs.history_commit_cache.clear();
    model.vcs.history_detail = CommitDetailsState::Idle;
    model.vcs.history_detail_selected_file = None;
    model.vcs.history_detail_expanded_file = None;
    model.vcs.history_detail_diff = DiffState::Idle;
    model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
    model.vcs.history_detail_diff_cache.clear();
    let Some(target) = repository_target(model) else {
        model.vcs.history_target = None;
        model.vcs.history = HistoryState::Failed("not a Git repository".to_string());
        model.vcs.history_revision = model.vcs.history_revision.wrapping_add(1);
        return None;
    };
    if model.vcs.history_target.as_ref() == Some(&target)
        && model.vcs.history == HistoryState::Loading
    {
        return None;
    }

    model.vcs.history_target = Some(target.clone());
    model.vcs.history = HistoryState::Loading;
    model.vcs.history_revision = model.vcs.history_revision.wrapping_add(1);
    Some(Effect::feature(HistoryRequest {
        target,
        offset: 0,
        revision: model.vcs.history_revision,
    }))
}

fn refresh_changes(model: &mut Model) -> Option<Effect> {
    model.vcs.expanded_change = None;
    model.vcs.diff = DiffState::Idle;
    model.vcs.diff_cache.clear();
    model.vcs.diff_revision = model.vcs.diff_revision.wrapping_add(1);
    let Some(target) = repository_target(model) else {
        model.vcs.changes_target = None;
        model.vcs.changes = ChangesState::Failed("not a Git repository".to_string());
        model.vcs.selected_change = None;
        model.vcs.changes_refreshing = false;
        model.vcs.changes_revision = model.vcs.changes_revision.wrapping_add(1);
        return None;
    };
    if model.vcs.changes_target.as_ref() == Some(&target) && model.vcs.changes_refreshing {
        return None;
    }

    model.vcs.changes_target = Some(target.clone());
    model.vcs.changes_refreshing = true;
    if !matches!(model.vcs.changes, ChangesState::Ready(_)) {
        model.vcs.changes = ChangesState::Loading;
    }
    model.vcs.changes_revision = model.vcs.changes_revision.wrapping_add(1);
    Some(Effect::feature(ChangesRequest {
        target,
        revision: model.vcs.changes_revision,
    }))
}

struct DiffRequest {
    change: FileChange,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for DiffRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::diff(&self.change, DIFF_LIMIT).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(DiffCompletion {
            change: self.change,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct DiffCompletion {
    change: FileChange,
    target: PathBuf,
    revision: u64,
    result: Result<Diff, String>,
}

impl Completion for DiffCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let expanded = (self.change.path.clone(), self.change.staged);
        if model.vcs.diff_revision != self.revision
            || model.vcs.diff_target.as_ref() != Some(&self.target)
            || model.vcs.expanded_change.as_ref() != Some(&expanded)
        {
            return None;
        }
        model.vcs.diff = match self.result {
            Ok(diff) => {
                model.vcs.diff_cache.insert(expanded, diff.clone());
                DiffState::Ready {
                    lines: diff.lines,
                    truncated: diff.truncated,
                }
            }
            Err(error) => DiffState::Failed(error),
        };
        None
    }
}

#[derive(Clone, Copy)]
enum OperationKind {
    Stage,
    Unstage,
}

struct OperationRequest {
    kind: OperationKind,
    path: PathBuf,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for OperationRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = match self.kind {
            OperationKind::Stage => ::vcs::stage(&self.path),
            OperationKind::Unstage => ::vcs::unstage(&self.path),
        }
        .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(OperationCompletion {
            kind: self.kind,
            path: self.path,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct OperationCompletion {
    kind: OperationKind,
    path: PathBuf,
    target: PathBuf,
    revision: u64,
    result: Result<(), String>,
}

impl Completion for OperationCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.operation_revision != self.revision
            || model.vcs.operation_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        match self.result {
            Ok(()) => {
                model.vcs.notice = Some(match self.kind {
                    OperationKind::Stage => "Staged file".to_string(),
                    OperationKind::Unstage => "Unstaged file".to_string(),
                });
                if apply_operation_to_changes(model, self.kind, &self.path) {
                    None
                } else {
                    refresh_changes(model)
                }
            }
            Err(error) => {
                model.vcs.notice = Some(error);
                None
            }
        }
    }
}

fn apply_operation_to_changes(
    model: &mut Model,
    kind: OperationKind,
    path: &std::path::Path,
) -> bool {
    let ChangesState::Ready(changes) = &mut model.vcs.changes else {
        return false;
    };
    let staged = matches!(kind, OperationKind::Stage);
    let Some(mut change) = changes
        .iter()
        .find(|change| change.path == *path && change.staged != staged)
        .cloned()
    else {
        return false;
    };
    change.staged = staged;
    change.kind = match (kind, change.kind) {
        (OperationKind::Stage, ChangeKind::Untracked) => ChangeKind::Added,
        (OperationKind::Unstage, ChangeKind::Added) => ChangeKind::Untracked,
        (_, kind) => kind,
    };
    changes.retain(|candidate| candidate.path != *path);
    changes.push(change);
    changes.sort_by(|left, right| {
        right
            .staged
            .cmp(&left.staged)
            .then_with(|| left.path.cmp(&right.path))
    });
    model.vcs.selected_change = changes
        .iter()
        .position(|candidate| candidate.path == *path && candidate.staged == staged);
    model.vcs.expanded_change = None;
    model.vcs.diff = DiffState::Idle;
    model.vcs.diff_cache.clear();
    model.vcs.diff_revision = model.vcs.diff_revision.wrapping_add(1);
    true
}

struct ChangesRequest {
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for ChangesRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::changes(&self.target).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(ChangesCompletion {
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct ChangesCompletion {
    target: PathBuf,
    revision: u64,
    result: Result<Vec<FileChange>, String>,
}

impl Completion for ChangesCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.changes_revision != self.revision
            || model.vcs.changes_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        model.vcs.changes_refreshing = false;
        model.vcs.changes = match self.result {
            Ok(changes) => {
                model.vcs.selected_change = (!changes.is_empty()).then_some(0);
                ChangesState::Ready(changes)
            }
            Err(error) => {
                model.vcs.selected_change = None;
                ChangesState::Failed(error)
            }
        };
        None
    }
}

struct HistoryRequest {
    target: PathBuf,
    offset: usize,
    revision: u64,
}

impl FeatureEffect for HistoryRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::history(&self.target, self.offset, HISTORY_PAGE_SIZE)
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryCompletion {
            target: self.target,
            offset: self.offset,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryCompletion {
    target: PathBuf,
    offset: usize,
    revision: u64,
    result: Result<HistoryPage, String>,
}

impl Completion for HistoryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_revision != self.revision
            || model.vcs.history_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        if self.offset == 0 {
            model.vcs.history_more = HistoryMoreState::Idle;
            model.vcs.history = match self.result {
                Ok(page) => {
                    model.vcs.history_selected = (!page.commits.is_empty()).then_some(0);
                    model.vcs.history_has_more = page.has_more;
                    HistoryState::Ready(page.commits)
                }
                Err(error) => {
                    model.vcs.history_selected = None;
                    model.vcs.history_has_more = false;
                    HistoryState::Failed(error)
                }
            };
            return None;
        }
        let HistoryState::Ready(commits) = &mut model.vcs.history else {
            return None;
        };
        match self.result {
            Ok(page) => {
                commits.extend(page.commits);
                model.vcs.history_has_more = page.has_more;
                model.vcs.history_more = HistoryMoreState::Idle;
            }
            Err(error) => model.vcs.history_more = HistoryMoreState::Failed(error),
        }
        None
    }
}

struct HistoryFilesRequest {
    commit_id: String,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HistoryFilesRequest {
    fn perform(self: Box<Self>) -> Message {
        let result =
            ::vcs::commit_details(&self.target, &self.commit_id).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryFilesCompletion {
            commit_id: self.commit_id,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryFilesCompletion {
    commit_id: String,
    target: PathBuf,
    revision: u64,
    result: Result<CommitDetails, String>,
}

impl Completion for HistoryFilesCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_files_revision != self.revision
            || model.vcs.history_files_target.as_ref() != Some(&self.target)
            || model.vcs.history_expanded.as_ref() != Some(&self.commit_id)
        {
            return None;
        }
        model.vcs.history_files = match self.result {
            Ok(details) => {
                model
                    .vcs
                    .history_commit_cache
                    .insert(self.commit_id, details.clone());
                CommitDetailsState::Ready(details)
            }
            Err(error) => CommitDetailsState::Failed(error),
        };
        None
    }
}

struct HistoryDetailRequest {
    commit_id: String,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HistoryDetailRequest {
    fn perform(self: Box<Self>) -> Message {
        let result =
            ::vcs::commit_details(&self.target, &self.commit_id).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryDetailCompletion {
            commit_id: self.commit_id,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryDetailCompletion {
    commit_id: String,
    target: PathBuf,
    revision: u64,
    result: Result<CommitDetails, String>,
}

impl Completion for HistoryDetailCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_detail_revision != self.revision
            || model.vcs.history_detail_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        model.vcs.history_detail = match self.result {
            Ok(details) => {
                model.vcs.history_detail_selected_file = (!details.files.is_empty()).then_some(0);
                model
                    .vcs
                    .history_commit_cache
                    .insert(self.commit_id, details.clone());
                CommitDetailsState::Ready(details)
            }
            Err(error) => CommitDetailsState::Failed(error),
        };
        None
    }
}

struct HistoryFileDiffRequest {
    commit_id: String,
    file_path: String,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HistoryFileDiffRequest {
    fn perform(self: Box<Self>) -> Message {
        let result =
            ::vcs::commit_file_diff(&self.target, &self.commit_id, &self.file_path, DIFF_LIMIT)
                .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryFileDiffCompletion {
            commit_id: self.commit_id,
            file_path: self.file_path,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryFileDiffCompletion {
    commit_id: String,
    file_path: String,
    target: PathBuf,
    revision: u64,
    result: Result<Diff, String>,
}

impl Completion for HistoryFileDiffCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_detail_revision != self.revision
            || model.vcs.history_detail_target.as_ref() != Some(&self.target)
            || model.vcs.history_detail_expanded_file.as_ref() != Some(&self.file_path)
        {
            return None;
        }
        model.vcs.history_detail_diff = match self.result {
            Ok(diff) => {
                model
                    .vcs
                    .history_detail_diff_cache
                    .insert((self.commit_id, self.file_path), diff.clone());
                DiffState::Ready {
                    lines: diff.lines,
                    truncated: diff.truncated,
                }
            }
            Err(error) => DiffState::Failed(error),
        };
        None
    }
}

struct BufferDiffRequest {
    buffer_id: BufferId,
    revision: i32,
    generation: u64,
    path: PathBuf,
}

impl FeatureEffect for BufferDiffRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::diff_base(&self.path).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(BufferDiffCompletion {
            buffer_id: self.buffer_id,
            revision: self.revision,
            generation: self.generation,
            result,
        }))
    }
}

struct BufferDiffCompletion {
    buffer_id: BufferId,
    revision: i32,
    generation: u64,
    result: Result<Option<Vec<u8>>, String>,
}

impl Completion for BufferDiffCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.diff_base_pending != Some((self.buffer_id, self.revision))
            || model.vcs.diff_base_generation != self.generation
        {
            return None;
        }
        model.vcs.diff_base_pending = None;
        let Ok(base) = self.result else {
            return None;
        };
        if let Ok(true) = model.editor.set_buffer_diff_base(
            self.buffer_id,
            self.revision,
            base.unwrap_or_default(),
        ) {
            model.vcs.diff_base_loaded.insert(self.buffer_id);
        }
        None
    }
}

struct HeadRequest {
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for HeadRequest {
    fn perform(self: Box<Self>) -> Message {
        let head = ::vcs::current_head(&self.target);
        Message::FeatureCompletion(FeatureCompletion::new(HeadCompletion {
            target: self.target,
            revision: self.revision,
            head,
        }))
    }
}

struct HeadCompletion {
    target: PathBuf,
    revision: u64,
    head: Option<String>,
}

impl Completion for HeadCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.revision != self.revision || model.vcs.target.as_ref() != Some(&self.target) {
            return None;
        }
        model.vcs.refreshing = false;
        model.vcs.head = self.head;
        None
    }
}

#[cfg(test)]
mod tests {
    use ::vcs::CommitFile;

    use super::*;
    use crate::{
        data_object::{Message, Workspace},
        manager::message_manager::MessageManager,
        test_utils::open_file,
    };

    #[test]
    fn refresh_coalesces_and_completion_rejects_stale_results() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.head = Some("old".to_string());

        assert!(refresh(&mut model).is_some());
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), Some("old"));
        let revision = model.vcs.revision;

        Box::new(HeadCompletion {
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            head: Some("stale".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("old"));
        assert!(model.vcs.refreshing);

        Box::new(HeadCompletion {
            target: root,
            revision,
            head: Some("main".to_string()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.head(), Some("main"));
        assert!(!model.vcs.refreshing);

        model.workspace = None;
        assert!(refresh(&mut model).is_none());
        assert_eq!(model.vcs.head(), None);
        assert_eq!(model.vcs.target, None);
    }

    #[test]
    fn page_open_and_close_restores_the_editor_context() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);

        assert!(handle_command(&mut model, CommandId("open_vcs_page")).is_none());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert_eq!(model.context_stack.last(), Some(&Context::VCS));
        assert_eq!(
            model.vcs.history(),
            &HistoryState::Failed("not a Git repository".to_string())
        );

        assert!(handle_command(&mut model, CommandId("close_vcs_page")).is_none());
        assert_eq!(model.active_page, AppPage::Editor);
        assert_eq!(model.context_stack.last(), Some(&Context::AppNormal));
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
        let revision = model.vcs.history_revision;

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
            date: "2026-11-16".to_string(),
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
            date: "2026-11-16".to_string(),
        };
        model.vcs.history = HistoryState::Ready(vec![commit("first")]);
        model.vcs.history_selected = Some(0);
        model.vcs.history_has_more = true;

        assert!(handle_command(&mut model, CommandId("vcs_history_load_more")).is_some());
        assert_eq!(model.vcs.history_more, HistoryMoreState::Loading);
        assert!(handle_command(&mut model, CommandId("vcs_history_load_more")).is_none());
        let revision = model.vcs.history_revision;
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

        let HistoryState::Ready(commits) = &model.vcs.history else {
            panic!("history should remain ready");
        };
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[1].id, "second");
        assert_eq!(model.vcs.history_selected, Some(0));
        assert_eq!(model.vcs.history_more, HistoryMoreState::Idle);
        assert!(!model.vcs.history_has_more);
        assert!(handle_command(&mut model, CommandId("vcs_history_load_more")).is_none());
    }

    #[test]
    fn history_load_more_can_retry_without_replacing_loaded_commits() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.history = HistoryState::Ready(vec![CommitSummary {
            id: "first".to_string(),
            subject: "first".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
        }]);
        model.vcs.history_selected = Some(0);
        model.vcs.history_has_more = true;

        assert!(handle_command(&mut model, CommandId("vcs_history_load_more")).is_some());
        Box::new(HistoryCompletion {
            target: root,
            offset: 1,
            revision: model.vcs.history_revision,
            result: Err("network unavailable".to_string()),
        })
        .update(&mut model);

        assert_eq!(
            model.vcs.history_more,
            HistoryMoreState::Failed("network unavailable".to_string())
        );
        assert!(matches!(model.vcs.history, HistoryState::Ready(_)));
        assert!(handle_command(&mut model, CommandId("vcs_history_load_more")).is_some());
        assert_eq!(model.vcs.history_more, HistoryMoreState::Loading);
    }

    fn change(path: PathBuf, kind: ChangeKind) -> FileChange {
        FileChange {
            kind,
            staged: false,
            path,
            previous_path: None,
        }
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
        let revision = model.vcs.changes_revision;

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
        assert_eq!(model.vcs.selected_change, Some(0));

        select_change(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.selected_change, Some(1));
        select_change(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.selected_change, Some(0));
        select_change(&mut model, SelectionDirection::Previous);
        assert_eq!(model.vcs.selected_change, Some(1));
    }

    #[test]
    fn selected_change_opens_through_the_safe_document_flow() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.active_page = AppPage::Vcs;
        model.context_stack.push(Context::VCS);
        model.vcs.changes = ChangesState::Ready(vec![change(path.clone(), ChangeKind::Modified)]);
        model.vcs.selected_change = Some(0);

        let Some(Effect::OpenDocument(request)) =
            handle_command(&mut model, CommandId("open_vcs_change"))
        else {
            panic!("expected an open-document effect");
        };
        assert_eq!(model.active_page, AppPage::Editor);
        let manager = MessageManager::default();
        manager.update(&mut model, Message::FileOpened(request.perform()));
        assert_eq!(
            model
                .editor
                .buffer_path(model.editor.active_buffer().unwrap()),
            Some(std::fs::canonicalize(&path).unwrap().as_path())
        );
    }

    #[test]
    fn stage_completion_updates_changes_and_stale_results_are_ignored() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);
        model.vcs.selected_change = Some(0);

        assert!(handle_command(&mut model, CommandId("vcs_stage")).is_some());
        let revision = model.vcs.operation_revision;
        Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root.clone(),
            revision: revision.wrapping_sub(1),
            result: Ok(()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.notice, None);

        Box::new(OperationCompletion {
            kind: OperationKind::Stage,
            path: root.join("note.txt"),
            target: root,
            revision,
            result: Ok(()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.notice.as_deref(), Some("Staged file"));
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("expected optimistic changed-file state");
        };
        assert!(changes[0].staged);
    }

    #[test]
    fn unstage_merges_duplicate_entries_without_a_refresh() {
        let path = PathBuf::from("/workspace/note.txt");
        let mut staged = change(path.clone(), ChangeKind::Modified);
        staged.staged = true;
        let unstaged = change(path.clone(), ChangeKind::Modified);
        let mut model = Model::default();
        model.vcs.changes = ChangesState::Ready(vec![staged, unstaged]);
        model.vcs.selected_change = Some(1);

        assert!(apply_operation_to_changes(
            &mut model,
            OperationKind::Unstage,
            &path
        ));
        let ChangesState::Ready(changes) = model.vcs.changes() else {
            panic!("expected ready changes");
        };
        assert_eq!(changes.len(), 1);
        assert!(!changes[0].staged);
        assert_eq!(model.vcs.selected_change, Some(0));
    }

    #[test]
    fn refresh_keeps_ready_content_visible() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        model.vcs.changes_target = Some(root.clone());
        model.vcs.changes =
            ChangesState::Ready(vec![change(root.join("note.txt"), ChangeKind::Modified)]);

        assert!(refresh_changes(&mut model).is_some());
        assert!(matches!(model.vcs.changes(), ChangesState::Ready(_)));
        assert!(model.vcs.changes_refreshing);
        assert!(refresh_changes(&mut model).is_none());

        Box::new(ChangesCompletion {
            target: root,
            revision: model.vcs.changes_revision,
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert!(!model.vcs.changes_refreshing);
    }

    #[test]
    fn tab_loads_and_closes_an_inline_diff() {
        let root = PathBuf::from("/workspace");
        let mut model = Model {
            workspace: Some(Workspace::new(root.clone())),
            ..Model::default()
        };
        let selected = change(root.join("note.txt"), ChangeKind::Modified);
        model.vcs.changes = ChangesState::Ready(vec![selected.clone()]);
        model.vcs.selected_change = Some(0);

        assert!(handle_command(&mut model, CommandId("vcs_toggle_diff")).is_some());
        let revision = model.vcs.diff_revision;
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
            model.vcs.diff,
            DiffState::Ready {
                lines: vec!["+changed".to_string()],
                truncated: false,
            }
        );

        assert!(handle_command(&mut model, CommandId("vcs_toggle_diff")).is_none());
        assert_eq!(model.vcs.expanded_change, None);
        assert_eq!(model.vcs.diff, DiffState::Idle);

        assert!(handle_command(&mut model, CommandId("vcs_toggle_diff")).is_none());
        assert_eq!(
            model.vcs.expanded_change,
            Some((selected.path.clone(), selected.staged))
        );
        assert!(matches!(model.vcs.diff, DiffState::Ready { .. }));
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
            generation: model.vcs.diff_base_generation,
            result: Ok(Some(b"hello\n".to_vec())),
        })
        .update(&mut model);
        assert!(model.vcs.diff_base_loaded.contains(&id));
        assert!(sync_active_buffer(&mut model).is_none());

        assert!(refresh_buffer_base(&mut model, id).is_some());
        assert!(!model.vcs.diff_base_loaded.contains(&id));
        assert_eq!(model.vcs.diff_base_pending, Some((id, revision)));
    }

    #[test]
    fn repository_change_invalidates_cached_bases_and_refreshes_the_active_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello\n").unwrap();
        let mut model = Model::default();
        let id = open_file(&mut model, &path);
        model.vcs.diff_base_loaded.insert(id);

        assert!(repository_changed(&mut model).is_some());
        assert!(!model.vcs.diff_base_loaded.contains(&id));
        assert_eq!(model.vcs.diff_base_pending.map(|(id, _)| id), Some(id));
    }

    #[test]
    fn history_page_opens_closes_and_wraps_selection() {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model.active_page = AppPage::Vcs;
        model.context_stack.push(Context::VCS);

        assert!(handle_command(&mut model, CommandId("open_vcs_history")).is_none());
        assert_eq!(model.active_page, AppPage::VcsHistory);
        assert_eq!(model.context_stack.last(), Some(&Context::VcsHistory));
        assert!(handle_command(&mut model, CommandId("close_vcs_history")).is_none());
        assert_eq!(model.active_page, AppPage::Vcs);
        assert_eq!(model.context_stack.last(), Some(&Context::VCS));

        let commit = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: "subject".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
        };
        model.vcs.history = HistoryState::Ready(vec![commit("first"), commit("second")]);
        model.vcs.history_selected = Some(0);
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history_selected, Some(1));
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history_selected, Some(0));
        select_history(&mut model, SelectionDirection::Previous);
        assert_eq!(model.vcs.history_selected, Some(1));
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
            date: "2026-11-16".to_string(),
        };
        model.vcs.history = HistoryState::Ready(vec![commit.clone()]);
        model.vcs.history_selected = Some(0);
        let details = CommitDetails {
            full_id: commit.id.clone(),
            message: "complete message".to_string(),
            author: commit.author.clone(),
            date: commit.date.clone(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        };

        assert!(handle_command(&mut model, CommandId("vcs_history_toggle_diff")).is_some());
        Box::new(HistoryFilesCompletion {
            commit_id: commit.id.clone(),
            target: root,
            revision: model.vcs.history_files_revision,
            result: Ok(details),
        })
        .update(&mut model);
        assert!(matches!(
            model.vcs.history_files,
            CommitDetailsState::Ready(_)
        ));

        assert!(handle_command(&mut model, CommandId("vcs_history_toggle_diff")).is_none());
        assert_eq!(model.vcs.history_expanded, None);
        assert!(handle_command(&mut model, CommandId("vcs_history_toggle_diff")).is_none());
        assert_eq!(model.vcs.history_expanded, Some(commit.id));
        assert!(matches!(
            model.vcs.history_files,
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
            date: "2026-11-16".to_string(),
        };
        model.vcs.history = HistoryState::Ready(vec![commit]);
        model.vcs.history_selected = Some(0);
        let details = CommitDetails {
            full_id: "full-commit-id".to_string(),
            message: "complete message".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        };

        assert!(handle_command(&mut model, CommandId("open_vcs_commit_detail")).is_some());
        Box::new(HistoryDetailCompletion {
            commit_id: "abc1234".to_string(),
            target: root.clone(),
            revision: model.vcs.history_detail_revision,
            result: Ok(details),
        })
        .update(&mut model);
        assert!(matches!(
            model.vcs.history_detail,
            CommitDetailsState::Ready(_)
        ));
        assert_eq!(model.vcs.history_detail_selected_file, Some(0));

        assert!(handle_command(&mut model, CommandId("vcs_history_toggle_diff")).is_some());
        Box::new(HistoryFileDiffCompletion {
            commit_id: "full-commit-id".to_string(),
            file_path: "note.txt".to_string(),
            target: root,
            revision: model.vcs.history_detail_revision,
            result: Ok(Diff {
                lines: vec!["+changed".to_string()],
                truncated: false,
            }),
        })
        .update(&mut model);
        assert!(matches!(
            model.vcs.history_detail_diff,
            DiffState::Ready { .. }
        ));
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
            date: "2026-11-16".to_string(),
        };
        let file = |path: &str| CommitFile {
            kind: ChangeKind::Modified,
            path: path.to_string(),
            previous_path: None,
        };
        model.vcs.history = HistoryState::Ready(vec![commit.clone()]);
        model.vcs.history_selected = Some(0);
        model.vcs.history_commit_cache.insert(
            commit.id,
            CommitDetails {
                full_id: "full-commit-id".to_string(),
                message: "complete message".to_string(),
                author: "Eggplant".to_string(),
                date: "2026-11-16".to_string(),
                files: vec![file("first.txt"), file("second.txt")],
            },
        );

        assert!(handle_command(&mut model, CommandId("open_vcs_commit_detail")).is_none());
        assert_eq!(model.vcs.history_detail_selected_file, Some(0));
        select_history(&mut model, SelectionDirection::Next);
        assert_eq!(model.vcs.history_detail_selected_file, Some(1));
        assert!(handle_command(&mut model, CommandId("vcs_history_toggle_diff")).is_some());
        Box::new(HistoryFileDiffCompletion {
            commit_id: "full-commit-id".to_string(),
            file_path: "second.txt".to_string(),
            target: root,
            revision: model.vcs.history_detail_revision,
            result: Ok(Diff {
                lines: vec!["+second".to_string()],
                truncated: false,
            }),
        })
        .update(&mut model);

        assert_eq!(
            model.vcs.history_detail_expanded_file.as_deref(),
            Some("second.txt")
        );
        assert!(matches!(
            model.vcs.history_detail_diff,
            DiffState::Ready { .. }
        ));
    }

    #[test]
    fn placeholder_keys_explain_that_they_do_not_mutate() {
        let mut model = Model::default();
        for (command, message) in [
            ("vcs_discard_placeholder", "Discard is not implemented"),
            ("vcs_help_placeholder", "Help is not implemented"),
        ] {
            assert!(handle_command(&mut model, CommandId(command)).is_none());
            assert_eq!(model.vcs.notice.as_deref(), Some(message));
        }
    }

    #[test]
    fn deleted_change_does_not_create_a_buffer() {
        let mut model = Model::default();
        model.vcs.changes = ChangesState::Ready(vec![change(
            PathBuf::from("/workspace/deleted.txt"),
            ChangeKind::Deleted,
        )]);
        model.vcs.selected_change = Some(0);

        assert!(handle_command(&mut model, CommandId("open_vcs_change")).is_none());
        assert!(model.editor.is_empty());
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Cannot open a deleted file")
        );
    }
}
