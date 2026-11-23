use std::path::PathBuf;

use ::vcs::{CommitDetails, CommitSummary, Diff, HistoryPage};

use crate::{
    clipboard::ClipboardWriter,
    copy_hash::{CopyHash, CopyHashOutcome, HashFormat},
    data_object::{AppPage, Message, Model, StatusMessage},
    document,
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{
    DIFF_LIMIT, DiffState, SelectionDirection, close_page, repository_target, wrapped_index,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryPane {
    #[default]
    List,
    Detail,
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
pub enum CommitDetailsState {
    #[default]
    Idle,
    Loading,
    Ready(CommitDetails),
    Failed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryApply {
    Ignored,
    Applied,
}

#[derive(Debug)]
pub struct HistoryController {
    pub(crate) state: HistoryState,
    pub(crate) pane: HistoryPane,
    pub(crate) has_more: bool,
    pub(crate) more: HistoryMoreState,
    pub(crate) selected: Option<usize>,
    pub(crate) expanded: Option<String>,
    pub(crate) files: CommitDetailsState,
    pub(super) files_target: Option<PathBuf>,
    pub(super) files_revision: u64,
    pub(super) commit_cache: std::collections::HashMap<String, CommitDetails>,
    pub(crate) detail: CommitDetailsState,
    pub(crate) selected_file: Option<usize>,
    pub(crate) expanded_file: Option<String>,
    pub(crate) diff: DiffState,
    pub(super) detail_target: Option<PathBuf>,
    pub(super) detail_commit_id: Option<String>,
    pub(super) detail_revision: u64,
    pub(super) detail_diff_cache: std::collections::HashMap<(String, String), Diff>,
    pub(super) target: Option<PathBuf>,
    pub(super) revision: u64,
}

impl Default for HistoryController {
    fn default() -> Self {
        Self {
            state: HistoryState::Idle,
            pane: HistoryPane::List,
            has_more: false,
            more: HistoryMoreState::Idle,
            selected: None,
            expanded: None,
            files: CommitDetailsState::Idle,
            files_target: None,
            files_revision: 0,
            commit_cache: std::collections::HashMap::new(),
            detail: CommitDetailsState::Idle,
            selected_file: None,
            expanded_file: None,
            diff: DiffState::Idle,
            detail_target: None,
            detail_commit_id: None,
            detail_revision: 0,
            detail_diff_cache: std::collections::HashMap::new(),
            target: None,
            revision: 0,
        }
    }
}

impl HistoryController {
    pub fn state(&self) -> &HistoryState {
        &self.state
    }

    pub(super) fn refresh(&mut self, target: Option<PathBuf>) -> Option<HistoryRequest> {
        self.has_more = false;
        self.more = HistoryMoreState::Idle;
        self.expanded = None;
        self.files = CommitDetailsState::Idle;
        self.files_revision = self.files_revision.wrapping_add(1);
        self.commit_cache.clear();
        self.pane = HistoryPane::List;
        self.detail = CommitDetailsState::Idle;
        self.selected_file = None;
        self.expanded_file = None;
        self.diff = DiffState::Idle;
        self.detail_commit_id = None;
        self.detail_revision = self.detail_revision.wrapping_add(1);
        self.detail_diff_cache.clear();
        let Some(target) = target else {
            self.target = None;
            self.state = HistoryState::Failed("not a Git repository".to_string());
            self.revision = self.revision.wrapping_add(1);
            return None;
        };
        if self.target.as_ref() == Some(&target) && self.state == HistoryState::Loading {
            return None;
        }
        self.target = Some(target.clone());
        self.state = HistoryState::Loading;
        self.revision = self.revision.wrapping_add(1);
        Some(HistoryRequest {
            target,
            offset: 0,
            revision: self.revision,
        })
    }

    pub(super) fn complete_page(
        &mut self,
        target: PathBuf,
        offset: usize,
        revision: u64,
        result: Result<::vcs::HistoryPage, String>,
    ) {
        if self.revision != revision || self.target.as_ref() != Some(&target) {
            return;
        }
        if offset == 0 {
            self.more = HistoryMoreState::Idle;
            self.state = match result {
                Ok(page) => {
                    self.selected = (!page.commits.is_empty()).then_some(0);
                    self.has_more = page.has_more;
                    HistoryState::Ready(page.commits)
                }
                Err(error) => {
                    self.selected = None;
                    self.has_more = false;
                    HistoryState::Failed(error)
                }
            };
            return;
        }
        let HistoryState::Ready(commits) = &mut self.state else {
            return;
        };
        match result {
            Ok(page) => {
                commits.extend(page.commits);
                self.has_more = page.has_more;
                self.more = HistoryMoreState::Idle;
            }
            Err(error) => self.more = HistoryMoreState::Failed(error),
        }
    }
    fn complete_files(
        &mut self,
        request: HistoryFilesRequest,
        result: Result<CommitDetails, String>,
    ) -> HistoryApply {
        if self.files_revision != request.revision
            || self.files_target.as_ref() != Some(&request.target)
            || self.expanded.as_ref() != Some(&request.commit_id)
        {
            return HistoryApply::Ignored;
        }
        self.files = match result {
            Ok(details) => {
                self.commit_cache.insert(request.commit_id, details.clone());
                CommitDetailsState::Ready(details)
            }
            Err(error) => CommitDetailsState::Failed(error),
        };
        HistoryApply::Applied
    }

    fn complete_detail(
        &mut self,
        request: HistoryDetailRequest,
        result: Result<CommitDetails, String>,
    ) -> HistoryApply {
        if self.detail_revision != request.revision
            || self.detail_target.as_ref() != Some(&request.target)
            || self.detail_commit_id.as_deref() != Some(request.commit_id.as_str())
        {
            return HistoryApply::Ignored;
        }
        self.detail = match result {
            Ok(details) => {
                self.selected_file = (!details.files.is_empty()).then_some(0);
                self.commit_cache.insert(request.commit_id, details.clone());
                CommitDetailsState::Ready(details)
            }
            Err(error) => CommitDetailsState::Failed(error),
        };
        HistoryApply::Applied
    }

    fn complete_file_diff(
        &mut self,
        request: HistoryFileDiffRequest,
        result: Result<Diff, String>,
    ) -> HistoryApply {
        if self.detail_revision != request.revision
            || self.detail_target.as_ref() != Some(&request.target)
            || self.expanded_file.as_ref() != Some(&request.file_path)
        {
            return HistoryApply::Ignored;
        }
        self.diff = match result {
            Ok(diff) => {
                self.detail_diff_cache
                    .insert((request.commit_id, request.file_path), diff.clone());
                DiffState::Ready {
                    lines: diff.lines,
                    truncated: diff.truncated,
                }
            }
            Err(error) => DiffState::Failed(error),
        };
        HistoryApply::Applied
    }
}

pub(super) fn open_history(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Vcs {
        return None;
    }
    model.active_page = AppPage::VcsHistory;
    refresh_history(model)
}

pub(super) fn close_history(model: &mut Model) -> Option<Effect> {
    if model.vcs.history.pane == HistoryPane::Detail {
        model.vcs.history.pane = HistoryPane::List;
        model.vcs.history.detail = CommitDetailsState::Idle;
        model.vcs.history.selected_file = None;
        model.vcs.history.expanded_file = None;
        model.vcs.history.diff = DiffState::Idle;
        model.vcs.history.detail_commit_id = None;
        model.vcs.history.detail_revision = model.vcs.history.detail_revision.wrapping_add(1);
        return None;
    }
    model.active_page = AppPage::Vcs;
    None
}

pub(super) fn select_history(model: &mut Model, direction: SelectionDirection) -> Option<Effect> {
    if model.vcs.history.pane == HistoryPane::Detail {
        let CommitDetailsState::Ready(details) = &model.vcs.history.detail else {
            return None;
        };
        model.vcs.history.selected_file = wrapped_index(
            model.vcs.history.selected_file,
            details.files.len(),
            direction,
        );
        return None;
    }
    let HistoryState::Ready(commits) = model.vcs.history() else {
        return None;
    };
    model.vcs.history.selected =
        wrapped_index(model.vcs.history.selected, commits.len(), direction);
    None
}

pub(super) fn load_history_more(model: &mut Model) -> Option<Effect> {
    let HistoryState::Ready(commits) = model.vcs.history() else {
        return None;
    };
    if !model.vcs.history.has_more || model.vcs.history.more == HistoryMoreState::Loading {
        return None;
    }
    let offset = commits.len();
    let target = repository_target(model)?;
    model.vcs.history.target = Some(target.clone());
    model.vcs.history.more = HistoryMoreState::Loading;
    model.vcs.history.revision = model.vcs.history.revision.wrapping_add(1);
    Some(Effect::feature(HistoryRequest {
        target,
        offset,
        revision: model.vcs.history.revision,
    }))
}

pub(super) fn toggle_history_files(model: &mut Model) -> Option<Effect> {
    let commit = selected_history_commit(model)?;
    let commit_id = commit.id;
    if model.vcs.history.expanded.as_ref() == Some(&commit_id) {
        model.vcs.history.expanded = None;
        model.vcs.history.files = CommitDetailsState::Idle;
        model.vcs.history.files_revision = model.vcs.history.files_revision.wrapping_add(1);
        return None;
    }
    if let Some(details) = model.vcs.history.commit_cache.get(&commit_id).cloned() {
        model.vcs.history.expanded = Some(commit_id);
        model.vcs.history.files = CommitDetailsState::Ready(details);
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.history.expanded = Some(commit_id.clone());
    model.vcs.history.files = CommitDetailsState::Loading;
    model.vcs.history.files_target = Some(target.clone());
    model.vcs.history.files_revision = model.vcs.history.files_revision.wrapping_add(1);
    Some(Effect::feature(HistoryFilesRequest {
        commit_id,
        target,
        revision: model.vcs.history.files_revision,
    }))
}

pub(super) fn open_history_detail(model: &mut Model) -> Option<Effect> {
    let commit = selected_history_commit(model)?;
    if let CommitDetailsState::Ready(_) = model.vcs.history.detail {
        return None;
    }
    let cached = model.vcs.history.commit_cache.get(&commit.id).cloned();
    let target = if cached.is_none() {
        Some(repository_target(model)?)
    } else {
        None
    };
    model.vcs.history.pane = HistoryPane::Detail;
    model.vcs.history.selected_file = None;
    model.vcs.history.expanded_file = None;
    model.vcs.history.diff = DiffState::Idle;
    model.vcs.history.detail_commit_id = Some(commit.id.clone());
    model.vcs.history.detail_revision = model.vcs.history.detail_revision.wrapping_add(1);
    if let Some(details) = cached {
        model.vcs.history.selected_file = (!details.files.is_empty()).then_some(0);
        model.vcs.history.detail = CommitDetailsState::Ready(details);
        return None;
    }
    let target = target.expect("an uncached detail requires a repository target");
    model.vcs.history.detail = CommitDetailsState::Loading;
    model.vcs.history.detail_target = Some(target.clone());
    Some(Effect::feature(HistoryDetailRequest {
        commit_id: commit.id,
        target,
        revision: model.vcs.history.detail_revision,
    }))
}

pub(super) fn open_history_file(model: &mut Model) -> Option<Effect> {
    let CommitDetailsState::Ready(details) = &model.vcs.history.detail else {
        return None;
    };
    let file = details.files.get(model.vcs.history.selected_file?)?.clone();
    if file.kind == ::vcs::ChangeKind::Deleted {
        return Some(crate::notifications::report_rejection(
            model,
            "Cannot open a deleted file",
        ));
    }
    let root = match &model.workspace {
        Some(workspace) => workspace.root().to_path_buf(),
        None => repository_target(model)?.parent()?.to_path_buf(),
    };
    let path = root.join(file.path);
    let documents = model.vcs.documents().clone();

    if model.vcs.history.pane == HistoryPane::Detail {
        close_history(model);
    }
    close_history(model);
    close_page(model);
    document::open_file(&documents, model, &path)
}

pub(super) fn toggle_history_file_diff(model: &mut Model) -> Option<Effect> {
    let CommitDetailsState::Ready(details) = &model.vcs.history.detail else {
        return None;
    };
    let file = details.files.get(model.vcs.history.selected_file?)?.clone();
    let commit_id = details.full_id.clone();
    let file_path = file.path.clone();
    if model.vcs.history.expanded_file.as_ref() == Some(&file_path) {
        model.vcs.history.expanded_file = None;
        model.vcs.history.diff = DiffState::Idle;
        model.vcs.history.detail_revision = model.vcs.history.detail_revision.wrapping_add(1);
        return None;
    }
    let cache_key = (commit_id.clone(), file_path.clone());
    if let Some(diff) = model.vcs.history.detail_diff_cache.get(&cache_key).cloned() {
        model.vcs.history.expanded_file = Some(file_path);
        model.vcs.history.diff = DiffState::Ready {
            lines: diff.lines,
            truncated: diff.truncated,
        };
        return None;
    }
    let target = repository_target(model)?;
    model.vcs.history.expanded_file = Some(file_path.clone());
    model.vcs.history.diff = DiffState::Loading;
    model.vcs.history.detail_target = Some(target.clone());
    model.vcs.history.detail_revision = model.vcs.history.detail_revision.wrapping_add(1);
    Some(Effect::feature(HistoryFileDiffRequest {
        commit_id,
        file_path,
        target,
        revision: model.vcs.history.detail_revision,
    }))
}

pub(super) fn copy_history_hash(model: &mut Model, format: HashFormat) -> Option<Effect> {
    if model.vcs.history.pane != HistoryPane::Detail {
        return None;
    }
    let CommitDetailsState::Ready(details) = &model.vcs.history.detail else {
        return None;
    };
    let copy = CopyHash::new(details.full_id.clone(), format, model.register.provider());
    Some(Effect::feature(HistoryCopyRequest { copy }))
}

struct HistoryCopyRequest<C> {
    copy: CopyHash<C>,
}

impl<C: ClipboardWriter + Send + 'static> FeatureEffect for HistoryCopyRequest<C> {
    fn perform(self: Box<Self>) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(HistoryCopyCompletion {
            outcome: self.copy.execute(),
        }))
    }
}

struct HistoryCopyCompletion {
    outcome: CopyHashOutcome,
}

impl Completion for HistoryCopyCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let outcome = self.outcome;
        let label = match outcome.format {
            HashFormat::Short => "short hash",
            HashFormat::Full => "full hash",
        };
        let status = match outcome.result {
            Ok(()) => StatusMessage::info(format!("Copied {label} {}", outcome.text)),
            Err(error) => StatusMessage::error(format!("System clipboard unavailable: {error}")),
        };
        Some(crate::notifications::publish_status(
            model,
            status,
            Some("Git"),
        ))
    }
}

fn selected_history_commit(model: &Model) -> Option<CommitSummary> {
    let HistoryState::Ready(commits) = model.vcs.history() else {
        return None;
    };
    commits.get(model.vcs.history.selected?).cloned()
}

pub(super) fn refresh_history(model: &mut Model) -> Option<Effect> {
    model
        .vcs
        .history
        .refresh(repository_target(model))
        .map(Effect::feature)
}

pub(super) struct HistoryRequest {
    pub(super) target: PathBuf,
    pub(super) offset: usize,
    pub(super) revision: u64,
}

impl FeatureEffect for HistoryRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = ::vcs::history(&self.target, self.offset, super::HISTORY_PAGE_SIZE)
            .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(HistoryCompletion {
            target: self.target,
            offset: self.offset,
            revision: self.revision,
            result,
        }))
    }
}

pub(super) struct HistoryCompletion {
    pub(super) target: PathBuf,
    pub(super) offset: usize,
    pub(super) revision: u64,
    pub(super) result: Result<HistoryPage, String>,
}

impl Completion for HistoryCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model
            .vcs
            .history
            .complete_page(self.target, self.offset, self.revision, self.result);
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

pub(super) struct HistoryFilesCompletion {
    pub(super) commit_id: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<CommitDetails, String>,
}

impl Completion for HistoryFilesCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model.vcs.history.complete_files(
            HistoryFilesRequest {
                commit_id: self.commit_id,
                target: self.target,
                revision: self.revision,
            },
            self.result,
        );
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

pub(super) struct HistoryDetailCompletion {
    pub(super) commit_id: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<CommitDetails, String>,
}

impl Completion for HistoryDetailCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model.vcs.history.complete_detail(
            HistoryDetailRequest {
                commit_id: self.commit_id,
                target: self.target,
                revision: self.revision,
            },
            self.result,
        );
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

pub(super) struct HistoryFileDiffCompletion {
    pub(super) commit_id: String,
    pub(super) file_path: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<Diff, String>,
}

impl Completion for HistoryFileDiffCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model.vcs.history.complete_file_diff(
            HistoryFileDiffRequest {
                commit_id: self.commit_id,
                file_path: self.file_path,
                target: self.target,
                revision: self.revision,
            },
            self.result,
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(id: &str) -> CommitSummary {
        CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 0,
            branches: Vec::new(),
        }
    }

    #[test]
    fn refresh_and_pagination_are_controller_transitions() {
        let root = PathBuf::from("/workspace");
        let mut state = HistoryController::default();
        let request = state.refresh(Some(root.clone())).unwrap();
        assert!(state.refresh(Some(root.clone())).is_none());

        state.complete_page(
            root.clone(),
            0,
            request.revision.wrapping_sub(1),
            Ok(HistoryPage {
                commits: Vec::new(),
                has_more: false,
            }),
        );
        assert_eq!(state.state(), &HistoryState::Loading);
        state.complete_page(
            root.clone(),
            0,
            request.revision,
            Ok(HistoryPage {
                commits: vec![commit("first")],
                has_more: true,
            }),
        );
        assert_eq!(state.selected, Some(0));

        state.revision = state.revision.wrapping_add(1);
        let revision = state.revision;
        state.complete_page(
            root,
            1,
            revision,
            Ok(HistoryPage {
                commits: vec![commit("second")],
                has_more: false,
            }),
        );
        let HistoryState::Ready(commits) = state.state() else {
            panic!("history should be ready");
        };
        assert_eq!(commits.len(), 2);
        assert!(!state.has_more);
        assert_eq!(state.more, HistoryMoreState::Idle);
    }

    #[test]
    fn detail_completions_validate_commit_and_diff_identity() {
        let root = PathBuf::from("/workspace");
        let mut state = HistoryController {
            detail: CommitDetailsState::Loading,
            detail_target: Some(root.clone()),
            detail_commit_id: Some("a".to_string()),
            detail_revision: 4,
            expanded_file: Some("note.txt".to_string()),
            ..HistoryController::default()
        };
        let details = || CommitDetails {
            full_id: "a".to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "date".to_string(),
            files: Vec::new(),
        };

        assert!(matches!(
            state.complete_detail(
                HistoryDetailRequest {
                    commit_id: "b".to_string(),
                    target: root.clone(),
                    revision: 4,
                },
                Ok(details())
            ),
            HistoryApply::Ignored
        ));
        assert_eq!(state.detail, CommitDetailsState::Loading);
        assert!(matches!(
            state.complete_detail(
                HistoryDetailRequest {
                    commit_id: "a".to_string(),
                    target: root.clone(),
                    revision: 4,
                },
                Ok(details())
            ),
            HistoryApply::Applied
        ));

        let diff = || {
            Ok(Diff {
                lines: vec!["+new".to_string()],
                truncated: false,
            })
        };
        assert!(matches!(
            state.complete_file_diff(
                HistoryFileDiffRequest {
                    commit_id: "a".to_string(),
                    file_path: "note.txt".to_string(),
                    target: root.clone(),
                    revision: 3,
                },
                diff()
            ),
            HistoryApply::Ignored
        ));
        assert!(matches!(
            state.complete_file_diff(
                HistoryFileDiffRequest {
                    commit_id: "a".to_string(),
                    file_path: "note.txt".to_string(),
                    target: root,
                    revision: 4,
                },
                diff()
            ),
            HistoryApply::Applied
        ));
        assert!(matches!(state.diff, DiffState::Ready { .. }));
    }

    fn history_file_model(root: &std::path::Path, kind: ::vcs::ChangeKind) -> Model {
        let mut model = Model {
            active_page: AppPage::VcsHistory,
            workspace: Some(crate::data_object::Workspace::new(root.to_path_buf())),
            ..Model::default()
        };
        model.vcs.history.pane = HistoryPane::Detail;
        model.vcs.history.detail = CommitDetailsState::Ready(CommitDetails {
            full_id: "abc1234".to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "date".to_string(),
            files: vec![::vcs::CommitFile {
                kind,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        });
        model.vcs.history.selected_file = Some(0);
        model
    }

    #[test]
    fn enter_on_a_detail_file_closes_history_and_opens_the_editor() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();
        let mut model = history_file_model(directory.path(), ::vcs::ChangeKind::Modified);

        let Some(Effect::OpenDocument(request)) = crate::vcs::handle_command(
            &mut model,
            crate::data_object::CommandId("open_vcs_commit_detail"),
        ) else {
            panic!("expected a document-open request");
        };
        assert_eq!(model.active_page, AppPage::Editor);
        assert_eq!(model.vcs.history.pane, HistoryPane::List);
        assert!(model.context_stack.is_empty());

        crate::manager::message_manager::MessageManager::default()
            .update(&mut model, Message::FileOpened(request.perform()));

        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);
        assert_eq!(
            model
                .editor
                .buffer_path(model.editor.active_buffer().unwrap()),
            Some(std::fs::canonicalize(path).unwrap().as_path())
        );
    }

    #[test]
    fn deleted_detail_files_are_rejected_without_closing_history() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = history_file_model(directory.path(), ::vcs::ChangeKind::Deleted);

        let effect = open_history_file(&mut model);

        assert!(matches!(effect, Some(Effect::Feature(_))));
        assert_eq!(model.active_page, AppPage::VcsHistory);
        assert!(model.context_stack.is_empty());
        assert!(model.editor.is_empty());
        assert_eq!(
            model.notifications.current().unwrap().title(),
            Some("Rejected")
        );
    }

    #[test]
    fn copy_adapter_requires_visible_detail_and_preserves_the_yank_register() {
        let full = "0123456789abcdef0123456789abcdef01234567";
        let mut model = Model::default();
        model
            .register
            .update_from_clipboard("keep register".to_string());
        model
            .register
            .set_provider(helix_view::clipboard::ClipboardProvider::None);
        model.vcs.history.detail = CommitDetailsState::Ready(CommitDetails {
            full_id: full.to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "date".to_string(),
            files: Vec::new(),
        });
        assert!(copy_history_hash(&mut model, HashFormat::Short).is_none());

        model.vcs.history.pane = HistoryPane::Detail;
        let Some(Effect::Feature(request)) = copy_history_hash(&mut model, HashFormat::Short)
        else {
            panic!("ready detail should compose a clipboard request");
        };
        assert_eq!(model.register.unnamed(), "keep register");
        let Message::FeatureCompletion(completion) = request.perform() else {
            panic!("expected history copy completion");
        };
        completion.update(&mut model);
        assert_eq!(model.register.unnamed(), "keep register");
        assert_eq!(
            model.notifications.current().unwrap().kind(),
            crate::capabilities::notification::NotificationKind::Error
        );

        Box::new(HistoryCopyCompletion {
            outcome: CopyHashOutcome {
                format: HashFormat::Full,
                text: full.to_string(),
                result: Ok(()),
            },
        })
        .update(&mut model);
        assert_eq!(model.register.unnamed(), "keep register");
        assert!(model.status.is_none());
        let notification = model.notifications.current().unwrap();
        assert_eq!(notification.title(), Some("Git"));
        assert_eq!(notification.message(), format!("Copied full hash {full}"));
    }
}
