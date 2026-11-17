use std::path::PathBuf;

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{ExplorerEntry, ExplorerState, fs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerRefreshKind {
    Manual,
    External,
}

#[derive(Debug)]
pub(crate) struct ExplorerRequest {
    pub root: PathBuf,
    pub expanded_directories: std::collections::HashSet<PathBuf>,
    pub ignore_mode: super::IgnoreMode,
    pub selected_path: Option<PathBuf>,
    pub generation: u64,
    pub kind: ExplorerRefreshKind,
}

pub(crate) struct ExplorerCompletion {
    request: ExplorerRequest,
    result: Result<Vec<ExplorerEntry>, String>,
}

pub(crate) enum ExplorerApply {
    Ignored,
    Applied(ExplorerRefreshKind),
    Failed(String),
}

impl ExplorerRequest {
    pub fn perform(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self.execute()))
    }

    fn execute(self) -> ExplorerCompletion {
        let result =
            fs::read_visible_entries(&self.root, &self.expanded_directories, self.ignore_mode)
                .map_err(|error| error.to_string());
        ExplorerCompletion {
            request: self,
            result,
        }
    }
}

impl FeatureEffect for ExplorerRequest {
    fn perform(self: Box<Self>) -> Message {
        ExplorerRequest::perform(*self)
    }
}

impl ExplorerState {
    pub(crate) fn complete_refresh(&mut self, completion: ExplorerCompletion) -> ExplorerApply {
        let ExplorerCompletion { request, result } = completion;
        if self.refresh_generation() != request.generation {
            return ExplorerApply::Ignored;
        }
        match result {
            Ok(entries) => {
                self.selected = request
                    .selected_path
                    .and_then(|path| entries.iter().position(|entry| entry.path == path))
                    .or_else(|| (!entries.is_empty()).then_some(0));
                self.entries = entries;
                ExplorerApply::Applied(request.kind)
            }
            Err(error) => ExplorerApply::Failed(error),
        }
    }
}

impl Completion for ExplorerCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let root = self.request.root.clone();
        match model.explorer.complete_refresh(*self) {
            ExplorerApply::Ignored => {}
            ExplorerApply::Applied(ExplorerRefreshKind::Manual) => model.status = None,
            ExplorerApply::Applied(ExplorerRefreshKind::External) => {}
            ExplorerApply::Failed(error) => {
                model.status = Some(StatusMessage::error(format!(
                    "failed to refresh `{}`: {error}",
                    root.display()
                )));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_reload_does_not_replace_newer_entries() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("first.txt"), "first").unwrap();
        let root = directory.path().to_path_buf();
        let mut state = ExplorerState::default();
        let first = state.reload_request(&root, ExplorerRefreshKind::Manual);
        let second = state.reload_request(&root, ExplorerRefreshKind::Manual);
        std::fs::write(root.join("second.txt"), "second").unwrap();

        assert!(matches!(
            state.complete_refresh(first.execute()),
            ExplorerApply::Ignored
        ));
        assert!(state.entries.is_empty());
        assert!(matches!(
            state.complete_refresh(second.execute()),
            ExplorerApply::Applied(ExplorerRefreshKind::Manual)
        ));
        assert_eq!(state.entries.len(), 2);
        assert_eq!(state.selected, Some(0));
    }
}
