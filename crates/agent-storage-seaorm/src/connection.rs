use std::{path::Path, time::Duration};

use eggplant_agent_storage::StorageError;
use sea_orm::{
    DatabaseConnection,
    sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use crate::migration;

const BUSY_TIMEOUT: Duration = Duration::from_secs(2);
const POOL_SIZE: u32 = 5;

/// Open an absolute SQLite path, create parents, and apply all agent migrations.
pub async fn connect(path: impl AsRef<Path>) -> Result<DatabaseConnection, StorageError> {
    let path = path.as_ref();
    if !path.is_absolute() {
        return Err(StorageError::Backend(
            "agent storage requires an absolute SQLite path".to_string(),
        ));
    }
    path.parent()
        .map(std::fs::create_dir_all)
        .transpose()
        .map_err(backend)?;
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .busy_timeout(BUSY_TIMEOUT);
    let pool = SqlitePoolOptions::new()
        .max_connections(POOL_SIZE)
        .acquire_timeout(BUSY_TIMEOUT)
        .connect_with(options)
        .await
        .map_err(backend)?;
    let db = DatabaseConnection::from(pool);
    migration::migrate(&db).await.map_err(backend)?;
    Ok(db)
}

fn backend(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend(error.to_string())
}
