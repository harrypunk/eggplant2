use std::path::PathBuf;

use app::language::{
    LanguageError, NavigationKind, NavigationRequest, NavigationRequestId, SourceLocation,
};
use editor::{BufferId, DocumentPosition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationOutcome {
    Ignored,
    Failed(LanguageError),
    NoResults,
    Locations,
}

/// Latest-navigation intent and the locations produced by its accepted completion.
#[derive(Debug, Default)]
pub struct CodeNavigationState {
    next_id: u64,
    pending: Option<NavigationRequest>,
    locations: Vec<SourceLocation>,
}

impl CodeNavigationState {
    pub fn begin(
        &mut self,
        kind: NavigationKind,
        buffer: BufferId,
        path: PathBuf,
        revision: i32,
        position: DocumentPosition,
    ) -> NavigationRequest {
        self.next_id = self.next_id.wrapping_add(1);
        let request = NavigationRequest {
            id: NavigationRequestId(self.next_id),
            kind,
            buffer,
            path,
            revision,
            position,
        };
        self.pending = Some(request.clone());
        self.locations.clear();
        request
    }

    pub fn complete(
        &mut self,
        request_id: NavigationRequestId,
        buffer: BufferId,
        revision: i32,
        result: Result<Vec<SourceLocation>, LanguageError>,
    ) -> NavigationOutcome {
        let Some(pending) = &self.pending else {
            return NavigationOutcome::Ignored;
        };
        if pending.id != request_id || pending.buffer != buffer || pending.revision != revision {
            return NavigationOutcome::Ignored;
        }
        self.pending = None;

        match result {
            Err(error) => {
                self.locations.clear();
                NavigationOutcome::Failed(error)
            }
            Ok(locations) if locations.is_empty() => {
                self.locations.clear();
                NavigationOutcome::NoResults
            }
            Ok(locations) => {
                self.locations = locations;
                NavigationOutcome::Locations
            }
        }
    }

    pub fn locations(&self) -> &[SourceLocation] {
        &self.locations
    }
}

#[cfg(test)]
mod tests {
    use editor::EditorSession;

    use super::*;

    fn location() -> SourceLocation {
        SourceLocation {
            path: PathBuf::from("/workspace/main.rs"),
            start: DocumentPosition {
                line: 1,
                character: 2,
            },
            end: DocumentPosition {
                line: 1,
                character: 4,
            },
        }
    }

    #[test]
    fn only_the_latest_matching_completion_replaces_locations() {
        let mut state = CodeNavigationState::default();
        let buffer = EditorSession::new()
            .insert_document(editor::DocumentFactory::new().create("fn main() {}"));
        let first = state.begin(
            NavigationKind::Definition,
            buffer,
            PathBuf::from("/workspace/main.rs"),
            1,
            DocumentPosition {
                line: 0,
                character: 0,
            },
        );
        let second = state.begin(
            NavigationKind::References,
            buffer,
            PathBuf::from("/workspace/main.rs"),
            1,
            DocumentPosition {
                line: 0,
                character: 0,
            },
        );
        assert_ne!(first.id, second.id);

        assert_eq!(
            state.complete(first.id, buffer, 1, Ok(Vec::new())),
            NavigationOutcome::Ignored
        );
        assert_eq!(
            state.complete(second.id, buffer, 1, Ok(vec![location()])),
            NavigationOutcome::Locations
        );
        assert_eq!(state.locations(), &[location()]);

        let third = state.begin(
            NavigationKind::Implementation,
            buffer,
            PathBuf::from("/workspace/main.rs"),
            1,
            DocumentPosition {
                line: 0,
                character: 0,
            },
        );
        assert_eq!(
            state.complete(third.id, buffer, 1, Ok(Vec::new())),
            NavigationOutcome::NoResults
        );
        assert!(state.locations().is_empty());
    }

    #[test]
    fn wrong_revision_is_ignored_and_current_failure_clears_locations() {
        let mut state = CodeNavigationState::default();
        let buffer =
            EditorSession::new().insert_document(editor::DocumentFactory::new().create(""));
        state.locations = vec![location()];
        let request = state.begin(
            NavigationKind::Definition,
            buffer,
            PathBuf::from("/workspace/main.rs"),
            4,
            DocumentPosition {
                line: 0,
                character: 0,
            },
        );

        assert_eq!(
            state.complete(request.id, buffer, 3, Ok(Vec::new())),
            NavigationOutcome::Ignored
        );
        assert_eq!(state.pending, Some(request.clone()));
        let error = LanguageError("request failed".to_string());
        assert_eq!(
            state.complete(request.id, buffer, 4, Err(error.clone())),
            NavigationOutcome::Failed(error)
        );
        assert!(state.pending.is_none());
        assert!(state.locations.is_empty());
    }
}
