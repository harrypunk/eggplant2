use std::path::PathBuf;

use editor::BufferId;

use crate::{
    data_object::{Message, Model},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{
    buffer_diff::BufferDiffApply, change_workflow::refresh_changes, history::refresh_history,
};

#[derive(Debug, Default)]
pub struct RepositoryState {
    pub(super) head: Option<String>,
    pub(super) target: Option<PathBuf>,
    pub(super) revision: u64,
    pub(super) refreshing: bool,
}

impl RepositoryState {
    pub fn head(&self) -> Option<&str> {
        self.head.as_deref()
    }

    pub(super) fn cancel(&mut self) {
        self.refreshing = false;
    }

    pub(super) fn refresh(&mut self, target: Option<PathBuf>) -> Option<HeadRequest> {
        let Some(target) = target else {
            self.target = None;
            self.head = None;
            self.refreshing = false;
            self.revision = self.revision.wrapping_add(1);
            return None;
        };
        if self.target.as_ref() == Some(&target) && self.refreshing {
            return None;
        }
        self.target = Some(target.clone());
        self.refreshing = true;
        self.revision = self.revision.wrapping_add(1);
        Some(HeadRequest {
            target,
            revision: self.revision,
        })
    }

    pub(super) fn complete(
        &mut self,
        target: PathBuf,
        revision: u64,
        head: Option<String>,
    ) -> bool {
        if self.revision != revision || self.target.as_ref() != Some(&target) {
            return false;
        }
        self.refreshing = false;
        self.head = head;
        true
    }
}

/// Load the active buffer's HEAD base once, without blocking the editor.
pub fn sync_active_buffer(model: &mut Model) -> Option<Effect> {
    let (buffer_id, revision) = model.editor.active_revision()?;
    if !model.vcs.buffer_diff.needs_load(buffer_id, revision) {
        return None;
    }
    let path = model.editor.buffer_path(buffer_id)?.to_path_buf();
    let generation = model.vcs.buffer_diff.begin(buffer_id, revision);
    Some(Effect::feature(BufferDiffRequest {
        buffer_id,
        revision,
        generation,
        path,
    }))
}

/// Invalidate one externally reloaded buffer and refresh it if active.
pub fn refresh_buffer_base(model: &mut Model, buffer_id: BufferId) -> Option<Effect> {
    model.vcs.buffer_diff.invalidate(buffer_id);
    if model.editor.active_buffer() == Some(buffer_id) {
        return sync_active_buffer(model);
    }
    None
}

/// Invalidate Git metadata and schedule fresh HEAD, status, and active-buffer state.
pub fn repository_changed(model: &mut Model) -> Option<Effect> {
    model.vcs.buffer_diff.invalidate_all();
    model.vcs.repository.cancel();
    model.vcs.changes.cancel();
    if model.vcs.history == super::HistoryState::Loading {
        model.vcs.history = super::HistoryState::Idle;
    }
    Effect::combine(
        Effect::combine(refresh(model), refresh_history(model)),
        Effect::combine(refresh_changes(model), sync_active_buffer(model)),
    )
}

/// Start one background HEAD lookup, preserving the previous label until completion.
pub fn refresh(model: &mut Model) -> Option<Effect> {
    model
        .vcs
        .repository
        .refresh(repository_target(model))
        .map(Effect::feature)
}

pub(crate) fn repository_target(model: &Model) -> Option<PathBuf> {
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

pub(super) struct BufferDiffCompletion {
    pub(super) buffer_id: BufferId,
    pub(super) revision: i32,
    pub(super) generation: u64,
    pub(super) result: Result<Option<Vec<u8>>, String>,
}

impl Completion for BufferDiffCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let BufferDiffApply::Apply {
            buffer_id,
            revision,
            base,
        } = model.vcs.buffer_diff.complete(
            self.buffer_id,
            self.revision,
            self.generation,
            self.result,
        )
        else {
            return None;
        };
        if let Ok(true) = model.editor.set_buffer_diff_base(buffer_id, revision, base) {
            model.vcs.buffer_diff.mark_loaded(buffer_id);
        }
        None
    }
}

pub(super) struct HeadRequest {
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

pub(super) struct HeadCompletion {
    pub(super) target: PathBuf,
    pub(super) revision: u64,
    pub(super) head: Option<String>,
}

impl Completion for HeadCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model
            .vcs
            .repository
            .complete(self.target, self.revision, self.head);
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_refresh_coalesces_and_rejects_stale_completion() {
        let root = PathBuf::from("/workspace");
        let mut state = RepositoryState::default();
        let request = state.refresh(Some(root.clone())).unwrap();
        assert!(state.refresh(Some(root.clone())).is_none());
        assert!(!state.complete(
            root.clone(),
            request.revision.wrapping_sub(1),
            Some("stale".to_string())
        ));
        assert!(state.complete(root, request.revision, Some("main".to_string())));
        assert_eq!(state.head(), Some("main"));
        assert!(!state.refreshing);

        assert!(state.refresh(None).is_none());
        assert_eq!(state.head(), None);
        assert_eq!(state.target, None);
    }
}
