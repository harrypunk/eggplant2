use std::path::{Path, PathBuf};

use ::vcs::{ChangeKind, Diff, FileChange};

use crate::{
    data_object::{Message, Model, StatusMessage},
    document,
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{
    ChangesState, DIFF_LIMIT, SelectionDirection,
    changes::{ChangesApply, ChangesRequest, DiffRequest},
    close_page, placeholder, report, repository_target, wrapped_index,
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
    model
        .vcs
        .changes
        .toggle_diff(change, repository_target(model))
        .map(Effect::feature)
}

fn reject_if_operation_in_progress(model: &mut Model) -> Option<Effect> {
    if !model.vcs.changes.operation_in_progress() {
        return None;
    }
    Some(report(
        model,
        StatusMessage::info("Git operation in progress"),
    ))
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
    if let Some(effect) = reject_if_operation_in_progress(model) {
        return Some(effect);
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
    if let Some(effect) = reject_if_operation_in_progress(model) {
        return Some(effect);
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
    model.vcs.changes.invalidate_diff();
    model
        .vcs
        .changes
        .refresh(repository_target(model))
        .map(Effect::feature)
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
        model.vcs.changes.complete_diff(
            DiffRequest {
                change: self.change,
                target: self.target,
                revision: self.revision,
            },
            self.result,
        );
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
                let status = StatusMessage::info(match self.kind {
                    OperationKind::Stage => "Staged file",
                    OperationKind::Unstage => "Unstaged file",
                });
                model.vcs.changes.finish_operation();
                Effect::combine(refresh_changes(model), Some(report(model, status)))
            }
            Err(error) => Some(report(model, StatusMessage::error(error))),
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
            return Some(report(model, StatusMessage::error(error)));
        }
        let status = StatusMessage::info(match self.stage {
            true => "Staged all changes",
            false => "Unstaged all changes",
        });
        model.vcs.changes.finish_operation();
        Effect::combine(refresh_changes(model), Some(report(model, status)))
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
