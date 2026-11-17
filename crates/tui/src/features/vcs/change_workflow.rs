use std::path::{Path, PathBuf};

use ::vcs::{ChangeKind, Diff, FileChange};

use crate::{
    data_object::{Message, Model, StatusMessage},
    document,
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{
    ChangesState, DIFF_LIMIT, DiffState, SelectionDirection,
    changes::{ChangesApply, ChangesRequest},
    close_page, placeholder, repository_target, wrapped_index,
};

pub(super) fn select_change(model: &mut Model, direction: SelectionDirection) -> Option<Effect> {
    let ChangesState::Ready(changes) = model.vcs.changes() else {
        return None;
    };
    model.vcs.changes.selected =
        wrapped_index(model.vcs.changes.selected, changes.len(), direction);
    None
}

pub(super) fn open_selected_change(model: &mut Model) -> Option<Effect> {
    let change = model.vcs.selected_change()?.clone();
    if change.kind == ChangeKind::Deleted {
        model.status = Some(StatusMessage::info("Cannot open a deleted file"));
        return None;
    }
    let documents = model.vcs.documents().clone();
    close_page(model);
    document::open_file(&documents, model, &change.path)
}

pub(super) fn toggle_diff(model: &mut Model) -> Option<Effect> {
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

fn reject_if_operation_in_progress(model: &mut Model) -> bool {
    if !model.vcs.changes.operation_in_progress() {
        return false;
    }
    model.vcs.notice = Some("Git operation in progress".to_string());
    true
}

fn begin_operation(model: &mut Model, target: PathBuf) -> Option<u64> {
    let revision = model.vcs.changes.begin_operation(target)?;
    model.vcs.notice = None;
    Some(revision)
}

fn accept_operation_completion(model: &mut Model, target: &Path, revision: u64) -> bool {
    model.vcs.changes.accept_operation(target, revision)
}

pub(super) fn mutate_selected_change(model: &mut Model, stage: bool) -> Option<Effect> {
    if reject_if_operation_in_progress(model) {
        return None;
    }
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
    let revision = begin_operation(model, target.clone())?;
    Some(Effect::feature(OperationRequest {
        kind: if stage {
            OperationKind::Stage
        } else {
            OperationKind::Unstage
        },
        path: change.path,
        target,
        revision,
    }))
}

pub(super) fn mutate_all_changes(model: &mut Model, stage: bool) -> Option<Effect> {
    if reject_if_operation_in_progress(model) {
        return None;
    }
    let ChangesState::Ready(changes) = model.vcs.changes() else {
        return None;
    };
    if !changes.iter().any(|change| change.staged != stage) {
        return placeholder(
            model,
            match stage {
                true => "No unstaged changes",
                false => "No staged changes",
            },
        );
    }
    if model.editor.has_modified_buffers() || model.editor.has_saving_buffers() {
        return placeholder(model, "Wait for pending saves before changing the index");
    }
    let target = repository_target(model)?;
    let revision = begin_operation(model, target.clone())?;
    Some(Effect::feature(BulkOperationRequest {
        stage,
        target,
        revision,
    }))
}

pub(crate) fn sync_changes(model: &mut Model) -> Option<Effect> {
    let target = repository_target(model);
    if model.vcs.changes.sync_in_flight(&target) {
        model.vcs.changes.mark_dirty();
        return None;
    }
    refresh_changes(model)
}

pub(crate) fn refresh_changes(model: &mut Model) -> Option<Effect> {
    model.vcs.expanded_change = None;
    model.vcs.diff = DiffState::Idle;
    model.vcs.diff_cache.clear();
    model.vcs.diff_revision = model.vcs.diff_revision.wrapping_add(1);
    model
        .vcs
        .changes
        .refresh(repository_target(model))
        .map(Effect::feature)
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

pub(super) struct DiffCompletion {
    pub(super) change: FileChange,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<Diff, String>,
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
pub(super) enum OperationKind {
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
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

pub(super) struct OperationCompletion {
    pub(super) kind: OperationKind,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<(), String>,
}

impl Completion for OperationCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if !accept_operation_completion(model, &self.target, self.revision) {
            return None;
        }
        match self.result {
            Ok(()) => {
                model.vcs.notice = Some(match self.kind {
                    OperationKind::Stage => "Staged file".to_string(),
                    OperationKind::Unstage => "Unstaged file".to_string(),
                });
                model.vcs.changes.finish_operation();
                refresh_changes(model)
            }
            Err(error) => {
                model.vcs.notice = Some(error);
                None
            }
        }
    }
}

struct BulkOperationRequest {
    stage: bool,
    target: PathBuf,
    revision: u64,
}

impl FeatureEffect for BulkOperationRequest {
    fn perform(self: Box<Self>) -> Message {
        let result = match self.stage {
            true => ::vcs::stage_all(&self.target),
            false => ::vcs::unstage_all(&self.target),
        }
        .map_err(|error| error.to_string());
        Message::FeatureCompletion(FeatureCompletion::new(BulkOperationCompletion {
            stage: self.stage,
            target: self.target,
            revision: self.revision,
            result,
        }))
    }
}

pub(super) struct BulkOperationCompletion {
    pub(super) stage: bool,
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<(), String>,
}

impl Completion for BulkOperationCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if !accept_operation_completion(model, &self.target, self.revision) {
            return None;
        }
        if let Err(error) = self.result {
            model.vcs.notice = Some(error);
            return None;
        }
        model.vcs.notice = Some(match self.stage {
            true => "Staged all changes".to_string(),
            false => "Unstaged all changes".to_string(),
        });
        model.vcs.changes.finish_operation();
        refresh_changes(model)
    }
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

pub(super) struct ChangesCompletion {
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) result: Result<Vec<FileChange>, String>,
}

impl Completion for ChangesCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match model
            .vcs
            .changes
            .complete(self.target, self.revision, self.result)
        {
            ChangesApply::Ignored | ChangesApply::Applied => None,
            ChangesApply::Refresh => refresh_changes(model),
        }
    }
}
