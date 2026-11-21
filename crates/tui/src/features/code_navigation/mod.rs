use std::{collections::VecDeque, path::PathBuf};

mod component;
mod work;

pub use component::overlay;
pub use work::{close_document, handle_command, register_commands, synchronize};

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

/// Latest-navigation intent and the locations produced by its accepted completion.
#[derive(Default)]
pub struct CodeNavigationState {
    next_id: u64,
    pending: Option<NavigationRequest>,
    locations: Vec<SourceLocation>,
    selected: Option<usize>,
    service: Option<Box<dyn LanguageService>>,
    service_spec: Option<LanguageServerSpec>,
    operation_spec: Option<LanguageServerSpec>,
    service_failed: bool,
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
            .field("service_spec", &self.service_spec)
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

    pub fn take_service_for(
        &mut self,
        spec: &LanguageServerSpec,
    ) -> Option<Box<dyn LanguageService>> {
        if self
            .service_spec
            .as_ref()
            .is_some_and(|current| current != spec)
        {
            self.service = None;
        }
        self.service_spec = None;
        self.operation_spec = Some(spec.clone());
        self.service_failed = false;
        self.service.take()
    }

    pub fn restore_service(&mut self, service: Box<dyn LanguageService>) {
        self.restore_with_spec(service, None);
    }

    pub fn restore_service_for(
        &mut self,
        spec: &LanguageServerSpec,
        service: Box<dyn LanguageService>,
    ) {
        self.restore_with_spec(service, Some(spec.clone()));
    }

    fn restore_with_spec(
        &mut self,
        service: Box<dyn LanguageService>,
        spec: Option<LanguageServerSpec>,
    ) {
        if self.service_failed {
            self.service_failed = false;
            self.service_spec = None;
        } else {
            self.service = Some(service);
            self.service_spec = spec;
        }
    }

    pub fn invalidate_service(&mut self) {
        self.service = None;
        self.service_spec = None;
        self.service_failed = true;
    }

    pub fn has_service(&self) -> bool {
        self.service.is_some() || self.is_busy()
    }

    pub fn has_service_for(&self, spec: &LanguageServerSpec) -> bool {
        if self.is_busy() {
            return self.operation_spec.as_ref() == Some(spec);
        }
        self.service.is_some()
            && self
                .service_spec
                .as_ref()
                .is_none_or(|current| current == spec)
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
    fn service_reuse_requires_the_same_server_specification() {
        let rust = spec("rust", "rust-analyzer");
        let same = rust.clone();
        let typescript = spec("typescript", "typescript-language-server");
        let changed_command = spec("rust", "another-rust-server");
        let mut state = CodeNavigationState::default();
        state.restore_service_for(&rust, Box::new(TestService));
        assert!(state.has_service_for(&same));
        assert!(!state.has_service_for(&typescript));

        state.begin_sync();
        let service = state.take_service_for(&same).unwrap();
        state.restore_service_for(&same, service);
        state.finish_operation();

        for mismatch in [typescript, changed_command] {
            state.restore_service_for(&rust, Box::new(TestService));
            state.begin_sync();
            assert!(state.take_service_for(&mismatch).is_none());
            state.finish_operation();
            assert!(!state.has_service());
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
