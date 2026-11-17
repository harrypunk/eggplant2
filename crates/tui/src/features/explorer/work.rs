use std::{io, path::PathBuf};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{ExplorerEntry, ExplorerState, IgnoreMode, fs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerRefreshKind {
    Manual,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    Refresh(ExplorerRefreshKind),
    Expansion { path: PathBuf },
    Ignore(IgnoreMode),
    Reveal { path: PathBuf },
}

#[derive(Debug)]
pub(crate) struct ExplorerRequest {
    pub root: PathBuf,
    pub expanded_directories: std::collections::HashSet<PathBuf>,
    pub ignore_mode: IgnoreMode,
    pub selected_path: Option<PathBuf>,
    pub generation: u64,
    pub action: ExplorerAction,
}

pub(crate) struct ExplorerCompletion {
    request: ExplorerRequest,
    result: io::Result<Vec<ExplorerEntry>>,
}

pub(crate) enum ExplorerApply {
    Ignored,
    Unchanged,
    Applied(ExplorerAction),
    Failed(ExplorerAction, io::Error),
}

impl ExplorerRequest {
    pub fn perform(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self.execute()))
    }

    pub(super) fn execute(self) -> ExplorerCompletion {
        let result =
            fs::read_visible_entries(&self.root, &self.expanded_directories, self.ignore_mode);
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
    pub(crate) fn complete(&mut self, completion: ExplorerCompletion) -> ExplorerApply {
        let ExplorerCompletion { request, result } = completion;
        if self.refresh_generation() != request.generation {
            return ExplorerApply::Ignored;
        }
        let entries = match result {
            Ok(entries) => entries,
            Err(error) => return ExplorerApply::Failed(request.action, error),
        };
        let selected_path = match &request.action {
            ExplorerAction::Reveal { path } => {
                if !entries.iter().any(|entry| &entry.path == path) {
                    return ExplorerApply::Unchanged;
                }
                Some(path.clone())
            }
            _ => request.selected_path.clone(),
        };
        self.replace_tree(
            entries,
            request.expanded_directories,
            request.ignore_mode,
            selected_path,
        );
        ExplorerApply::Applied(request.action)
    }
}

impl Completion for ExplorerCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let root = self.request.root.clone();
        match model.explorer.complete(*self) {
            ExplorerApply::Ignored | ExplorerApply::Unchanged => {}
            ExplorerApply::Applied(ExplorerAction::Refresh(ExplorerRefreshKind::Manual))
            | ExplorerApply::Applied(ExplorerAction::Expansion { .. }) => model.status = None,
            ExplorerApply::Applied(ExplorerAction::Refresh(ExplorerRefreshKind::External))
            | ExplorerApply::Applied(ExplorerAction::Reveal { .. }) => {}
            ExplorerApply::Applied(ExplorerAction::Ignore(mode)) => {
                let text = match mode {
                    IgnoreMode::HideIgnored => "Hiding ignored files",
                    IgnoreMode::ShowIgnored => "Showing ignored files",
                };
                model.status = Some(StatusMessage::info(text));
            }
            ExplorerApply::Failed(action, error) => {
                let (action, path) = match action {
                    ExplorerAction::Refresh(_) => ("failed to refresh", root),
                    ExplorerAction::Expansion { path } => ("failed to update directory tree", path),
                    ExplorerAction::Ignore(_) => ("failed to toggle ignored files", root),
                    ExplorerAction::Reveal { path } => ("failed to update file explorer", path),
                };
                model.status = Some(StatusMessage::error(format!(
                    "{action} `{}`: {error}",
                    path.display()
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
            state.complete(first.execute()),
            ExplorerApply::Ignored
        ));
        assert!(state.entries.is_empty());
        assert!(matches!(
            state.complete(second.execute()),
            ExplorerApply::Applied(ExplorerAction::Refresh(ExplorerRefreshKind::Manual))
        ));
        assert_eq!(state.entries.len(), 2);
        assert_eq!(state.selected, Some(0));
    }
}
