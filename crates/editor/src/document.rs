use std::{fmt, path::Path, sync::Arc};

use arc_swap::ArcSwap;
use helix_core::{Rope, syntax};
use helix_view::{Document, document::DocumentOpenError, editor::Config};

type SharedConfig = Arc<ArcSwap<Config>>;
type SharedSyntaxLoader = Arc<ArcSwap<syntax::Loader>>;

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
            // Basic editing does not load grammars; this default loader only
            // satisfies Document's constructor and is shared by new documents.
            syntax_loader: Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
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

    pub fn open(&self, path: impl AsRef<Path>) -> Result<Document, DocumentOpenError> {
        Document::open(
            &helix_stdx::path::canonicalize(path),
            None,
            false,
            self.config.clone(),
            self.syntax_loader.clone(),
        )
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
