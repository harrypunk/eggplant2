use std::{fmt, path::PathBuf};

use editor::{EditorChange, ExactTextReplacement};

use crate::filesystem::CreateTarget;

#[derive(Debug, Clone)]
pub struct CreateTextFile {
    root: PathBuf,
    target: CreateTarget,
    content: String,
}

impl CreateTextFile {
    pub(crate) fn new(root: PathBuf, target: CreateTarget, content: String) -> Self {
        Self {
            root,
            target,
            content,
        }
    }

    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub(crate) fn target(&self) -> &CreateTarget {
        &self.target
    }

    pub fn content(&self) -> &str {
        &self.content
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEditorError(String);

impl AgentEditorError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl fmt::Display for AgentEditorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AgentEditorError {}

/// High-level editor capabilities exposed to prepared agent tools.
///
/// Helix documents, views, history, and document construction remain adapter details.
pub trait AgentHost {
    fn replace_exact_text(
        &mut self,
        request: ExactTextReplacement<'_>,
    ) -> Result<EditorChange, AgentEditorError>;

    fn create_text_file(
        &mut self,
        request: CreateTextFile,
    ) -> Result<EditorChange, AgentEditorError>;
}
