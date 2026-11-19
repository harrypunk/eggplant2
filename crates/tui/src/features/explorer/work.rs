use std::{io, path::PathBuf};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{ExplorerEntry, ExplorerState, IgnoreMode, TreeChange, fs};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExplorerRefreshKind {
    Manual,
    External,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExplorerAction {
    Refresh(ExplorerRefreshKind),
    Expansion { path: PathBuf, depth: usize },
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
        let result = match &self.action {
            ExplorerAction::Expansion { path, depth } => fs::read_directory(
                path,
                depth + 1,
                &self.expanded_directories,
                self.ignore_mode,
            ),
            _ => fs::read_visible_entries(&self.root, &self.expanded_directories, self.ignore_mode),
        };
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
        if let ExplorerAction::Expansion { .. } = &request.action {
            if self.expand_directory(&request, entries) == TreeChange::Unchanged {
                return ExplorerApply::Unchanged;
            }
            return ExplorerApply::Applied(request.action);
        }
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
                    ExplorerAction::Expansion { path, .. } => {
                        ("failed to update directory tree", path)
                    }
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
    fn folding_is_immediate_and_invalidates_pending_refreshes() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("child.txt"), "child").unwrap();
        let root = directory.path().to_path_buf();
        let mut state = ExplorerState::load(&root).unwrap();
        assert_eq!(state.unfold_selected(&root).unwrap(), TreeChange::Changed);
        let refresh = state.reload_request(&root, ExplorerRefreshKind::Manual);

        assert_eq!(state.fold_selected(), TreeChange::Changed);
        let collapsed = state.entries.clone();
        assert_eq!(collapsed.len(), 1);
        assert!(matches!(
            state.complete(refresh.execute()),
            ExplorerApply::Ignored
        ));
        assert_eq!(state.entries, collapsed);
    }

    #[test]
    fn unfold_reads_only_selected_subtree_and_preserves_nested_expansion() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("a");
        let nested = first.join("nested");
        let grandchild = nested.join("grandchild.txt");
        let second = directory.path().join("b");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(&grandchild, "grandchild").unwrap();
        std::fs::create_dir(&second).unwrap();
        std::fs::write(second.join("child.txt"), "child").unwrap();
        let root = directory.path().to_path_buf();
        let mut state = ExplorerState::load(&root).unwrap();

        assert_eq!(state.unfold_selected(&root).unwrap(), TreeChange::Changed);
        state.selected = state.entries.iter().position(|entry| entry.path == nested);
        assert_eq!(state.unfold_selected(&root).unwrap(), TreeChange::Changed);
        state.selected = state.entries.iter().position(|entry| entry.path == second);
        assert_eq!(state.unfold_selected(&root).unwrap(), TreeChange::Changed);
        state.selected = state.entries.iter().position(|entry| entry.path == first);
        assert_eq!(state.fold_selected(), TreeChange::Changed);

        // A full visible-tree rebuild would fail while reading this other expanded directory.
        std::fs::remove_dir_all(&second).unwrap();
        let request = state.unfold_request(&root).unwrap();
        assert!(matches!(
            state.complete(request.execute()),
            ExplorerApply::Applied(ExplorerAction::Expansion { .. })
        ));
        let nested_entry = state
            .entries
            .iter()
            .find(|entry| entry.path == nested)
            .unwrap();
        assert_eq!(
            nested_entry.expansion,
            super::super::ExpansionState::Expanded
        );
        assert!(state.entries.iter().any(|entry| entry.path == grandchild));
        assert!(state.entries.iter().any(|entry| entry.path == second));
    }

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
