use std::path::Path;

use eggplant_agent::{CancellationToken, SessionId};
use eggplant_agent_storage::{
    FinalizedRun, RecentPage, RecentQuery, SearchPage, SearchQuery, SessionStore, StorageError,
    StoreFuture, TranscriptPage, TranscriptQuery,
};
use sea_orm::DatabaseConnection;

use crate::connection;

mod read;
mod records;
mod search;
mod write;

/// SQLite-backed session storage. Construction opens the database and applies migrations.
#[derive(Debug)]
pub struct SeaOrmSessionStore {
    db: DatabaseConnection,
}

impl SeaOrmSessionStore {
    /// Open an absolute SQLite file path, creating parent directories and applying migrations.
    pub async fn connect(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Ok(Self {
            db: connection::connect(path).await?,
        })
    }
}

impl SessionStore for SeaOrmSessionStore {
    fn finalize_run<'a>(
        &'a self,
        run: FinalizedRun,
        cancellation: CancellationToken,
    ) -> StoreFuture<'a, ()> {
        Box::pin(write::finalize_run(&self.db, run, cancellation))
    }

    fn recent_sessions<'a>(&'a self, query: RecentQuery) -> StoreFuture<'a, RecentPage> {
        Box::pin(read::recent_sessions(&self.db, query))
    }

    fn transcript<'a>(
        &'a self,
        session_id: SessionId,
        query: TranscriptQuery,
    ) -> StoreFuture<'a, TranscriptPage> {
        Box::pin(read::transcript(&self.db, session_id, query))
    }

    fn search<'a>(&'a self, query: SearchQuery) -> StoreFuture<'a, SearchPage> {
        Box::pin(search::search(&self.db, query))
    }
}

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod tests;
