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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageServerSpec {
    language: String,
    command: String,
    args: Vec<String>,
    extensions: Vec<String>,
}

impl LanguageServerSpec {
    pub fn new<I, S>(language: &str, command: &str, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            language: language.to_string(),
            command: command.to_string(),
            args: args.into_iter().map(Into::into).collect(),
            extensions: Vec::new(),
        }
    }

    pub fn with_extensions<I, S>(mut self, extensions: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extensions = extensions.into_iter().map(Into::into).collect();
        self
    }

    pub fn matches_path(&self, path: &std::path::Path) -> bool {
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                self.extensions
                    .iter()
                    .any(|candidate| candidate == extension)
            })
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }
}

#[derive(Debug, Default)]
pub struct LanguageServers {
    specs: Vec<LanguageServerSpec>,
}

/// Provider-neutral lookup used by application features instead of hard-coded catalogs.
pub trait LanguageServerRegistry: std::fmt::Debug + Send + Sync {
    fn find_for_path(&self, path: &std::path::Path) -> Option<LanguageServerSpec>;
}

impl LanguageServers {
    pub fn defaults() -> Self {
        Self {
            specs: vec![
                LanguageServerSpec::new("rust", "rust-analyzer", std::iter::empty::<&str>())
                    .with_extensions(["rs"]),
                LanguageServerSpec::new("typescript", "typescript-language-server", ["--stdio"])
                    .with_extensions(["ts", "mts", "cts"]),
                LanguageServerSpec::new(
                    "typescriptreact",
                    "typescript-language-server",
                    ["--stdio"],
                )
                .with_extensions(["tsx"]),
                LanguageServerSpec::new("javascript", "typescript-language-server", ["--stdio"])
                    .with_extensions(["js", "mjs", "cjs"]),
                LanguageServerSpec::new(
                    "javascriptreact",
                    "typescript-language-server",
                    ["--stdio"],
                )
                .with_extensions(["jsx"]),
                LanguageServerSpec::new("python", "pyright-langserver", ["--stdio"])
                    .with_extensions(["py", "pyi"]),
            ],
        }
    }

    pub fn register(&mut self, spec: LanguageServerSpec) -> Result<(), LanguageError> {
        if self
            .specs
            .iter()
            .any(|current| current.language == spec.language)
        {
            return Err(LanguageError(format!(
                "duplicate language server for `{}`",
                spec.language
            )));
        }
        self.specs.push(spec);
        Ok(())
    }

    pub fn find(&self, language: &str) -> Option<&LanguageServerSpec> {
        self.specs.iter().find(|spec| spec.language == language)
    }

    pub fn find_for_path(&self, path: &std::path::Path) -> Option<&LanguageServerSpec> {
        self.specs.iter().find(|spec| spec.matches_path(path))
    }
}

impl LanguageServerRegistry for LanguageServers {
    fn find_for_path(&self, path: &std::path::Path) -> Option<LanguageServerSpec> {
        self.specs
            .iter()
            .find(|spec| spec.matches_path(path))
            .cloned()
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticBatch {
    pub path: PathBuf,
    pub version: Option<i32>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageMessageKind {
    Error,
    Warning,
    Information,
    Log,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageMessage {
    pub kind: LanguageMessageKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanguageProgressStage {
    Begin,
    Report,
    End,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageProgress {
    pub stage: LanguageProgressStage,
    pub title: String,
    pub message: Option<String>,
    pub percentage: Option<u32>,
}

/// Provider-neutral events delivered independently of navigation responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageEvent {
    Diagnostics(DiagnosticBatch),
    Message(LanguageMessage),
    Progress(LanguageProgress),
    /// Terminal runtime failure; no further events will be emitted.
    Failure(LanguageError),
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

    #[test]
    fn community_defaults_select_external_servers_and_reject_duplicates() {
        let mut servers = LanguageServers::defaults();
        let rust = servers.find("rust").unwrap();
        assert_eq!(rust.command(), "rust-analyzer");
        assert!(rust.args().is_empty());
        assert!(servers.find("unknown").is_none());
        for (path, language) in [
            ("main.rs", "rust"),
            ("main.ts", "typescript"),
            ("main.mts", "typescript"),
            ("main.cts", "typescript"),
            ("component.tsx", "typescriptreact"),
            ("main.js", "javascript"),
            ("main.mjs", "javascript"),
            ("main.cjs", "javascript"),
            ("component.jsx", "javascriptreact"),
            ("main.py", "python"),
            ("main.pyi", "python"),
        ] {
            let spec = servers.find_for_path(std::path::Path::new(path)).unwrap();
            assert_eq!(spec.language(), language, "wrong language for {path}");
            let command = if language == "python" {
                "pyright-langserver"
            } else if language == "rust" {
                "rust-analyzer"
            } else {
                "typescript-language-server"
            };
            assert_eq!(spec.command(), command, "wrong command for {path}");
        }
        assert_eq!(
            servers.find("typescript").unwrap().args(),
            &[String::from("--stdio")]
        );
        assert_eq!(
            servers.find("python").unwrap().args(),
            &[String::from("--stdio")]
        );
        assert!(
            servers
                .find_for_path(std::path::Path::new("main.txt"))
                .is_none()
        );

        let duplicate =
            LanguageServerSpec::new("rust", "another-server", std::iter::empty::<&str>());
        let error = servers.register(duplicate).unwrap_err();
        assert!(error.0.contains("duplicate language server"));
        assert_eq!(servers.find("rust").unwrap().command(), "rust-analyzer");
    }

    #[test]
    fn registry_accepts_owned_configuration_through_the_abstract_interface() {
        let mut servers = LanguageServers::default();
        let language = String::from("typescript");
        let command = String::from("typescript-language-server");
        servers
            .register(
                LanguageServerSpec::new(&language, &command, [String::from("--stdio")])
                    .with_extensions([String::from("ts"), String::from("tsx")]),
            )
            .unwrap();

        let spec =
            LanguageServerRegistry::find_for_path(&servers, std::path::Path::new("component.tsx"))
                .unwrap();
        assert_eq!(spec.language(), "typescript");
        assert_eq!(spec.command(), "typescript-language-server");
        assert_eq!(spec.args(), &[String::from("--stdio")]);
    }
}
