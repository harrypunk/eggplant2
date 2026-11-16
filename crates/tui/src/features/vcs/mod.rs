use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use ::vcs::{ChangeKind, CommitSummary, Diff, FileChange};
use editor::{BufferId, DocumentFactory};

use crate::{
    data_object::{AppPage, CommandId, Context, Message, Model, StatusMessage},
    document,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
};

const HISTORY_LIMIT: usize = 1;
const DIFF_LIMIT: usize = 200;

#[derive(Debug)]
pub struct VcsState {
    head: Option<String>,
    target: Option<PathBuf>,
    revision: u64,
    refreshing: bool,
    pub(crate) history: HistoryState,
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
    .try_for_each(|command| commands.register(command))
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "open_vcs_page" => open_page(model),
        "close_vcs_page" | "close_vcs_page_q" => close_page(model),
        "vcs_next_change" => select_change(model, true),
        "vcs_previous_change" => select_change(model, false),
        "open_vcs_change" => open_selected_change(model),
        "vcs_stage" => mutate_selected_change(model, true),
        "vcs_unstage" => mutate_selected_change(model, false),
        "vcs_toggle_diff" => toggle_diff(model),
        "vcs_discard_placeholder" => placeholder(model, "Discard is not implemented"),
        "vcs_commit_placeholder" => placeholder(model, "Commit is not implemented"),
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

fn select_change(model: &mut Model, forward: bool) -> Option<Effect> {
    let ChangesState::Ready(changes) = &model.vcs.changes else {
        return None;
    };
    if changes.is_empty() {
        return None;
    }
    let selected = model.vcs.selected_change.unwrap_or(0);
    model.vcs.selected_change = Some(if forward {
        (selected + 1) % changes.len()
    } else {
        (selected + changes.len() - 1) % changes.len()
    });
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
    revision: u64,
}

impl FeatureEffect for HistoryRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::history(&self.target, HISTORY_LIMIT).map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryCompletion {
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

struct HistoryCompletion {
    target: PathBuf,
    revision: u64,
    result: Result<Vec<CommitSummary>, String>,
}

impl Completion for HistoryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_revision != self.revision
            || model.vcs.history_target.as_ref() != Some(&self.target)
        {
            return None;
        }
        model.vcs.history = match self.result {
            Ok(commits) => HistoryState::Ready(commits),
            Err(error) => HistoryState::Failed(error),
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
            revision: revision.wrapping_sub(1),
            result: Ok(Vec::new()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Loading);

        let commits = vec![CommitSummary {
            id: "abc1234".to_string(),
            subject: "subject".to_string(),
        }];
        Box::new(HistoryCompletion {
            target: root,
            revision,
            result: Ok(commits.clone()),
        })
        .update(&mut model);
        assert_eq!(model.vcs.history(), &HistoryState::Ready(commits));
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

        select_change(&mut model, true);
        assert_eq!(model.vcs.selected_change, Some(1));
        select_change(&mut model, true);
        assert_eq!(model.vcs.selected_change, Some(0));
        select_change(&mut model, false);
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
    fn placeholder_keys_explain_that_they_do_not_mutate() {
        let mut model = Model::default();
        for (command, message) in [
            ("vcs_discard_placeholder", "Discard is not implemented"),
            ("vcs_commit_placeholder", "Commit is not implemented"),
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
