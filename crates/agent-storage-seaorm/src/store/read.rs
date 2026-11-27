use std::collections::BTreeSet;

use eggplant_agent::SessionId;
use eggplant_agent_storage::{
    RecentPage, RecentQuery, RunRecord, StorageError, ToolCallRecord, TranscriptCursor,
    TranscriptPage, TranscriptQuery,
};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::ExprTrait,
};

use super::records::{
    backend, fetch_limit, message_record, page_limit, recent_page, run_record, session_record,
    sqlite_i64, tool_record,
};
use crate::entity::{agent_messages, agent_runs, agent_sessions, agent_tool_calls};

pub(super) async fn recent_sessions(
    db: &DatabaseConnection,
    query: RecentQuery,
) -> Result<RecentPage, StorageError> {
    let RecentQuery {
        workspace,
        before,
        limit,
    } = query;
    let before = before.map(|cursor| {
        let updated_at = cursor.updated_at;
        let id = cursor.id.as_str().to_string();
        agent_sessions::Column::UpdatedAt
            .lt(updated_at)
            .or(agent_sessions::Column::UpdatedAt
                .eq(updated_at)
                .and(agent_sessions::Column::Id.lt(id)))
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
        .all(db)
        .await
        .map_err(backend)?;
    Ok(recent_page(models, limit))
}

pub(super) async fn transcript(
    db: &DatabaseConnection,
    session_id: SessionId,
    query: TranscriptQuery,
) -> Result<TranscriptPage, StorageError> {
    let session = agent_sessions::Entity::find_by_id(session_id.as_str())
        .one(db)
        .await
        .map_err(backend)?
        .map(session_record)
        .ok_or(StorageError::SessionNotFound(session_id.clone()))?;
    let after = query
        .after
        .map(|cursor| {
            sqlite_i64(cursor.run_id.0, "transcript cursor run number").and_then(|run_number| {
                sqlite_i64(cursor.sequence, "transcript cursor sequence")
                    .map(|sequence| (run_number, sequence))
            })
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
        .all(db)
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
    let runs = load_runs(db, &run_ids).await?;
    let tool_calls = load_tools(db, session_id.as_str(), &run_ids).await?;
    Ok(TranscriptPage {
        session,
        runs,
        messages: messages.into_iter().take(limit).collect(),
        tool_calls,
        next,
    })
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
