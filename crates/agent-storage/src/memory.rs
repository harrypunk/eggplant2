use std::collections::{BTreeMap, BTreeSet};

use eggplant_agent::{CancellationToken, RunId, SessionId};

use crate::{
    FinalizedRun, MessageRecord, ProviderCredential, ProviderCredentialInfo,
    ProviderCredentialStore, RecentPage, RecentQuery, RunRecord, SearchCursor, SearchHit,
    SearchKind, SearchPage, SearchQuery, SessionCursor, SessionRecord, SessionStore, StorageError,
    StoreFuture, ToolCallRecord, TranscriptCursor, TranscriptPage, TranscriptQuery, page_limit,
};

#[derive(Debug, Default)]
struct MemoryState {
    sessions: BTreeMap<SessionId, SessionRecord>,
    runs: BTreeMap<(SessionId, RunId), RunRecord>,
    messages: BTreeMap<(SessionId, RunId, u64), MessageRecord>,
    tool_calls: BTreeMap<(SessionId, RunId, u64), ToolCallRecord>,
    provider_credentials: BTreeMap<String, ProviderCredential>,
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
            let hits = matching_hits(&state, &query);
            Ok(search_page(hits, query.after, query.limit))
        })
    }
}

impl ProviderCredentialStore for InMemoryStore {
    fn get<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, Option<ProviderCredential>> {
        Box::pin(async move {
            let state = self
                .state
                .read()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            Ok(state.provider_credentials.get(provider_id).cloned())
        })
    }

    fn list<'a>(&'a self) -> StoreFuture<'a, Vec<ProviderCredentialInfo>> {
        Box::pin(async move {
            let state = self
                .state
                .read()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            Ok(state
                .provider_credentials
                .values()
                .map(|credential| ProviderCredentialInfo {
                    provider_id: credential.provider_id.clone(),
                    auth_type: credential.auth_type,
                })
                .collect())
        })
    }

    fn upsert<'a>(&'a self, credential: ProviderCredential) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            credential.validate()?;
            let mut state = self
                .state
                .write()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            state
                .provider_credentials
                .insert(credential.provider_id.clone(), credential);
            Ok(())
        })
    }

    fn delete<'a>(&'a self, provider_id: &'a str) -> StoreFuture<'a, bool> {
        Box::pin(async move {
            let mut state = self
                .state
                .write()
                .map_err(|_| StorageError::Backend("in-memory store lock failed".to_string()))?;
            Ok(state.provider_credentials.remove(provider_id).is_some())
        })
    }
}

fn matching_hits(state: &MemoryState, query: &SearchQuery) -> Vec<SearchHit> {
    match query.kind {
        SearchKind::Message => state
            .messages
            .values()
            .filter(|message| message_matches(message, query))
            .map(message_hit)
            .collect(),
        SearchKind::Tool => state
            .tool_calls
            .values()
            .filter(|call| tool_matches(call, query))
            .map(tool_hit)
            .collect(),
    }
}

fn message_matches(message: &MessageRecord, query: &SearchQuery) -> bool {
    message.session_id == query.session_id
        && query.role.is_none_or(|role| message.role == role)
        && text_matches(&message.text, query.text.as_deref())
}

fn tool_matches(call: &ToolCallRecord, query: &SearchQuery) -> bool {
    call.session_id == query.session_id
        && exact_matches(&call.name, query.tool_name.as_deref())
        && exact_matches(&call.call_id, query.call_id.as_deref())
        && tool_text_matches(call, query.text.as_deref())
}

fn exact_matches(value: &str, expected: Option<&str>) -> bool {
    expected.is_none_or(|expected| value == expected)
}

fn text_matches(value: &str, query: Option<&str>) -> bool {
    query.is_none_or(|query| contains_text(value, query))
}

fn tool_text_matches(call: &ToolCallRecord, query: Option<&str>) -> bool {
    query.is_none_or(|query| {
        [call.name.as_str(), call.arguments.as_str()]
            .into_iter()
            .chain(call.result.as_ref().map(|result| result.content.as_str()))
            .any(|value| contains_text(value, query))
    })
}

fn is_after_cursor(hit: &SearchHit, after: Option<SearchCursor>) -> bool {
    after.is_none_or(|after| (hit.run_id, hit.sequence) > (after.run_id, after.sequence))
}

fn search_page(mut hits: Vec<SearchHit>, after: Option<SearchCursor>, limit: usize) -> SearchPage {
    hits.sort_by_key(|hit| (hit.run_id, hit.sequence));
    hits.retain(|hit| is_after_cursor(hit, after));
    let limit = page_limit(limit);
    let next = hits
        .get(limit)
        .and_then(|_| hits.get(limit - 1))
        .map(|hit| SearchCursor {
            run_id: hit.run_id,
            sequence: hit.sequence,
        });
    SearchPage {
        hits: hits.into_iter().take(limit).collect(),
        next,
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
