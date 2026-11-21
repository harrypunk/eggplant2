use eggplant_agent::{MessageRole, RunId, RunStatus, SessionId, ToolResult, ToolResultStatus};

pub const DEFAULT_PAGE_LIMIT: usize = 50;
pub const MAX_PAGE_LIMIT: usize = 100;
const TRUNCATION_MARKER: &str = "\n[truncated]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRecord {
    pub id: SessionId,
    pub created_at: i64,
    pub updated_at: i64,
    pub workspace: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunRecord {
    pub id: RunId,
    pub session_id: SessionId,
    pub status: RunStatus,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    pub provider: String,
    pub model: Option<String>,
    pub editor_context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRecord {
    pub session_id: SessionId,
    pub run_id: RunId,
    pub sequence: u64,
    pub role: MessageRole,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCallRecord {
    pub session_id: SessionId,
    pub run_id: RunId,
    pub sequence: u64,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub result: Option<ToolResult>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalizedRun {
    pub session: SessionRecord,
    pub run: RunRecord,
    pub messages: Vec<MessageRecord>,
    pub tool_calls: Vec<ToolCallRecord>,
}

impl FinalizedRun {
    /// Apply the same persisted-output bounds for every storage backend.
    pub fn bounded(self) -> Self {
        let run = RunRecord {
            status: bounded_status(self.run.status),
            provider: bound_text(self.run.provider, 256),
            model: self.run.model.map(|value| bound_text(value, 256)),
            editor_context: self
                .run
                .editor_context
                .map(|value| bound_text(value, 4 * 1024)),
            ..self.run
        };
        Self {
            session: SessionRecord {
                workspace: self
                    .session
                    .workspace
                    .map(|value| bound_text(value, 4 * 1024)),
                title: self.session.title.map(|value| bound_text(value, 512)),
                ..self.session
            },
            run,
            messages: self
                .messages
                .into_iter()
                .map(|message| MessageRecord {
                    text: bound_text(message.text, 64 * 1024),
                    ..message
                })
                .collect(),
            tool_calls: self
                .tool_calls
                .into_iter()
                .map(|call| ToolCallRecord {
                    call_id: bound_text(call.call_id, 256),
                    name: bound_text(call.name, 256),
                    arguments: bound_text(call.arguments, 16 * 1024),
                    result: call.result.map(|result| ToolResult {
                        call_id: bound_text(result.call_id, 256),
                        content: bound_text(result.content, 50 * 1024),
                        ..result
                    }),
                    ..call
                })
                .collect(),
        }
    }

    pub fn has_consistent_identity(&self) -> bool {
        self.session.id == self.run.session_id
            && self.messages.iter().all(|message| {
                message.session_id == self.session.id && message.run_id == self.run.id
            })
            && self
                .tool_calls
                .iter()
                .all(|call| call.session_id == self.session.id && call.run_id == self.run.id)
    }
}

fn bounded_status(status: RunStatus) -> RunStatus {
    match status {
        RunStatus::Failed(error) => RunStatus::Failed(bound_text(error, 4 * 1024)),
        other => other,
    }
}

pub fn bound_text(text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }
    let content_limit = max_bytes.saturating_sub(TRUNCATION_MARKER.len());
    let boundary = (0..=content_limit.min(text.len()))
        .rev()
        .find(|boundary| text.is_char_boundary(*boundary))
        .unwrap_or(0);
    text[..boundary].to_string() + TRUNCATION_MARKER
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCursor {
    pub updated_at: i64,
    pub id: SessionId,
}

#[derive(Debug, Clone)]
pub struct RecentQuery {
    pub workspace: Option<String>,
    pub before: Option<SessionCursor>,
    pub limit: usize,
}

impl Default for RecentQuery {
    fn default() -> Self {
        Self {
            workspace: None,
            before: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentPage {
    pub sessions: Vec<SessionRecord>,
    pub next: Option<SessionCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TranscriptCursor {
    pub run_id: RunId,
    pub sequence: u64,
}

#[derive(Debug, Clone)]
pub struct TranscriptQuery {
    pub after: Option<TranscriptCursor>,
    pub limit: usize,
}

impl Default for TranscriptQuery {
    fn default() -> Self {
        Self {
            after: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptPage {
    pub session: SessionRecord,
    pub runs: Vec<RunRecord>,
    pub messages: Vec<MessageRecord>,
    pub tool_calls: Vec<ToolCallRecord>,
    pub next: Option<TranscriptCursor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Message,
    Tool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchCursor {
    pub run_id: RunId,
    pub sequence: u64,
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub session_id: SessionId,
    pub kind: SearchKind,
    /// Case-insensitive literal substring; wildcard characters are escaped by backends.
    pub text: Option<String>,
    pub role: Option<MessageRole>,
    pub tool_name: Option<String>,
    pub call_id: Option<String>,
    pub after: Option<SearchCursor>,
    pub limit: usize,
}

impl SearchQuery {
    pub fn messages(session_id: SessionId) -> Self {
        Self {
            session_id,
            kind: SearchKind::Message,
            text: None,
            role: None,
            tool_name: None,
            call_id: None,
            after: None,
            limit: DEFAULT_PAGE_LIMIT,
        }
    }

    pub fn tools(session_id: SessionId) -> Self {
        Self {
            kind: SearchKind::Tool,
            ..Self::messages(session_id)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub session_id: SessionId,
    pub run_id: RunId,
    pub sequence: u64,
    pub role: Option<MessageRole>,
    pub tool_name: Option<String>,
    pub call_id: Option<String>,
    pub status: Option<ToolResultStatus>,
    pub text: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub next: Option<SearchCursor>,
}

pub(crate) fn page_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_PAGE_LIMIT)
}
