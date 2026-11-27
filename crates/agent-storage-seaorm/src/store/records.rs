use eggplant_agent::{MessageRole, RunId, RunStatus, SessionId, ToolResult, ToolResultStatus};
use eggplant_agent_storage::{
    MAX_PAGE_LIMIT, MessageRecord, RecentPage, RunRecord, SearchCursor, SearchHit, SearchPage,
    SessionCursor, SessionRecord, StorageError, ToolCallRecord,
};
use sea_orm::ActiveValue::Set;
use serde::{Deserialize, Serialize};

use crate::entity::{agent_messages, agent_runs, agent_sessions, agent_tool_calls};

pub(super) fn session_active(session: SessionRecord) -> agent_sessions::ActiveModel {
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

pub(super) fn run_active(run: RunRecord, run_number: i64) -> agent_runs::ActiveModel {
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

pub(super) fn message_active(
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

pub(super) fn tool_active(
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

pub(super) fn recent_page(models: Vec<agent_sessions::Model>, limit: usize) -> RecentPage {
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

pub(super) fn search_page(hits: Vec<SearchHit>, limit: usize) -> SearchPage {
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

pub(super) fn session_record(model: agent_sessions::Model) -> SessionRecord {
    SessionRecord {
        id: SessionId::from(model.id),
        created_at: model.created_at,
        updated_at: model.updated_at,
        workspace: model.workspace,
        title: model.title,
    }
}

pub(super) fn run_record(model: agent_runs::Model) -> Result<RunRecord, StorageError> {
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

pub(super) fn message_record(
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

pub(super) fn tool_record(
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

pub(super) fn message_hit(
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

pub(super) fn tool_hit(
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

pub(super) fn role_name(role: MessageRole) -> &'static str {
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

pub(super) fn page_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_PAGE_LIMIT)
}

pub(super) fn fetch_limit(limit: usize) -> Result<u64, StorageError> {
    u64::try_from(limit + 1).map_err(|_| StorageError::Backend("page limit overflow".into()))
}

pub(super) fn sqlite_i64(value: u64, field: &str) -> Result<i64, StorageError> {
    i64::try_from(value).map_err(|_| {
        StorageError::Backend(format!("{field} does not fit in a SQLite signed integer"))
    })
}

fn from_sqlite_u64(value: i64, field: &str) -> Result<u64, StorageError> {
    u64::try_from(value).map_err(|_| {
        StorageError::Backend(format!("persisted {field} is not a nonnegative integer"))
    })
}

pub(super) fn backend(error: impl std::fmt::Display) -> StorageError {
    StorageError::Backend(error.to_string())
}
