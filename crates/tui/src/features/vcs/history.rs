use std::path::PathBuf;

use ::vcs::{CommitDetails, CommitSummary, Diff, HistoryPage};

use crate::{
    data_object::{AppPage, Context, Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{DIFF_LIMIT, DiffState, SelectionDirection, repository_target, wrapped_index};

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

pub(super) fn open_history(model: &mut Model) -> Option<Effect> {
    if model.active_page != AppPage::Vcs {
        return None;
    }
    model.active_page = AppPage::VcsHistory;
    model.context_stack.push(Context::VcsHistory);
    refresh_history(model)
}

pub(super) fn close_history(model: &mut Model) -> Option<Effect> {
    if model.vcs.history_pane == HistoryPane::Detail {
        model.vcs.history_pane = HistoryPane::List;
        model.vcs.history_detail = CommitDetailsState::Idle;
        model.vcs.history_detail_selected_file = None;
        model.vcs.history_detail_expanded_file = None;
        model.vcs.history_detail_diff = DiffState::Idle;
        model.vcs.history_detail_commit_id = None;
        model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
        return None;
    }
    model.active_page = AppPage::Vcs;
    model
        .context_stack
        .pop_if(|context| context == &Context::VcsHistory);
    None
}

pub(super) fn select_history(model: &mut Model, direction: SelectionDirection) -> Option<Effect> {
    if model.vcs.history_pane == HistoryPane::Detail {
        let CommitDetailsState::Ready(details) = &model.vcs.history_detail else {
            return None;
        };
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

pub(super) fn load_history_more(model: &mut Model) -> Option<Effect> {
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

pub(super) fn toggle_history_files(model: &mut Model) -> Option<Effect> {
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

pub(super) fn open_history_detail(model: &mut Model) -> Option<Effect> {
    let commit = selected_history_commit(model)?;
    if matches!(model.vcs.history_detail, CommitDetailsState::Ready(_)) {
        return None;
    }
    let cached = model.vcs.history_commit_cache.get(&commit.id).cloned();
    let target = if cached.is_none() {
        Some(repository_target(model)?)
    } else {
        None
    };
    model.vcs.history_pane = HistoryPane::Detail;
    model.vcs.history_detail_selected_file = None;
    model.vcs.history_detail_expanded_file = None;
    model.vcs.history_detail_diff = DiffState::Idle;
    model.vcs.history_detail_commit_id = Some(commit.id.clone());
    model.vcs.history_detail_revision = model.vcs.history_detail_revision.wrapping_add(1);
    if let Some(details) = cached {
        model.vcs.history_detail_selected_file = (!details.files.is_empty()).then_some(0);
        model.vcs.history_detail = CommitDetailsState::Ready(details);
        return None;
    }
    let target = target.expect("an uncached detail requires a repository target");
    model.vcs.history_detail = CommitDetailsState::Loading;
    model.vcs.history_detail_target = Some(target.clone());
    Some(Effect::feature(HistoryDetailRequest {
        commit_id: commit.id,
        target,
        revision: model.vcs.history_detail_revision,
    }))
}

pub(super) fn toggle_history_file_diff(model: &mut Model) -> Option<Effect> {
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

pub(super) fn refresh_history(model: &mut Model) -> Option<Effect> {
    model.vcs.history_has_more = false;
    model.vcs.history_more = HistoryMoreState::Idle;
    model.vcs.history_expanded = None;
    model.vcs.history_files = CommitDetailsState::Idle;
    model.vcs.history_files_revision = model.vcs.history_files_revision.wrapping_add(1);
    model.vcs.history_commit_cache.clear();
    model.vcs.history_pane = HistoryPane::List;
    model.vcs.history_detail = CommitDetailsState::Idle;
    model.vcs.history_detail_selected_file = None;
    model.vcs.history_detail_expanded_file = None;
    model.vcs.history_detail_diff = DiffState::Idle;
    model.vcs.history_detail_commit_id = None;
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

struct HistoryRequest {
    target: PathBuf,
    offset: usize,
    revision: u64,
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

pub(super) struct HistoryFilesCompletion {
    pub(super) commit_id: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<CommitDetails, String>,
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

pub(super) struct HistoryDetailCompletion {
    pub(super) commit_id: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<CommitDetails, String>,
}

impl Completion for HistoryDetailCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if model.vcs.history_detail_revision != self.revision
            || model.vcs.history_detail_target.as_ref() != Some(&self.target)
            || model.vcs.history_detail_commit_id.as_deref() != Some(self.commit_id.as_str())
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

pub(super) struct HistoryFileDiffCompletion {
    pub(super) commit_id: String,
    pub(super) file_path: String,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<Diff, String>,
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
