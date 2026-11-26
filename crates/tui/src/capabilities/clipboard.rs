use std::fmt;

use helix_view::clipboard::{ClipboardProvider, ClipboardType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardError {
    Unavailable,
    Backend(String),
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("no system clipboard provider is available"),
            Self::Backend(error) => formatter.write_str(error),
        }
    }
}

impl std::error::Error for ClipboardError {}

/// Minimal text-input port used by paste-oriented capabilities.
pub trait ClipboardReader {
    fn read_text(&self) -> Result<String, ClipboardError>;
}

impl<C: ClipboardReader + ?Sized> ClipboardReader for &C {
    fn read_text(&self) -> Result<String, ClipboardError> {
        (*self).read_text()
    }
}

impl ClipboardReader for ClipboardProvider {
    fn read_text(&self) -> Result<String, ClipboardError> {
        if let ClipboardProvider::None = self {
            return Err(ClipboardError::Unavailable);
        }
        self.get_contents(&ClipboardType::Clipboard)
            .map_err(|error| ClipboardError::Backend(error.to_string()))
    }
}

/// Minimal text-output port used by copy-oriented capabilities.
pub trait ClipboardWriter {
    fn write_text(&self, text: &str) -> Result<(), String>;
}

impl<C: ClipboardWriter + ?Sized> ClipboardWriter for &C {
    fn write_text(&self, text: &str) -> Result<(), String> {
        (*self).write_text(text)
    }
}

impl ClipboardWriter for ClipboardProvider {
    fn write_text(&self, text: &str) -> Result<(), String> {
        if let ClipboardProvider::None = self {
            return Err("no system clipboard provider is available".to_string());
        }
        self.set_contents(text, ClipboardType::Clipboard)
            .map_err(|error| error.to_string())
    }
}
