use std::{collections::BTreeSet, path::Path};

use eggplant_agent::{
    CancellationToken, MessageRole, RunId, RunStatus, SessionId, ToolResult, ToolResultStatus,
};
use eggplant_agent_storage::{
    FinalizedRun, MAX_PAGE_LIMIT, MessageRecord, RecentPage, RecentQuery, RunRecord, SearchCursor,
    SearchHit, SearchKind, SearchPage, SearchQuery, SessionCursor, SessionRecord, SessionStore,
    StorageError, StoreFuture, ToolCallRecord, TranscriptCursor, TranscriptPage, TranscriptQuery,
};
use sea_orm::{
    ActiveValue::Set,
    ColumnTrait, Condition, DatabaseConnection, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionError, TransactionTrait,
    sea_query::{Expr, ExprTrait, Func, LikeExpr, OnConflict},
};
use serde::{Deserialize, Serialize};

use crate::{
    connection,
    entity::{agent_messages, agent_runs, agent_sessions, agent_tool_calls},
};

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
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(StorageError::Cancelled);
            }
            if !run.has_consistent_identity() {
                return Err(StorageError::InvalidIdentity);
            }
            let run = run.bounded();
            let run_number = sqlite_i64(run.run.id.0, "run number")?;
            self.db
                .transaction(move |transaction| {
                    Box::pin(async move { persist_run(transaction, run, run_number).await })
                })
                .await
                .map_err(|error| match error {
                    TransactionError::Connection(error) => backend(error),
                    TransactionError::Transaction(error) => error,
                })
        })
    }

    fn recent_sessions<'a>(&'a self, query: RecentQuery) -> StoreFuture<'a, RecentPage> {
        Box::pin(async move {
            let RecentQuery {
                workspace,
                before,
                limit,
            } = query;
            let before = before.map(|cursor| {
                let updated_at = cursor.updated_at;
                let id = cursor.id.as_str().to_string();
                agent_sessions::Column::UpdatedAt.lt(updated_at).or(
                    agent_sessions::Column::UpdatedAt
                        .eq(updated_at)
                        .and(agent_sessions::Column::Id.lt(id)),
                )
            });
            let condition = [
                workspace.map(|value| agent_sessions::Column::Workspace.eq(value)),
                before,
            ]
            .into_iter()
            .flatten()
            .fold(Condition::all(), |condition, expression| {
                condition.add(expression)
            });
            let limit = page_limit(limit);
            let models = agent_sessions::Entity::find()
                .filter(condition)
                .order_by_desc(agent_sessions::Column::UpdatedAt)
                .order_by_desc(agent_sessions::Column::Id)
                .limit(fetch_limit(limit)?)
                .all(&self.db)
                .await
                .map_err(backend)?;
            Ok(recent_page(models, limit))
        })
    }

    fn transcript<'a>(
        &'a self,
        session_id: SessionId,
        query: TranscriptQuery,
    ) -> StoreFuture<'a, TranscriptPage> {
        Box::pin(async move {
            let session = agent_sessions::Entity::find_by_id(session_id.as_str())
                .one(&self.db)
                .await
                .map_err(backend)?
                .map(session_record)
                .ok_or(StorageError::SessionNotFound(session_id.clone()))?;
            let after = query
                .after
                .map(|cursor| {
                    sqlite_i64(cursor.run_id.0, "transcript cursor run number").and_then(
                        |run_number| {
                            sqlite_i64(cursor.sequence, "transcript cursor sequence")
                                .map(|sequence| (run_number, sequence))
                        },
                    )
                })
                .transpose()?;
            let after = after.map(|(run_number, sequence)| {
                agent_runs::Column::RunNumber
                    .gt(run_number)
                    .or(agent_runs::Column::RunNumber
                        .eq(run_number)
                        .and(agent_messages::Column::Sequence.gt(sequence)))
            });
            let condition = [
                Some(agent_messages::Column::SessionId.eq(session_id.as_str())),
                after,
            ]
            .into_iter()
            .flatten()
            .fold(Condition::all(), |condition, expression| {
                condition.add(expression)
            });
            let limit = page_limit(query.limit);
            let rows = agent_messages::Entity::find()
                .find_also_related(agent_runs::Entity)
                .filter(condition)
                .order_by_asc(agent_runs::Column::RunNumber)
                .order_by_asc(agent_messages::Column::Sequence)
                .limit(fetch_limit(limit)?)
                .all(&self.db)
                .await
                .map_err(backend)?;
            let run_ids = rows
                .iter()
                .take(limit)
                .map(|(_, run)| {
                    run.as_ref()
                        .map(|run| run.id)
                        .ok_or_else(|| StorageError::Backend("message has no persisted run".into()))
                })
                .collect::<Result<BTreeSet<_>, _>>()?;
            let messages = rows
                .into_iter()
                .map(message_record)
                .collect::<Result<Vec<_>, _>>()?;
            let next = (messages.len() > limit)
                .then(|| messages.get(limit - 1))
                .flatten()
                .map(|message| TranscriptCursor {
                    run_id: message.run_id,
                    sequence: message.sequence,
                });
            let runs = load_runs(&self.db, &run_ids).await?;
            let tool_calls = load_tools(&self.db, session_id.as_str(), &run_ids).await?;
            Ok(TranscriptPage {
                session,
                runs,
                messages: messages.into_iter().take(limit).collect(),
                tool_calls,
                next,
            })
        })
    }

    fn search<'a>(&'a self, query: SearchQuery) -> StoreFuture<'a, SearchPage> {
        Box::pin(async move {
            match query.kind {
                SearchKind::Message => search_messages(&self.db, query).await,
                SearchKind::Tool => search_tools(&self.db, query).await,
            }
        })
    }
}

async fn persist_run(
    transaction: &DatabaseTransaction,
    run: FinalizedRun,
    run_number: i64,
) -> Result<(), StorageError> {
    let FinalizedRun {
        session,
        run,
        messages,
        tool_calls,
    } = run;
    let session_id = session.id.as_str().to_string();
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
        .map_err(backend)?;
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
        .map_err(backend)?;
    let run_id = agent_runs::Entity::find()
        .filter(agent_runs::Column::SessionId.eq(session_id))
        .filter(agent_runs::Column::RunNumber.eq(run_number))
        .one(transaction)
        .await
        .map_err(backend)?
        .map(|run| run.id)
        .ok_or_else(|| StorageError::Backend("upserted agent run was not found".into()))?;
    let messages = messages
        .into_iter()
        .map(|message| message_active(message, run_id))
        .collect::<Result<Vec<_>, _>>()?;
    if !messages.is_empty() {
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
            .map_err(backend)?;
    }
    let tool_calls = tool_calls
        .into_iter()
        .map(|call| tool_active(call, run_id))
        .collect::<Result<Vec<_>, _>>()?;
    if !tool_calls.is_empty() {
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
            .map_err(backend)?;
    }
    Ok(())
}

async fn load_runs(
    db: &DatabaseConnection,
    run_ids: &BTreeSet<i64>,
) -> Result<Vec<RunRecord>, StorageError> {
    if run_ids.is_empty() {
        return Ok(Vec::new());
    }
    agent_runs::Entity::find()
        .filter(agent_runs::Column::Id.is_in(run_ids.iter().copied().collect::<Vec<_>>()))
        .order_by_asc(agent_runs::Column::RunNumber)
        .all(db)
        .await
        .map_err(backend)?
        .into_iter()
        .map(run_record)
        .collect()
}

async fn load_tools(
    db: &DatabaseConnection,
    session_id: &str,
    run_ids: &BTreeSet<i64>,
) -> Result<Vec<ToolCallRecord>, StorageError> {
    if run_ids.is_empty() {
        return Ok(Vec::new());
    }
    agent_tool_calls::Entity::find()
        .find_also_related(agent_runs::Entity)
        .filter(agent_tool_calls::Column::SessionId.eq(session_id))
        .filter(agent_tool_calls::Column::RunId.is_in(run_ids.iter().copied().collect::<Vec<_>>()))
        .order_by_asc(agent_runs::Column::RunNumber)
        .order_by_asc(agent_tool_calls::Column::Sequence)
        .all(db)
        .await
        .map_err(backend)?
        .into_iter()
        .map(tool_record)
        .collect()
}

async fn search_messages(
    db: &DatabaseConnection,
    query: SearchQuery,
) -> Result<SearchPage, StorageError> {
    let SearchQuery {
        session_id,
        text,
        role,
        after,
        limit,
        ..
    } = query;
    let text = text.map(|text| {
        contains(
            Expr::col(agent_messages::Column::Text.as_column_ref()),
            text,
        )
    });
    let role = role.map(|role| agent_messages::Column::Role.eq(role_name(role)));
    let after = search_after(
        after,
        Expr::col(agent_messages::Column::Sequence.as_column_ref()),
    )?;
    let condition = [
        Some(agent_messages::Column::SessionId.eq(session_id.as_str())),
        role,
        text,
        after,
    ]
    .into_iter()
    .flatten()
    .fold(Condition::all(), |condition, expression| {
        condition.add(expression)
    });
    let limit = page_limit(limit);
    let hits = agent_messages::Entity::find()
        .find_also_related(agent_runs::Entity)
        .filter(condition)
        .order_by_asc(agent_runs::Column::RunNumber)
        .order_by_asc(agent_messages::Column::Sequence)
        .limit(fetch_limit(limit)?)
        .all(db)
        .await
        .map_err(backend)?
        .into_iter()
        .map(message_hit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(search_page(hits, limit))
}

async fn search_tools(
    db: &DatabaseConnection,
    query: SearchQuery,
) -> Result<SearchPage, StorageError> {
    let SearchQuery {
        session_id,
        text,
        tool_name,
        call_id,
        after,
        limit,
        ..
    } = query;
    let text = text.map(|text| {
        contains(
            Expr::col(agent_tool_calls::Column::Name.as_column_ref()),
            text.clone(),
        )
        .or(contains(
            Expr::col(agent_tool_calls::Column::ArgumentsJson.as_column_ref()),
            text.clone(),
        ))
        .or(contains(
            Expr::col(agent_tool_calls::Column::ResultContent.as_column_ref()),
            text,
        ))
    });
    let tool_name = tool_name.map(|name| agent_tool_calls::Column::Name.eq(name));
    let call_id = call_id.map(|call_id| agent_tool_calls::Column::CallId.eq(call_id));
    let after = search_after(
        after,
        Expr::col(agent_tool_calls::Column::Sequence.as_column_ref()),
    )?;
    let condition = [
        Some(agent_tool_calls::Column::SessionId.eq(session_id.as_str())),
        tool_name,
        call_id,
        text,
        after,
    ]
    .into_iter()
    .flatten()
    .fold(Condition::all(), |condition, expression| {
        condition.add(expression)
    });
    let limit = page_limit(limit);
    let hits = agent_tool_calls::Entity::find()
        .find_also_related(agent_runs::Entity)
        .filter(condition)
        .order_by_asc(agent_runs::Column::RunNumber)
        .order_by_asc(agent_tool_calls::Column::Sequence)
        .limit(fetch_limit(limit)?)
        .all(db)
        .await
        .map_err(backend)?
        .into_iter()
        .map(tool_hit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(search_page(hits, limit))
}

fn contains(expression: Expr, text: String) -> Expr {
    let escaped = text
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
        .to_lowercase();
    Func::lower(expression).like(LikeExpr::new(format!("%{escaped}%")).escape('\\'))
}

fn search_after(
    cursor: Option<SearchCursor>,
    sequence: Expr,
) -> Result<Option<Expr>, StorageError> {
    cursor
        .map(|cursor| {
            sqlite_i64(cursor.run_id.0, "search cursor run number").and_then(|run_number| {
                sqlite_i64(cursor.sequence, "search cursor sequence").map(|sequence_value| {
                    agent_runs::Column::RunNumber
                        .gt(run_number)
                        .or(agent_runs::Column::RunNumber
                            .eq(run_number)
                            .and(sequence.gt(sequence_value)))
                })
            })
        })
        .transpose()
}

fn session_active(session: SessionRecord) -> agent_sessions::ActiveModel {
    let SessionRecord {
        id,
        created_at,
        updated_at,
        workspace,
        title,
    } = session;
    agent_sessions::ActiveModel {
        id: Set(id.as_str().to_string()),
        created_at: Set(created_at),
        updated_at: Set(updated_at),
        workspace: Set(workspace),
        title: Set(title),
    }
}

fn run_active(run: RunRecord, run_number: i64) -> agent_runs::ActiveModel {
    let RunRecord {
        session_id,
        status,
        created_at,
        completed_at,
        provider,
        model,
        editor_context,
        ..
    } = run;
    let (status, error) = run_status_parts(status);
    agent_runs::ActiveModel {
        id: Default::default(),
        session_id: Set(session_id.as_str().to_string()),
        run_number: Set(run_number),
        status: Set(status.to_string()),
        error: Set(error),
        created_at: Set(created_at),
        completed_at: Set(completed_at),
        provider: Set(provider),
        model: Set(model),
        editor_context: Set(editor_context),
    }
}

fn message_active(
    message: MessageRecord,
    run_id: i64,
) -> Result<agent_messages::ActiveModel, StorageError> {
    let sequence = sqlite_i64(message.sequence, "message sequence")?;
    let MessageRecord {
        session_id,
        role,
        text,
        created_at,
        ..
    } = message;
    Ok(agent_messages::ActiveModel {
        id: Default::default(),
        session_id: Set(session_id.as_str().to_string()),
        run_id: Set(run_id),
        sequence: Set(sequence),
        role: Set(role_name(role).to_string()),
        text: Set(text),
        created_at: Set(created_at),
    })
}

fn tool_active(
    call: ToolCallRecord,
    run_id: i64,
) -> Result<agent_tool_calls::ActiveModel, StorageError> {
    let sequence = sqlite_i64(call.sequence, "tool call sequence")?;
    let ToolCallRecord {
        session_id,
        call_id,
        name,
        arguments,
        result,
        created_at,
        completed_at,
        ..
    } = call;
    let result = result.map(StoredToolResult::from);
    let status = result
        .as_ref()
        .map(|result| stored_tool_status_name(result.status))
        .unwrap_or("pending");
    let result_content = result.as_ref().map(|result| result.content.clone());
    let result_json = result
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(backend)?;
    Ok(agent_tool_calls::ActiveModel {
        id: Default::default(),
        session_id: Set(session_id.as_str().to_string()),
        run_id: Set(run_id),
        sequence: Set(sequence),
        call_id: Set(call_id),
        name: Set(name),
        arguments_json: Set(arguments),
        status: Set(status.to_string()),
        result_json: Set(result_json),
        result_content: Set(result_content),
        created_at: Set(created_at),
        completed_at: Set(completed_at),
    })
}

fn recent_page(models: Vec<agent_sessions::Model>, limit: usize) -> RecentPage {
    let next = (models.len() > limit)
        .then(|| models.get(limit - 1))
        .flatten()
        .map(|session| SessionCursor {
            updated_at: session.updated_at,
            id: SessionId::from(session.id.clone()),
        });
    RecentPage {
        sessions: models.into_iter().take(limit).map(session_record).collect(),
        next,
    }
}

fn search_page(hits: Vec<SearchHit>, limit: usize) -> SearchPage {
    let next = (hits.len() > limit)
        .then(|| hits.get(limit - 1))
        .flatten()
        .map(|hit| SearchCursor {
            run_id: hit.run_id,
            sequence: hit.sequence,
        });
    SearchPage {
        hits: hits.into_iter().take(limit).collect(),
        next,
    }
}

fn session_record(model: agent_sessions::Model) -> SessionRecord {
    SessionRecord {
        id: SessionId::from(model.id),
        created_at: model.created_at,
        updated_at: model.updated_at,
        workspace: model.workspace,
        title: model.title,
    }
}

fn run_record(model: agent_runs::Model) -> Result<RunRecord, StorageError> {
    Ok(RunRecord {
        id: RunId(from_sqlite_u64(model.run_number, "run number")?),
        session_id: SessionId::from(model.session_id),
        status: parse_run_status(model.status, model.error)?,
        created_at: model.created_at,
        completed_at: model.completed_at,
        provider: model.provider,
        model: model.model,
        editor_context: model.editor_context,
    })
}

fn message_record(
    (message, run): (agent_messages::Model, Option<agent_runs::Model>),
) -> Result<MessageRecord, StorageError> {
    let run = run.ok_or_else(|| StorageError::Backend("message has no persisted run".into()))?;
    Ok(MessageRecord {
        session_id: SessionId::from(message.session_id),
        run_id: RunId(from_sqlite_u64(run.run_number, "message run number")?),
        sequence: from_sqlite_u64(message.sequence, "message sequence")?,
        role: parse_role(message.role)?,
        text: message.text,
        created_at: message.created_at,
    })
}

fn tool_record(
    (call, run): (agent_tool_calls::Model, Option<agent_runs::Model>),
) -> Result<ToolCallRecord, StorageError> {
    let run = run.ok_or_else(|| StorageError::Backend("tool call has no persisted run".into()))?;
    let result = call
        .result_json
        .as_deref()
        .map(serde_json::from_str::<StoredToolResult>)
        .transpose()
        .map_err(backend)?
        .map(ToolResult::from);
    Ok(ToolCallRecord {
        session_id: SessionId::from(call.session_id),
        run_id: RunId(from_sqlite_u64(run.run_number, "tool call run number")?),
        sequence: from_sqlite_u64(call.sequence, "tool call sequence")?,
        call_id: call.call_id,
        name: call.name,
        arguments: call.arguments_json,
        result,
        created_at: call.created_at,
        completed_at: call.completed_at,
    })
}

fn message_hit(
    row: (agent_messages::Model, Option<agent_runs::Model>),
) -> Result<SearchHit, StorageError> {
    let message = message_record(row)?;
    Ok(SearchHit {
        session_id: message.session_id,
        run_id: message.run_id,
        sequence: message.sequence,
        role: Some(message.role),
        tool_name: None,
        call_id: None,
        status: None,
        text: message.text,
        created_at: message.created_at,
    })
}

fn tool_hit(
    row: (agent_tool_calls::Model, Option<agent_runs::Model>),
) -> Result<SearchHit, StorageError> {
    let call = tool_record(row)?;
    let status = call.result.as_ref().map(|result| result.status);
    let text = call
        .result
        .map(|result| result.content)
        .unwrap_or(call.arguments);
    Ok(SearchHit {
        session_id: call.session_id,
        run_id: call.run_id,
        sequence: call.sequence,
        role: None,
        tool_name: Some(call.name),
        call_id: Some(call.call_id),
        status,
        text,
        created_at: call.created_at,
    })
}

fn run_status_parts(status: RunStatus) -> (&'static str, Option<String>) {
    match status {
        RunStatus::Waiting => ("waiting", None),
        RunStatus::Streaming => ("streaming", None),
        RunStatus::WaitingForApproval => ("waiting_for_approval", None),
        RunStatus::Complete => ("complete", None),
        RunStatus::Failed(error) => ("failed", Some(error)),
        RunStatus::Cancelled => ("cancelled", None),
    }
}

fn parse_run_status(status: String, error: Option<String>) -> Result<RunStatus, StorageError> {
    match status.as_str() {
        "waiting" => Ok(RunStatus::Waiting),
        "streaming" => Ok(RunStatus::Streaming),
        "waiting_for_approval" => Ok(RunStatus::WaitingForApproval),
        "complete" => Ok(RunStatus::Complete),
        "failed" => error
            .map(RunStatus::Failed)
            .ok_or_else(|| StorageError::Backend("failed agent run has no error message".into())),
        "cancelled" => Ok(RunStatus::Cancelled),
        _ => Err(StorageError::Backend(format!(
            "unknown persisted run status: {status}"
        ))),
    }
}

fn role_name(role: MessageRole) -> &'static str {
    match role {
        MessageRole::User => "user",
        MessageRole::Assistant => "assistant",
    }
}

fn parse_role(role: String) -> Result<MessageRole, StorageError> {
    match role.as_str() {
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        _ => Err(StorageError::Backend(format!(
            "unknown persisted message role: {role}"
        ))),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredToolResult {
    call_id: String,
    status: StoredToolResultStatus,
    content: String,
    truncated: bool,
}

impl From<ToolResult> for StoredToolResult {
    fn from(result: ToolResult) -> Self {
        Self {
            call_id: result.call_id,
            status: result.status.into(),
            content: result.content,
            truncated: result.truncated,
        }
    }
}

impl From<StoredToolResult> for ToolResult {
    fn from(result: StoredToolResult) -> Self {
        Self {
            call_id: result.call_id,
            status: result.status.into(),
            content: result.content,
            truncated: result.truncated,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum StoredToolResultStatus {
    Success,
    Error,
    Rejected,
    Cancelled,
}

impl From<ToolResultStatus> for StoredToolResultStatus {
    fn from(status: ToolResultStatus) -> Self {
        match status {
            ToolResultStatus::Success => Self::Success,
            ToolResultStatus::Error => Self::Error,
            ToolResultStatus::Rejected => Self::Rejected,
            ToolResultStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<StoredToolResultStatus> for ToolResultStatus {
    fn from(status: StoredToolResultStatus) -> Self {
        match status {
            StoredToolResultStatus::Success => Self::Success,
            StoredToolResultStatus::Error => Self::Error,
            StoredToolResultStatus::Rejected => Self::Rejected,
            StoredToolResultStatus::Cancelled => Self::Cancelled,
        }
    }
}

fn stored_tool_status_name(status: StoredToolResultStatus) -> &'static str {
    match status {
        StoredToolResultStatus::Success => "success",
        StoredToolResultStatus::Error => "error",
        StoredToolResultStatus::Rejected => "rejected",
        StoredToolResultStatus::Cancelled => "cancelled",
    }
}

fn page_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_PAGE_LIMIT)
}

fn fetch_limit(limit: usize) -> Result<u64, StorageError> {
    u64::try_from(limit + 1).map_err(|_| StorageError::Backend("page limit overflow".into()))
}

fn sqlite_i64(value: u64, field: &str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| {
        StorageError::Backend(format!("{field} does not fit in a SQLite signed integer"))
    })
}

fn from_sqlite_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| {
        StorageError::Backend(format!("persisted {field} is not a nonnegative integer"))
    })
}

fn backend(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend(error.to_string())
}

#[cfg(test)]
mod tests;
