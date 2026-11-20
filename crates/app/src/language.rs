use std::{error::Error, fmt::Display, path::PathBuf};

use editor::{BufferId, DocumentPosition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigationKind {
    Definition,
    References,
    Implementation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationRequestId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NavigationRequest {
    pub id: NavigationRequestId,
    pub kind: NavigationKind,
    pub buffer: BufferId,
    pub path: PathBuf,
    pub revision: i32,
    pub position: DocumentPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub path: PathBuf,
    pub start: DocumentPosition,
    pub end: DocumentPosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentEvent {
    Open {
        path: PathBuf,
        language: String,
        version: i32,
        text: String,
    },
    Change {
        path: PathBuf,
        version: i32,
        text: String,
    },
    Close {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageError(pub String);

impl Display for LanguageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for LanguageError {}

/// Provider-neutral document synchronization and source navigation capability.
/// Implementations may use LSP, but callers never depend on protocol types.
pub trait LanguageService: Send {
    fn synchronize(&mut self, event: DocumentEvent) -> Result<(), LanguageError>;

    fn navigate(
        &mut self,
        request: NavigationRequest,
    ) -> Result<Vec<SourceLocation>, LanguageError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub path: PathBuf,
    pub start: DocumentPosition,
    pub end: DocumentPosition,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeService {
        events: Vec<DocumentEvent>,
        locations: Vec<SourceLocation>,
    }

    impl LanguageService for FakeService {
        fn synchronize(&mut self, event: DocumentEvent) -> Result<(), LanguageError> {
            self.events.push(event);
            Ok(())
        }

        fn navigate(
            &mut self,
            request: NavigationRequest,
        ) -> Result<Vec<SourceLocation>, LanguageError> {
            assert_eq!(request.kind, NavigationKind::Definition);
            Ok(self.locations.clone())
        }
    }

    #[test]
    fn service_uses_owned_documents_and_provider_neutral_locations() {
        let path = PathBuf::from("/workspace/main.rs");
        let position = DocumentPosition {
            line: 2,
            character: 4,
        };
        let location = SourceLocation {
            path: path.clone(),
            start: position,
            end: DocumentPosition {
                line: 2,
                character: 8,
            },
        };
        let mut service = FakeService {
            events: Vec::new(),
            locations: vec![location.clone()],
        };
        let buffer =
            editor::EditorSession::new().insert_document(editor::DocumentFactory::new().create(""));
        let event = DocumentEvent::Open {
            path: path.clone(),
            language: "rust".to_string(),
            version: 3,
            text: "fn main() {}".to_string(),
        };

        service.synchronize(event.clone()).unwrap();
        let locations = service
            .navigate(NavigationRequest {
                id: NavigationRequestId(1),
                kind: NavigationKind::Definition,
                buffer,
                path,
                revision: 3,
                position,
            })
            .unwrap();

        assert_eq!(service.events, [event]);
        assert_eq!(locations, [location]);
    }
}
