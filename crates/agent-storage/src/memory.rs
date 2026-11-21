use std::collections::{BTreeMap, BTreeSet};

use eggplant_agent::{CancellationToken, RunId, SessionId};

use crate::{
    FinalizedRun, MessageRecord, RecentPage, RecentQuery, RunRecord, SearchCursor, SearchHit,
    SearchKind, SearchPage, SearchQuery, SessionCursor, SessionRecord, SessionStore, StorageError,
    StoreFuture, ToolCallRecord, TranscriptCursor, TranscriptPage, TranscriptQuery, page_limit,
};

#[derive(Debug, Default)]
struct MemoryState {
    sessions: BTreeMap<SessionId, SessionRecord>,
    runs: BTreeMap<(SessionId, RunId), RunRecord>,
    messages: BTreeMap<(SessionId, RunId, u64), MessageRecord>,
    tool_calls: BTreeMap<(SessionId, RunId, u64), ToolCallRecord>,
}

#[derive(Debug, Default)]
pub struct InMemoryStore {
    state: std::sync::RwLock<MemoryState>,
}

impl InMemoryStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionStore for InMemoryStore {
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
            let mut state = self
                .state
                .write()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            state.sessions.insert(run.session.id.clone(), run.session);
            state
                .runs
                .insert((run.run.session_id.clone(), run.run.id), run.run);
            run.messages.into_iter().for_each(|message| {
                state.messages.insert(
                    (message.session_id.clone(), message.run_id, message.sequence),
                    message,
                );
            });
            run.tool_calls.into_iter().for_each(|call| {
                state
                    .tool_calls
                    .insert((call.session_id.clone(), call.run_id, call.sequence), call);
            });
            Ok(())
        })
    }

    fn recent_sessions<'a>(&'a self, query: RecentQuery) -> StoreFuture<'a, RecentPage> {
        Box::pin(async move {
            let RecentQuery {
                workspace,
                before,
                limit,
            } = query;
            let state = self
                .state
                .read()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            let sessions = state
                .sessions
                .values()
                .filter(|session| matches_workspace(session, workspace.as_deref()))
                .filter(|session| is_before_cursor(session, before.as_ref()))
                .cloned()
                .collect();
            Ok(recent_page(sessions, limit))
        })
    }

    fn transcript<'a>(
        &'a self,
        session_id: SessionId,
        query: TranscriptQuery,
    ) -> StoreFuture<'a, TranscriptPage> {
        Box::pin(async move {
            let state = self
                .state
                .read()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            let session = state
                .sessions
                .get(&session_id)
                .cloned()
                .ok_or(StorageError::SessionNotFound(session_id.clone()))?;
            let limit = page_limit(query.limit);
            let messages = state
                .messages
                .values()
                .filter(|message| {
                    message.session_id == session_id
                        && query.after.is_none_or(|after| {
                            (message.run_id, message.sequence) > (after.run_id, after.sequence)
                        })
                })
                .take(limit + 1)
                .cloned()
                .collect::<Vec<_>>();
            let next = (messages.len() > limit)
                .then(|| messages.get(limit - 1))
                .flatten()
                .map(|message| TranscriptCursor {
                    run_id: message.run_id,
                    sequence: message.sequence,
                });
            let messages = messages.into_iter().take(limit).collect::<Vec<_>>();
            let run_ids = messages
                .iter()
                .map(|message| message.run_id)
                .collect::<BTreeSet<_>>();
            let runs = state
                .runs
                .values()
                .filter(|run| run.session_id == session_id && run_ids.contains(&run.id))
                .cloned()
                .collect();
            let tool_calls = state
                .tool_calls
                .values()
                .filter(|call| call.session_id == session_id && run_ids.contains(&call.run_id))
                .cloned()
                .collect();
            Ok(TranscriptPage {
                session,
                runs,
                messages,
                tool_calls,
                next,
            })
        })
    }

    fn search<'a>(&'a self, query: SearchQuery) -> StoreFuture<'a, SearchPage> {
        Box::pin(async move {
            let state = self
                .state
                .read()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            let mut hits = match query.kind {
                SearchKind::Message => state
                    .messages
                    .values()
                    .filter(|message| {
                        message.session_id == query.session_id
                            && query.role.is_none_or(|role| message.role == role)
                            && query
                                .text
                                .as_ref()
                                .is_none_or(|text| contains_text(&message.text, text))
                    })
                    .map(message_hit)
                    .collect::<Vec<_>>(),
                SearchKind::Tool => state
                    .tool_calls
                    .values()
                    .filter(|call| {
                        call.session_id == query.session_id
                            && query
                                .tool_name
                                .as_ref()
                                .is_none_or(|name| call.name == *name)
                            && query
                                .call_id
                                .as_ref()
                                .is_none_or(|call_id| call.call_id == *call_id)
                            && query.text.as_ref().is_none_or(|text| {
                                contains_text(&call.name, text)
                                    || contains_text(&call.arguments, text)
                                    || call
                                        .result
                                        .as_ref()
                                        .is_some_and(|result| contains_text(&result.content, text))
                            })
                    })
                    .map(tool_hit)
                    .collect::<Vec<_>>(),
            };
            hits.sort_by_key(|hit| (hit.run_id, hit.sequence));
            hits.retain(|hit| {
                query
                    .after
                    .is_none_or(|after| (hit.run_id, hit.sequence) > (after.run_id, after.sequence))
            });
            let limit = page_limit(query.limit);
            let next = (hits.len() > limit)
                .then(|| hits.get(limit - 1))
                .flatten()
                .map(|hit| SearchCursor {
                    run_id: hit.run_id,
                    sequence: hit.sequence,
                });
            Ok(SearchPage {
                hits: hits.into_iter().take(limit).collect(),
                next,
            })
        })
    }
}

fn matches_workspace(session: &SessionRecord, workspace: Option<&str>) -> bool {
    workspace.is_none_or(|expected| session.workspace.as_deref() == Some(expected))
}

fn is_before_cursor(session: &SessionRecord, before: Option<&SessionCursor>) -> bool {
    before.is_none_or(|cursor| (session.updated_at, &session.id) < (cursor.updated_at, &cursor.id))
}

fn by_recency(left: &SessionRecord, right: &SessionRecord) -> std::cmp::Ordering {
    right
        .updated_at
        .cmp(&left.updated_at)
        .then_with(|| right.id.cmp(&left.id))
}

fn recent_page(mut sessions: Vec<SessionRecord>, limit: usize) -> RecentPage {
    sessions.sort_by(by_recency);
    let limit = page_limit(limit);
    let next = sessions
        .get(limit)
        .and_then(|_| sessions.get(limit - 1))
        .map(|session| SessionCursor {
            updated_at: session.updated_at,
            id: session.id.clone(),
        });
    RecentPage {
        sessions: sessions.into_iter().take(limit).collect(),
        next,
    }
}

fn contains_text(value: &str, query: &str) -> bool {
    value.to_lowercase().contains(&query.to_lowercase())
}

fn message_hit(message: &MessageRecord) -> SearchHit {
    SearchHit {
        session_id: message.session_id.clone(),
        run_id: message.run_id,
        sequence: message.sequence,
        role: Some(message.role),
        tool_name: None,
        call_id: None,
        status: None,
        text: message.text.clone(),
        created_at: message.created_at,
    }
}

fn tool_hit(call: &ToolCallRecord) -> SearchHit {
    SearchHit {
        session_id: call.session_id.clone(),
        run_id: call.run_id,
        sequence: call.sequence,
        role: None,
        tool_name: Some(call.name.clone()),
        call_id: Some(call.call_id.clone()),
        status: call.result.as_ref().map(|result| result.status),
        text: call
            .result
            .as_ref()
            .map(|result| result.content.clone())
            .unwrap_or_else(|| call.arguments.clone()),
        created_at: call.created_at,
    }
}
