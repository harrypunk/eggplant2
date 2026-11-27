use eggplant_agent::CancellationToken;
use eggplant_agent_storage::{
    FinalizedRun, MessageRecord, RunRecord, SessionRecord, StorageError, ToolCallRecord,
};
use sea_orm::{
    ColumnTrait, DatabaseConnection, DatabaseTransaction, EntityTrait, QueryFilter,
    TransactionError, TransactionTrait, sea_query::OnConflict,
};

use super::records::{
    backend, message_active, run_active, session_active, sqlite_i64, tool_active,
};
use crate::entity::{agent_messages, agent_runs, agent_sessions, agent_tool_calls};

pub(super) async fn finalize_run(
    db: &DatabaseConnection,
    run: FinalizedRun,
    cancellation: CancellationToken,
) -> Result<(), StorageError> {
    let prepared = PreparedRun::new(run, cancellation)?;
    db.transaction(move |transaction| Box::pin(async move { prepared.persist(transaction).await }))
        .await
        .map_err(|error| match error {
            TransactionError::Connection(error) => backend(error),
            TransactionError::Transaction(error) => error,
        })
}

struct PreparedRun {
    run: FinalizedRun,
    run_number: i64,
}

impl PreparedRun {
    fn new(run: FinalizedRun, cancellation: CancellationToken) -> Result<Self, StorageError> {
        if cancellation.is_cancelled() {
            return Err(StorageError::Cancelled);
        }
        if !run.has_consistent_identity() {
            return Err(StorageError::InvalidIdentity);
        }
        let run = run.bounded();
        let run_number = sqlite_i64(run.run.id.0, "run number")?;
        Ok(Self { run, run_number })
    }

    async fn persist(self, transaction: &DatabaseTransaction) -> Result<(), StorageError> {
        let FinalizedRun {
            session,
            run,
            messages,
            tool_calls,
        } = self.run;
        let session_id = session.id.as_str().to_string();
        upsert_session(transaction, session).await?;
        upsert_run(transaction, run, self.run_number).await?;
        let run_id = persisted_run_id(transaction, &session_id, self.run_number).await?;
        upsert_messages(transaction, messages, run_id).await?;
        upsert_tool_calls(transaction, tool_calls, run_id).await
    }
}

async fn upsert_session(
    transaction: &DatabaseTransaction,
    session: SessionRecord,
) -> Result<(), StorageError> {
    agent_sessions::Entity::insert(session_active(session))
        .on_conflict(
            OnConflict::column(agent_sessions::Column::Id)
                .update_columns([
                    agent_sessions::Column::CreatedAt,
                    agent_sessions::Column::UpdatedAt,
                    agent_sessions::Column::Workspace,
                    agent_sessions::Column::Title,
                ])
                .to_owned(),
        )
        .exec(transaction)
        .await
        .map_err(backend)
        .map(|_| ())
}

async fn upsert_run(
    transaction: &DatabaseTransaction,
    run: RunRecord,
    run_number: i64,
) -> Result<(), StorageError> {
    agent_runs::Entity::insert(run_active(run, run_number))
        .on_conflict(
            OnConflict::columns([agent_runs::Column::SessionId, agent_runs::Column::RunNumber])
                .update_columns([
                    agent_runs::Column::Status,
                    agent_runs::Column::Error,
                    agent_runs::Column::CreatedAt,
                    agent_runs::Column::CompletedAt,
                    agent_runs::Column::Provider,
                    agent_runs::Column::Model,
                    agent_runs::Column::EditorContext,
                ])
                .to_owned(),
        )
        .exec(transaction)
        .await
        .map_err(backend)
        .map(|_| ())
}

async fn persisted_run_id(
    transaction: &DatabaseTransaction,
    session_id: &str,
    run_number: i64,
) -> Result<i64, StorageError> {
    agent_runs::Entity::find()
        .filter(agent_runs::Column::SessionId.eq(session_id))
        .filter(agent_runs::Column::RunNumber.eq(run_number))
        .one(transaction)
        .await
        .map_err(backend)?
        .map(|run| run.id)
        .ok_or_else(|| StorageError::Backend("upserted agent run was not found".into()))
}

async fn upsert_messages(
    transaction: &DatabaseTransaction,
    messages: Vec<MessageRecord>,
    run_id: i64,
) -> Result<(), StorageError> {
    let messages = messages
        .into_iter()
        .map(|message| message_active(message, run_id))
        .collect::<Result<Vec<_>, _>>()?;
    if messages.is_empty() {
        return Ok(());
    }
    agent_messages::Entity::insert_many(messages)
        .on_conflict(
            OnConflict::columns([
                agent_messages::Column::SessionId,
                agent_messages::Column::RunId,
                agent_messages::Column::Sequence,
            ])
            .update_columns([
                agent_messages::Column::Role,
                agent_messages::Column::Text,
                agent_messages::Column::CreatedAt,
            ])
            .to_owned(),
        )
        .exec(transaction)
        .await
        .map_err(backend)
        .map(|_| ())
}

async fn upsert_tool_calls(
    transaction: &DatabaseTransaction,
    tool_calls: Vec<ToolCallRecord>,
    run_id: i64,
) -> Result<(), StorageError> {
    let tool_calls = tool_calls
        .into_iter()
        .map(|call| tool_active(call, run_id))
        .collect::<Result<Vec<_>, _>>()?;
    if tool_calls.is_empty() {
        return Ok(());
    }
    agent_tool_calls::Entity::insert_many(tool_calls)
        .on_conflict(
            OnConflict::columns([
                agent_tool_calls::Column::SessionId,
                agent_tool_calls::Column::RunId,
                agent_tool_calls::Column::Sequence,
            ])
            .update_columns([
                agent_tool_calls::Column::CallId,
                agent_tool_calls::Column::Name,
                agent_tool_calls::Column::ArgumentsJson,
                agent_tool_calls::Column::Status,
                agent_tool_calls::Column::ResultJson,
                agent_tool_calls::Column::ResultContent,
                agent_tool_calls::Column::CreatedAt,
                agent_tool_calls::Column::CompletedAt,
            ])
            .to_owned(),
        )
        .exec(transaction)
        .await
        .map_err(backend)
        .map(|_| ())
}
