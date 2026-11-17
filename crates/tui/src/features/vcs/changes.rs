use std::path::PathBuf;

use ::vcs::FileChange;

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

pub(crate) enum ChangesApply {
    Ignored,
    Applied,
    Refresh,
}

#[derive(Debug)]
pub struct ChangesController {
    pub(crate) state: ChangesState,
    pub(crate) selected: Option<usize>,
    pub(super) target: Option<PathBuf>,
    pub(super) revision: u64,
    pub(super) refreshing: bool,
    pub(super) dirty: bool,
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
        if !matches!(self.state, ChangesState::Ready(_)) {
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
