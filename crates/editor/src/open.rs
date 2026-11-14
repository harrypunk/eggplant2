use std::{
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::{
    DocumentFactory,
    document::PreparedDocument,
    session::{BufferId, EditorSession},
};

/// Owned prepared data, transported through a completion message.
#[derive(Clone)]
pub struct OpenedDocument {
    documents: DocumentFactory,
    prepared: Arc<Mutex<PreparedDocument>>,
}

impl fmt::Debug for OpenedDocument {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OpenedDocument")
    }
}

#[derive(Debug)]
pub struct DocumentOpenRequest {
    documents: DocumentFactory,
    path: PathBuf,
}

impl DocumentOpenRequest {
    pub fn new(documents: DocumentFactory, path: &Path) -> Self {
        Self {
            documents,
            path: path.to_path_buf(),
        }
    }

    pub fn perform(self) -> DocumentOpenCompletion {
        let result = self
            .documents
            .prepare_open(&self.path)
            .map(|prepared| OpenedDocument {
                documents: self.documents.clone(),
                prepared: Arc::new(Mutex::new(prepared)),
            })
            .map_err(|error| error.to_string());
        DocumentOpenCompletion {
            path: self.path,
            result,
        }
    }
}

/// Completion containing no public Helix document/view types.
#[derive(Debug, Clone)]
pub struct DocumentOpenCompletion {
    path: PathBuf,
    result: Result<OpenedDocument, String>,
}

impl DocumentOpenCompletion {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn into_result(self) -> Result<OpenedDocument, String> {
        self.result
    }
}

impl OpenedDocument {
    pub(crate) fn into_prepared(self) -> Result<PreparedDocument, String> {
        Arc::try_unwrap(self.prepared)
            .map_err(|_| "opened document is still shared".to_string())?
            .into_inner()
            .map_err(|_| "opened document lock is poisoned".to_string())
    }
}

impl EditorSession {
    pub fn insert_opened_document(&mut self, opened: OpenedDocument) -> Result<BufferId, String> {
        let documents = opened.documents.clone();
        let prepared = opened.into_prepared()?;
        Ok(self.insert_document(documents.build_document(prepared)))
    }
}
