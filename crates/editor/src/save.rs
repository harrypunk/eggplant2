use std::{future::Future, pin::Pin};

use helix_view::document::{DocumentSavedEvent, DocumentSavedEventResult};
use thiserror::Error;

use crate::session::BufferId;

/// Cloneable save error suitable for completion messages.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct SaveError {
    message: String,
}

impl From<anyhow::Error> for SaveError {
    fn from(error: anyhow::Error) -> Self {
        Self {
            message: format!("{error:#}"),
        }
    }
}

/// An owned save operation that can run without borrowing the editor session.
pub struct SaveRequest {
    pub(crate) buffer_id: BufferId,
    pub(crate) future: Pin<Box<dyn Future<Output = DocumentSavedEventResult> + Send + 'static>>,
}

impl SaveRequest {
    /// Execute the file write and return its completion for the update loop.
    pub async fn perform(self) -> SaveCompletion {
        SaveCompletion {
            buffer_id: self.buffer_id,
            result: self.future.await.map_err(SaveError::from),
        }
    }
}

/// The result of one completed save snapshot.
#[derive(Debug, Clone)]
pub struct SaveCompletion {
    pub(crate) buffer_id: BufferId,
    pub(crate) result: Result<DocumentSavedEvent, SaveError>,
}

impl SaveCompletion {
    pub fn buffer_id(&self) -> BufferId {
        self.buffer_id
    }
}
