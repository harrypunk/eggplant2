use std::{future::Future, pin::Pin};

use eggplant_agent::{CancellationToken, SessionId};

use crate::{
    FinalizedRun, RecentPage, RecentQuery, SearchPage, SearchQuery, TranscriptPage, TranscriptQuery,
};

pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, StorageError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    Cancelled,
    InvalidIdentity,
    SessionNotFound(SessionId),
    Backend(String),
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("session storage was cancelled"),
            Self::InvalidIdentity => formatter.write_str("session storage identity mismatch"),
            Self::SessionNotFound(_) => formatter.write_str("agent session was not found"),
            Self::Backend(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {}

/// Consumer-oriented persistence port. All values are owned and backend-neutral.
pub trait SessionStore: std::fmt::Debug + Send + Sync {
    /// Atomically upsert one finalized session/run and its ordered records.
    fn finalize_run<'a>(
        &'a self,
        run: FinalizedRun,
        cancellation: CancellationToken,
    ) -> StoreFuture<'a, ()>;

    fn recent_sessions<'a>(&'a self, query: RecentQuery) -> StoreFuture<'a, RecentPage>;

    fn transcript<'a>(
        &'a self,
        session_id: SessionId,
        query: TranscriptQuery,
    ) -> StoreFuture<'a, TranscriptPage>;

    fn search<'a>(&'a self, query: SearchQuery) -> StoreFuture<'a, SearchPage>;
}
