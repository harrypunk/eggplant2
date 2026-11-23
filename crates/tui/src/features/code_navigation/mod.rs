use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

mod component;
mod work;

pub use component::overlay;
pub use work::{close_document, commands, handle_command, synchronize, synchronize_buffer};

use app::language::{
    DocumentEvent, LanguageError, LanguageServerSpec, LanguageService, NavigationKind,
    NavigationRequest, NavigationRequestId, SourceLocation,
};
use editor::{BufferId, DocumentPosition};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationOutcome {
    Ignored,
    Failed(LanguageError),
    NoResults,
    Locations,
}

#[derive(Debug, Clone)]
pub struct PendingSync {
    pub spec: LanguageServerSpec,
    pub event: DocumentEvent,
}

/// Latest-navigation intent and the locations produced by its accepted completion.
#[derive(Default)]
pub struct CodeNavigationState {
    next_id: u64,
    pending: Option<NavigationRequest>,
    locations: Vec<SourceLocation>,
    selected: Option<usize>,
    services: HashMap<String, Box<dyn LanguageService>>,
    operation_spec: Option<LanguageServerSpec>,
    service_failed: bool,
    operation_active: bool,
    trailing_sync: VecDeque<PendingSync>,
}

impl std::fmt::Debug for CodeNavigationState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CodeNavigationState")
            .field("pending", &self.pending)
            .field("locations", &self.locations)
            .field("selected", &self.selected)
            .field("service_count", &self.services.len())
            .field("operation_spec", &self.operation_spec)
            .field("service_failed", &self.service_failed)
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
        self.operation_spec = None;
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
        self.operation_spec = None;
    }

    pub fn queue_sync(&mut self, spec: LanguageServerSpec, event: DocumentEvent) {
        if matches!(&event, DocumentEvent::Close { .. }) {
            self.trailing_sync.push_back(PendingSync { spec, event });
            return;
        }
        let same_target = |pending: &PendingSync| {
            pending.spec.id() == spec.id() && event_path(&pending.event) == event_path(&event)
        };
        let replacement = self.trailing_sync.iter().rposition(|pending| {
            same_target(pending) && !matches!(&pending.event, DocumentEvent::Close { .. })
        });
        if let Some(index) = replacement {
            let close_after = self.trailing_sync.iter().skip(index + 1).any(|pending| {
                same_target(pending) && matches!(&pending.event, DocumentEvent::Close { .. })
            });
            if !close_after {
                self.trailing_sync[index] = PendingSync { spec, event };
                return;
            }
        }
        self.trailing_sync.push_back(PendingSync { spec, event });
    }

    pub fn has_trailing_sync(&self) -> bool {
        !self.trailing_sync.is_empty()
    }

    pub fn take_trailing_sync(&mut self) -> Option<PendingSync> {
        self.trailing_sync.pop_front()
    }

    pub fn take_service_for(
        &mut self,
        spec: &LanguageServerSpec,
    ) -> Option<Box<dyn LanguageService>> {
        self.operation_spec = Some(spec.clone());
        self.service_failed = false;
        self.services.remove(spec.id())
    }

    pub fn restore_service_for(
        &mut self,
        spec: &LanguageServerSpec,
        service: Box<dyn LanguageService>,
    ) {
        if self.service_failed {
            self.service_failed = false;
        } else {
            self.services.insert(spec.id().to_string(), service);
        }
    }

    pub fn invalidate_service_for(&mut self, server_id: &str) {
        self.services.remove(server_id);
        if self.operation_spec.as_ref().map(LanguageServerSpec::id) == Some(server_id) {
            self.service_failed = true;
        }
    }

    pub fn has_service(&self) -> bool {
        !self.services.is_empty() || self.is_busy()
    }

    pub fn has_service_for(&self, spec: &LanguageServerSpec) -> bool {
        self.services.contains_key(spec.id())
            || (self.is_busy()
                && self.operation_spec.as_ref().map(LanguageServerSpec::id) == Some(spec.id()))
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

    struct TestService;

    impl LanguageService for TestService {
        fn synchronize(&mut self, _: DocumentEvent) -> Result<(), LanguageError> {
            Ok(())
        }

        fn navigate(&mut self, _: NavigationRequest) -> Result<Vec<SourceLocation>, LanguageError> {
            Ok(Vec::new())
        }
    }

    fn spec(language: &str, command: &str) -> LanguageServerSpec {
        LanguageServerSpec::new(language, command, std::iter::empty::<&str>())
            .with_extensions(["rs"])
    }

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
    fn services_are_independently_owned_by_server_id() {
        let rust = spec("rust", "rust-analyzer");
        let typescript = spec("typescript", "typescript-language-server");
        let mut state = CodeNavigationState::default();
        state.restore_service_for(&rust, Box::new(TestService));
        state.restore_service_for(&typescript, Box::new(TestService));
        assert!(state.has_service_for(&rust));
        assert!(state.has_service_for(&typescript));

        state.begin_sync();
        assert!(state.take_service_for(&rust).is_some());
        assert!(state.has_service_for(&typescript));
        state.invalidate_service_for(rust.id());
        state.restore_service_for(&rust, Box::new(TestService));
        state.finish_operation();

        assert!(!state.has_service_for(&rust));
        assert!(state.has_service_for(&typescript));
        state.begin_sync();
        assert!(state.take_service_for(&typescript).is_some());
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
        let spec = spec("rust", "rust-analyzer");
        state.queue_sync(
            spec.clone(),
            DocumentEvent::Open {
                path: path("a.rs"),
                language: "rust".to_string(),
                version: 1,
                text: "first".to_string(),
            },
        );
        state.queue_sync(
            spec.clone(),
            DocumentEvent::Change {
                path: path("a.rs"),
                version: 2,
                text: "latest".to_string(),
            },
        );
        state.queue_sync(spec.clone(), DocumentEvent::Close { path: path("a.rs") });
        state.queue_sync(spec, DocumentEvent::Close { path: path("b.rs") });

        let first = state.take_trailing_sync().unwrap();
        assert_eq!(event_path(&first.event), path("a.rs"));
        assert_eq!(
            first.event,
            DocumentEvent::Change {
                path: path("a.rs"),
                version: 2,
                text: "latest".to_string(),
            }
        );
        assert!(state.has_trailing_sync());
        assert_eq!(
            state.take_trailing_sync().map(|pending| pending.event),
            Some(DocumentEvent::Close { path: path("a.rs") })
        );
        assert_eq!(
            state.take_trailing_sync().map(|pending| pending.event),
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
