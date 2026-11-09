use std::{
    fmt,
    path::Path,
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
    ///
    /// Links, directories, binary content, and invalid UTF-8 are rejected before
    /// Helix constructs a document. Every application open path uses this rule.
    pub fn open(&self, path: impl AsRef<Path>) -> Result<Document, OpenDocumentError> {
        let path = path.as_ref();
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => {
                if !metadata.file_type().is_file() {
                    return Err(OpenDocumentError::NotTextFile);
                }
                let bytes = std::fs::read(path)?;
                if bytes.contains(&0) || String::from_utf8(bytes).is_err() {
                    return Err(OpenDocumentError::NotTextFile);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }

        Document::open(
            &helix_stdx::path::canonicalize(path),
            None,
            true,
            self.config.clone(),
            self.syntax_loader.clone(),
        )
        .map_err(Into::into)
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
