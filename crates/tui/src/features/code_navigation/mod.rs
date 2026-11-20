use std::{collections::VecDeque, path::PathBuf};

mod component;
mod work;

pub use component::overlay;
pub use work::{close_document, handle_command, register_commands, synchronize};

use app::language::{
    DocumentEvent, LanguageError, LanguageService, NavigationKind, NavigationRequest,
    NavigationRequestId, SourceLocation,
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
#[derive(Default)]
pub struct CodeNavigationState {
    next_id: u64,
    pending: Option<NavigationRequest>,
    locations: Vec<SourceLocation>,
    selected: Option<usize>,
    service: Option<Box<dyn LanguageService>>,
    operation_active: bool,
    trailing_sync: VecDeque<DocumentEvent>,
}

impl std::fmt::Debug for CodeNavigationState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodeNavigationState")
            .field("pending", &self.pending)
            .field("locations", &self.locations)
            .field("selected", &self.selected)
            .field("has_service", &self.service.is_some())
            .field("operation_active", &self.operation_active)
            .field("has_trailing_sync", &!self.trailing_sync.is_empty())
            .finish()
    }
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
        self.operation_active = true;
        self.locations.clear();
        self.selected = None;
        request
    }

    pub fn complete(
        &mut self,
        request_id: NavigationRequestId,
        buffer: BufferId,
        revision: i32,
        result: Result<Vec<SourceLocation>, LanguageError>,
    ) -> NavigationOutcome {
        let Some(pending) = self.pending.clone() else {
            return NavigationOutcome::Ignored;
        };
        if pending.id != request_id {
            return NavigationOutcome::Ignored;
        }
        self.pending = None;
        self.operation_active = false;
        if pending.buffer != buffer || pending.revision != revision {
            return NavigationOutcome::Ignored;
        }

        match result {
            Err(error) => {
                self.locations.clear();
                self.selected = None;
                NavigationOutcome::Failed(error)
            }
            Ok(locations) if locations.is_empty() => {
                self.locations.clear();
                self.selected = None;
                NavigationOutcome::NoResults
            }
            Ok(locations) => {
                self.locations = locations;
                self.selected = Some(0);
                NavigationOutcome::Locations
            }
        }
    }

    pub fn locations(&self) -> &[SourceLocation] {
        &self.locations
    }

    pub fn is_busy(&self) -> bool {
        self.operation_active
    }

    pub fn begin_sync(&mut self) {
        self.operation_active = true;
    }

    pub fn finish_operation(&mut self) {
        self.operation_active = false;
    }

    pub fn queue_sync(&mut self, event: DocumentEvent) {
        let replace = match (&event, self.trailing_sync.back()) {
            (DocumentEvent::Close { .. }, _) | (_, Some(DocumentEvent::Close { .. })) => false,
            (_, None) => false,
            (event, Some(previous)) => event_path(event) == event_path(previous),
        };
        if replace {
            *self
                .trailing_sync
                .back_mut()
                .expect("queued event must exist") = event;
        } else {
            self.trailing_sync.push_back(event);
        }
    }

    pub fn has_trailing_sync(&self) -> bool {
        !self.trailing_sync.is_empty()
    }

    pub fn take_trailing_sync(&mut self) -> Option<DocumentEvent> {
        self.trailing_sync.pop_front()
    }

    pub fn take_service(&mut self) -> Option<Box<dyn LanguageService>> {
        self.service.take()
    }

    pub fn restore_service(&mut self, service: Box<dyn LanguageService>) {
        self.service = Some(service);
    }

    pub fn has_service(&self) -> bool {
        self.service.is_some() || self.is_busy()
    }

    pub fn selected_location(&self) -> Option<&SourceLocation> {
        self.locations.get(self.selected?)
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn select_next(&mut self) {
        if !self.locations.is_empty() {
            self.selected = Some((self.selected.unwrap_or(0) + 1) % self.locations.len());
        }
    }

    pub fn select_previous(&mut self) {
        if !self.locations.is_empty() {
            self.selected = Some(
                (self.selected.unwrap_or(0) + self.locations.len() - 1) % self.locations.len(),
            );
        }
    }

    pub fn clear_locations(&mut self) {
        self.locations.clear();
        self.selected = None;
    }
}

fn event_path(event: &DocumentEvent) -> &std::path::Path {
    match event {
        DocumentEvent::Open { path, .. }
        | DocumentEvent::Change { path, .. }
        | DocumentEvent::Close { path } => path,
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
    fn close_events_are_not_coalesced_away() {
        let mut state = CodeNavigationState::default();
        let path = |name| PathBuf::from(name);
        state.queue_sync(DocumentEvent::Open {
            path: path("a.rs"),
            language: "rust".to_string(),
            version: 1,
            text: "first".to_string(),
        });
        state.queue_sync(DocumentEvent::Change {
            path: path("a.rs"),
            version: 2,
            text: "latest".to_string(),
        });
        state.queue_sync(DocumentEvent::Close { path: path("a.rs") });
        state.queue_sync(DocumentEvent::Close { path: path("b.rs") });

        let first = state.take_trailing_sync().unwrap();
        assert_eq!(event_path(&first), path("a.rs"));
        assert_eq!(
            first,
            DocumentEvent::Change {
                path: path("a.rs"),
                version: 2,
                text: "latest".to_string(),
            }
        );
        assert!(state.has_trailing_sync());
        assert_eq!(
            state.take_trailing_sync(),
            Some(DocumentEvent::Close { path: path("a.rs") })
        );
        assert_eq!(
            state.take_trailing_sync(),
            Some(DocumentEvent::Close { path: path("b.rs") })
        );
        assert!(!state.has_trailing_sync());
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
        assert_eq!(state.pending, None);
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
        let error = LanguageError("request failed".to_string());
        assert_eq!(
            state.complete(request.id, buffer, 4, Err(error.clone())),
            NavigationOutcome::Failed(error)
        );
        assert!(state.pending.is_none());
        assert!(state.locations.is_empty());
    }
}
