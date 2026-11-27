use eggplant_agent_storage::{SearchCursor, SearchKind, SearchPage, SearchQuery, StorageError};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect,
    sea_query::{Expr, ExprTrait, Func, LikeExpr},
};

use super::records::{
    backend, fetch_limit, message_hit, page_limit, role_name, search_page, sqlite_i64, tool_hit,
};
use crate::entity::{agent_messages, agent_runs, agent_tool_calls};

pub(super) async fn search(
    db: &DatabaseConnection,
    query: SearchQuery,
) -> Result<SearchPage, StorageError> {
    match query.kind {
        SearchKind::Message => search_messages(db, query).await,
        SearchKind::Tool => search_tools(db, query).await,
    }
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
