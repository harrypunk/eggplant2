use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use arc_swap::ArcSwap;
use helix_core::{Rope, config::default_lang_loader, syntax};
use helix_view::{
    Document, document::DocumentOpenError, editor::Config, theme::Loader as ThemeLoader,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenDocumentError {
    #[error("not a text file")]
    NotTextFile,
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Helix(#[from] DocumentOpenError),
}

pub(crate) struct PreparedDocument {
    path: PathBuf,
    pub(crate) text: Rope,
    pub(crate) language: Option<Arc<syntax::config::LanguageConfiguration>>,
    pub(crate) syntax: Option<syntax::Syntax>,
    pub(crate) existed: bool,
}

type SharedConfig = Arc<ArcSwap<Config>>;
pub(crate) type SharedSyntaxLoader = Arc<ArcSwap<syntax::Loader>>;

static DEFAULT_SYNTAX_LOADER: OnceLock<SharedSyntaxLoader> = OnceLock::new();

pub(crate) fn default_syntax_loader() -> SharedSyntaxLoader {
    DEFAULT_SYNTAX_LOADER
        .get_or_init(|| {
            let loader = default_lang_loader();
            let theme = ThemeLoader::new(&[]).default();
            loader.set_scopes(theme.scopes().to_vec());
            Arc::new(ArcSwap::from_pointee(loader))
        })
        .clone()
}

/// Read a regular UTF-8 text file without following links or accepting NUL bytes.
pub fn read_text_file(path: &Path) -> Result<String, OpenDocumentError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() {
        return Err(OpenDocumentError::NotTextFile);
    }
    let bytes = std::fs::read(path)?;
    let text = String::from_utf8(bytes).map_err(|_| OpenDocumentError::NotTextFile)?;
    if text.contains('\0') {
        return Err(OpenDocumentError::NotTextFile);
    }
    Ok(text)
}

/// Owns Helix runtime resources needed to construct documents.
#[derive(Clone)]
pub struct DocumentFactory {
    config: SharedConfig,
    syntax_loader: SharedSyntaxLoader,
}

impl DocumentFactory {
    pub fn new() -> Self {
        Self {
            // ArcSwap stores an Arc<Config> snapshot. The outer Arc shares the
            // swap cell, allowing later config reloads without lock-based mutation.
            config: Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            syntax_loader: default_syntax_loader(),
        }
    }

    pub fn create(&self, text: &str) -> Document {
        Document::from(
            Rope::from(text),
            None,
            self.config.clone(),
            self.syntax_loader.clone(),
        )
    }

    /// Open a regular UTF-8 text file, or create a document for a missing path.
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Document, OpenDocumentError> {
        let prepared = self.prepare_open(path.as_ref())?;
        Ok(self.build_document(prepared))
    }

    /// Read and parse on any thread; construct the non-Send Document on the caller's thread.
    pub(crate) fn prepare_open(&self, path: &Path) -> Result<PreparedDocument, OpenDocumentError> {
        let (text, existed) = match read_text_file(path) {
            Ok(text) => (Rope::from(text), true),
            Err(OpenDocumentError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                let line_ending: helix_core::LineEnding =
                    self.config.load().default_line_ending.into();
                (Rope::from(line_ending.as_str()), false)
            }
            Err(error) => return Err(error),
        };
        let path = helix_stdx::path::canonicalize(path);
        let loader = self.syntax_loader.load();
        let source = text.slice(..);
        let language = loader
            .language_for_filename(&path)
            .or_else(|| loader.language_for_shebang(source))
            .map(|language| loader.language(language).config().clone());
        let syntax = language
            .as_ref()
            .and_then(|config| syntax::Syntax::new(source, config.language(), &loader).ok());

        Ok(PreparedDocument {
            path,
            text,
            language,
            syntax,
            existed,
        })
    }

    pub(crate) fn build_document(&self, prepared: PreparedDocument) -> Document {
        let mut document = Document::from(
            prepared.text,
            Some((helix_core::encoding::UTF_8, false)),
            self.config.clone(),
            self.syntax_loader.clone(),
        );
        document.set_path(Some(&prepared.path));
        document.language = prepared.language;
        document.syntax = prepared.syntax;
        document.detect_indent_and_line_ending();
        document
    }
}

impl Default for DocumentFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for DocumentFactory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DocumentFactory")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_binary_invalid_utf8_and_links() {
        let directory = tempfile::tempdir().unwrap();
        let text = directory.path().join("text.txt");
        let binary = directory.path().join("binary.bin");
        let invalid_utf8 = directory.path().join("invalid.txt");
        let link = directory.path().join("link.txt");
        std::fs::write(&text, "text").unwrap();
        std::fs::write(&binary, [b'a', 0, b'b']).unwrap();
        std::fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();
        std::os::unix::fs::symlink(&text, &link).unwrap();
        let factory = DocumentFactory::new();

        for path in [binary, invalid_utf8, link] {
            assert!(matches!(
                factory.open(path),
                Err(OpenDocumentError::NotTextFile)
            ));
        }
        assert!(factory.open(text).is_ok());
    }
}
