use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use ::vcs::{Diff, FileChange};

use super::DiffState;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum ChangesState {
    #[default]
    Idle,
    Loading,
    Ready(Vec<FileChange>),
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct ChangesRequest {
    pub target: PathBuf,
    pub revision: u64,
}

pub(crate) struct DiffRequest {
    pub change: FileChange,
    pub target: PathBuf,
    pub revision: u64,
}

pub(crate) enum DiffApply {
    Ignored,
    Applied,
}

pub(crate) enum ChangesApply {
    Ignored,
    Applied,
    Refresh,
}

#[derive(Debug)]
struct IndexOperation {
    target: PathBuf,
    revision: u64,
}

#[derive(Debug)]
pub struct ChangesController {
    pub(crate) state: ChangesState,
    pub(crate) selected: Option<usize>,
    pub(super) target: Option<PathBuf>,
    pub(super) revision: u64,
    pub(super) refreshing: bool,
    pub(super) dirty: bool,
    operation: Option<IndexOperation>,
    operation_revision: u64,
    pub(crate) expanded: Option<(PathBuf, bool)>,
    pub(crate) diff: DiffState,
    pub(super) diff_target: Option<PathBuf>,
    pub(super) diff_revision: u64,
    diff_cache: HashMap<(PathBuf, bool), Diff>,
}

impl Default for ChangesController {
    fn default() -> Self {
        Self {
            state: ChangesState::Idle,
            selected: None,
            target: None,
            revision: 0,
            refreshing: false,
            dirty: false,
            operation: None,
            operation_revision: 0,
            expanded: None,
            diff: DiffState::Idle,
            diff_target: None,
            diff_revision: 0,
            diff_cache: HashMap::new(),
        }
    }
}

impl ChangesController {
    pub fn state(&self) -> &ChangesState {
        &self.state
    }

    pub fn selected_change(&self) -> Option<&FileChange> {
        let ChangesState::Ready(changes) = &self.state else {
            return None;
        };
        changes.get(self.selected?)
    }

    pub(crate) fn expanded(&self) -> Option<(&Path, bool)> {
        self.expanded
            .as_ref()
            .map(|(path, staged)| (path.as_path(), *staged))
    }

    pub(crate) fn diff(&self) -> &DiffState {
        &self.diff
    }

    pub(super) fn invalidate_diff(&mut self) {
        self.expanded = None;
        self.diff = DiffState::Idle;
        self.diff_cache.clear();
        self.diff_revision = self.diff_revision.wrapping_add(1);
    }

    pub(super) fn toggle_diff(
        &mut self,
        change: FileChange,
        target: Option<PathBuf>,
    ) -> Option<DiffRequest> {
        let expanded = (change.path.clone(), change.staged);
        if self.expanded.as_ref() == Some(&expanded) {
            self.expanded = None;
            self.diff = DiffState::Idle;
            self.diff_revision = self.diff_revision.wrapping_add(1);
            return None;
        }
        if let Some(diff) = self.diff_cache.get(&expanded).cloned() {
            self.expanded = Some(expanded);
            self.diff = DiffState::Ready {
                lines: diff.lines,
                truncated: diff.truncated,
            };
            return None;
        }
        let target = target?;
        self.expanded = Some(expanded);
        self.diff = DiffState::Loading;
        self.diff_target = Some(target.clone());
        self.diff_revision = self.diff_revision.wrapping_add(1);
        Some(DiffRequest {
            change,
            target,
            revision: self.diff_revision,
        })
    }

    /// Apply a worker result only when it still belongs to the visible diff request.
    /// Successful diffs are cached by path and staged side for collapse/reopen.
    pub(super) fn complete_diff(
        &mut self,
        request: DiffRequest,
        result: Result<Diff, String>,
    ) -> DiffApply {
        let expanded = (request.change.path.clone(), request.change.staged);
        if self.diff_revision != request.revision
            || self.diff_target.as_ref() != Some(&request.target)
            || self.expanded.as_ref() != Some(&expanded)
        {
            return DiffApply::Ignored;
        }
        self.diff = match result {
            Ok(diff) => {
                self.diff_cache.insert(expanded, diff.clone());
                DiffState::Ready {
                    lines: diff.lines,
                    truncated: diff.truncated,
                }
            }
            Err(error) => DiffState::Failed(error),
        };
        DiffApply::Applied
    }

    pub(super) fn begin_operation(&mut self, target: PathBuf) -> Option<u64> {
        if self.operation.is_some() {
            return None;
        }
        self.invalidate();
        self.operation_revision = self.operation_revision.wrapping_add(1);
        self.operation = Some(IndexOperation {
            target,
            revision: self.operation_revision,
        });
        Some(self.operation_revision)
    }

    pub(super) fn accept_operation(&mut self, target: &std::path::Path, revision: u64) -> bool {
        let matches = self.operation.as_ref().is_some_and(|operation| {
            operation.revision == revision && operation.target.as_path() == target
        });
        if !matches {
            return false;
        }
        self.operation = None;
        true
    }

    pub(crate) fn operation_in_progress(&self) -> bool {
        self.operation.is_some()
    }

    #[cfg(test)]
    pub(super) fn operation_revision(&self) -> u64 {
        self.operation_revision
    }

    pub(super) fn refresh(&mut self, target: Option<PathBuf>) -> Option<ChangesRequest> {
        let Some(target) = target else {
            self.target = None;
            self.state = ChangesState::Failed("not a Git repository".to_string());
            self.selected = None;
            self.refreshing = false;
            self.dirty = false;
            self.revision = self.revision.wrapping_add(1);
            return None;
        };
        if self.target.as_ref() == Some(&target) && self.refreshing {
            return None;
        }
        self.target = Some(target.clone());
        self.refreshing = true;
        if let ChangesState::Idle | ChangesState::Loading | ChangesState::Failed(_) = self.state {
            self.state = ChangesState::Loading;
        }
        self.revision = self.revision.wrapping_add(1);
        Some(ChangesRequest {
            target,
            revision: self.revision,
        })
    }

    pub(super) fn sync_in_flight(&self, target: &Option<PathBuf>) -> bool {
        self.refreshing && self.target == *target
    }

    pub(super) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub(super) fn cancel(&mut self) {
        self.refreshing = false;
        self.dirty = false;
    }

    pub(super) fn finish_operation(&mut self) {
        self.refreshing = false;
    }

    pub(super) fn invalidate(&mut self) {
        self.cancel();
        self.revision = self.revision.wrapping_add(1);
    }

    pub(super) fn complete(
        &mut self,
        target: PathBuf,
        revision: u64,
        result: Result<Vec<FileChange>, String>,
    ) -> ChangesApply {
        if self.revision != revision || self.target.as_ref() != Some(&target) {
            return ChangesApply::Ignored;
        }
        let selected_path = self.selected_change().map(|change| change.path.clone());
        self.refreshing = false;
        self.state = match result {
            Ok(changes) => {
                self.selected = selected_path
                    .and_then(|path| changes.iter().position(|change| change.path == path))
                    .or_else(|| (!changes.is_empty()).then_some(0));
                ChangesState::Ready(changes)
            }
            Err(error) => {
                self.selected = None;
                ChangesState::Failed(error)
            }
        };
        if self.dirty {
            self.dirty = false;
            ChangesApply::Refresh
        } else {
            ChangesApply::Applied
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use ::vcs::ChangeKind;

    use super::*;

    fn change(path: &str) -> FileChange {
        FileChange {
            kind: ChangeKind::Modified,
            staged: false,
            path: PathBuf::from(path),
            previous_path: None,
        }
    }

    #[test]
    fn index_operation_is_single_flight_and_released_by_matching_completion() {
        let root = PathBuf::from("/workspace");
        let mut controller = ChangesController::default();
        let revision = controller.begin_operation(root.clone()).unwrap();

        assert!(controller.begin_operation(root.clone()).is_none());
        assert!(!controller.accept_operation(Path::new("/other"), revision));
        assert!(!controller.accept_operation(&root, revision.wrapping_sub(1)));
        assert!(controller.operation_in_progress());
        assert!(controller.accept_operation(&root, revision));
        assert!(!controller.operation_in_progress());
        assert!(controller.begin_operation(root).unwrap() > revision);
    }

    #[test]
    fn working_diff_validates_completion_and_reuses_cache() {
        let root = PathBuf::from("/workspace");
        let selected = change("/workspace/note.txt");
        let mut controller = ChangesController::default();
        let request = controller
            .toggle_diff(selected.clone(), Some(root.clone()))
            .unwrap();
        assert!(matches!(
            controller.complete_diff(
                DiffRequest {
                    change: selected.clone(),
                    target: root.clone(),
                    revision: request.revision.wrapping_sub(1),
                },
                Ok(Diff {
                    lines: vec!["stale".to_string()],
                    truncated: false,
                })
            ),
            DiffApply::Ignored
        ));
        assert_eq!(controller.diff(), &DiffState::Loading);

        assert!(matches!(
            controller.complete_diff(
                DiffRequest {
                    change: selected.clone(),
                    target: root.clone(),
                    revision: request.revision,
                },
                Ok(Diff {
                    lines: vec!["+new".to_string()],
                    truncated: false,
                })
            ),
            DiffApply::Applied
        ));
        assert!(matches!(controller.diff(), DiffState::Ready { .. }));
        assert!(
            controller
                .toggle_diff(selected.clone(), Some(root.clone()))
                .is_none()
        );
        assert_eq!(controller.expanded(), None);
        assert!(controller.toggle_diff(selected, Some(root)).is_none());
        assert!(matches!(controller.diff(), DiffState::Ready { .. }));

        controller.invalidate_diff();
        assert_eq!(controller.expanded(), None);
        assert_eq!(controller.diff(), &DiffState::Idle);
    }

    #[test]
    fn refresh_completion_preserves_selection_and_requests_trailing_work() {
        let root = PathBuf::from("/workspace");
        let path = root.join("note.txt");
        let mut controller = ChangesController {
            state: ChangesState::Ready(vec![FileChange {
                path: path.clone(),
                ..change("/old")
            }]),
            selected: Some(0),
            ..ChangesController::default()
        };
        let request = controller.refresh(Some(root.clone())).unwrap();
        controller.mark_dirty();

        assert!(matches!(
            controller.complete(
                root.clone(),
                request.revision.wrapping_sub(1),
                Ok(Vec::new())
            ),
            ChangesApply::Ignored
        ));
        assert!(matches!(
            controller.complete(
                root.clone(),
                request.revision,
                Ok(vec![FileChange {
                    path: path.clone(),
                    ..change("/unused")
                }])
            ),
            ChangesApply::Refresh
        ));
        assert_eq!(controller.selected_change().unwrap().path, path);

        let trailing = controller.refresh(Some(root)).unwrap();
        assert!(matches!(
            controller.complete(trailing.target, trailing.revision, Ok(Vec::new())),
            ChangesApply::Applied
        ));
    }
}
