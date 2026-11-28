use std::{collections::BTreeMap, path::Path, sync::Arc};

use eggplant_agent::{
    AgentRun, AgentSession, CancellationToken, ChatMessage, MessageRole, RunId, SessionId,
    SessionSnapshot, ToolEvent,
};
use eggplant_agent_storage::{
    FinalizedRun, MAX_PAGE_LIMIT, MessageRecord, RunRecord, SessionRecord, SessionStore,
    StorageError, ToolCallRecord, TranscriptQuery,
};

use futures_util::{TryStreamExt, stream};

use crate::{
    data_object::{Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

use super::{AgentRuntime, EditorContext};

#[derive(Debug)]
enum Finalization {
    NotFinalized,
    Ready(Box<FinalizedRun>),
    Invalid(FinalizationError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalizationError {
    MissingRunStartedAt(RunId),
    MissingSessionCreatedAt,
}

impl std::fmt::Display for FinalizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingRunStartedAt(run_id) => {
                write!(formatter, "missing start timestamp for run {}", run_id.0)
            }
            Self::MissingSessionCreatedAt => {
                formatter.write_str("missing session creation timestamp")
            }
        }
    }
}

pub fn persist_finalized(model: &mut Model) -> Option<Effect> {
    let store = model.agent.runtime().session_store()?;
    let workspace = model.workspace.as_ref().map(|workspace| workspace.root());
    match finalized_run(
        model.agent.runtime(),
        workspace,
        chrono::Utc::now().timestamp_millis(),
    ) {
        Finalization::NotFinalized => None,
        Finalization::Invalid(error) => Some(crate::notifications::publish_status(
            model,
            StatusMessage::error(format!("Agent session storage failed: {error}")),
            Some("Agent"),
        )),
        Finalization::Ready(run) => {
            let run = (*run).bounded();
            Some(Effect::feature(StorageEffect {
                store,
                session_id: run.session.id.clone(),
                run_id: run.run.id,
                run,
            }))
        }
    }
}

fn finalized_run(runtime: &AgentRuntime, workspace: Option<&Path>, now: i64) -> Finalization {
    let session = runtime.session();
    let Some(run) = session.run() else {
        return Finalization::NotFinalized;
    };
    if run.status.is_running() {
        return Finalization::NotFinalized;
    }
    let run_id = run.id;
    let Some(started) = runtime.run_started_at(run_id) else {
        return Finalization::Invalid(FinalizationError::MissingRunStartedAt(run_id));
    };
    let Some(session_created) = runtime.session_created_at() else {
        return Finalization::Invalid(FinalizationError::MissingSessionCreatedAt);
    };
    let session_id = session.id().clone();
    let title = session
        .messages()
        .iter()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.text.chars().take(160).collect::<String>());
    Finalization::Ready(Box::new(FinalizedRun {
        session: SessionRecord {
            id: session_id.clone(),
            created_at: session_created,
            updated_at: now,
            workspace: workspace.map(|root| root.to_string_lossy().into_owned()),
            title,
        },
        run: RunRecord {
            id: run_id,
            session_id: session_id.clone(),
            status: run.status.clone(),
            created_at: started,
            completed_at: Some(now),
            provider: runtime.provider_name().to_string(),
            model: runtime.model().map(str::to_string),
            editor_context: editor_context_record(runtime.run_context(run_id)),
        },
        messages: message_records(session, &session_id, run_id, started, now),
        tool_calls: tool_call_records(session, &session_id, run_id, started, now),
    }))
}

fn editor_context_record(context: Option<&EditorContext>) -> Option<String> {
    context.map(|context| {
        serde_json::json!({
            "buffer_id": context.buffer.as_raw(),
            "path": context.path,
            "revision": context.revision,
            "line": context.line,
        })
        .to_string()
    })
}

fn message_records(
    session: &AgentSession,
    session_id: &SessionId,
    run_id: RunId,
    started: i64,
    now: i64,
) -> Vec<MessageRecord> {
    session
        .messages()
        .iter()
        .filter(|message| message.run_id == run_id)
        .enumerate()
        .map(|(sequence, message)| MessageRecord {
            session_id: session_id.clone(),
            run_id,
            sequence: sequence as u64,
            role: message.role,
            text: message.text.clone(),
            created_at: match message.role {
                MessageRole::User => started,
                MessageRole::Assistant => now,
            },
        })
        .collect()
}

fn tool_call_records(
    session: &AgentSession,
    session_id: &SessionId,
    run_id: RunId,
    started: i64,
    now: i64,
) -> Vec<ToolCallRecord> {
    session
        .tool_events()
        .iter()
        .filter(|event| event.run_id == run_id)
        .enumerate()
        .map(|(sequence, event)| ToolCallRecord {
            session_id: session_id.clone(),
            run_id,
            sequence: sequence as u64,
            call_id: event.call_id.clone(),
            name: event.name.clone(),
            arguments: event.arguments.clone(),
            result: event.result.clone(),
            created_at: started,
            completed_at: event.result.as_ref().map(|_| now),
        })
        .collect()
}

#[derive(Debug)]
pub struct LoadedSession {
    pub snapshot: SessionSnapshot,
    pub created_at: i64,
}

/// Load all finalized transcript pages through the storage port without UI state.
pub async fn load_session(
    store: &dyn SessionStore,
    session_id: SessionId,
    cancellation: CancellationToken,
) -> Result<LoadedSession, StorageError> {
    load_session_page(store, session_id, cancellation, MAX_PAGE_LIMIT).await
}

async fn load_session_page(
    store: &dyn SessionStore,
    session_id: SessionId,
    cancellation: CancellationToken,
    page_limit: usize,
) -> Result<LoadedSession, StorageError> {
    let page_session_id = session_id.clone();
    let page_cancellation = cancellation.clone();
    let pages = stream::try_unfold((None, false), move |(after, done)| {
        let session_id = page_session_id.clone();
        let cancellation = page_cancellation.clone();
        async move {
            if done {
                return Ok(None);
            }
            if cancellation.is_cancelled() {
                return Err(StorageError::Cancelled);
            }
            let page = store
                .transcript(
                    session_id,
                    TranscriptQuery {
                        after,
                        limit: page_limit,
                    },
                )
                .await?;
            let next = page.next;
            Ok(Some((page, (next, next.is_none()))))
        }
    })
    .try_collect::<Vec<_>>()
    .await?;

    let (stored_id, created_at) = pages
        .last()
        .map(|page| (page.session.id.clone(), page.session.created_at))
        .ok_or(StorageError::SessionNotFound(session_id))?;
    let runs = pages
        .iter()
        .flat_map(|page| &page.runs)
        .map(|run| {
            (
                run.id,
                AgentRun {
                    id: run.id,
                    status: run.status.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    let tool_events = pages
        .iter()
        .flat_map(|page| &page.tool_calls)
        .map(|call| {
            (
                (call.run_id, call.sequence),
                ToolEvent {
                    run_id: call.run_id,
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    arguments: call.arguments.clone(),
                    result: call.result.clone(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>()
        .into_values()
        .collect();
    let messages = pages
        .into_iter()
        .flat_map(|page| page.messages)
        .map(|message| ChatMessage::restored(message.role, message.run_id, message.text))
        .collect();
    Ok(LoadedSession {
        snapshot: SessionSnapshot {
            id: stored_id,
            run: runs.into_values().next_back(),
            messages,
            tool_events,
        },
        created_at,
    })
}

struct StorageEffect {
    store: Arc<dyn SessionStore>,
    session_id: SessionId,
    run_id: RunId,
    run: FinalizedRun,
}

impl StorageEffect {
    async fn perform_async(self) -> StorageCompletion {
        let result = self
            .store
            .finalize_run(self.run, CancellationToken::default())
            .await;
        StorageCompletion {
            session_id: self.session_id,
            run_id: self.run_id,
            result,
        }
    }
}

impl FeatureEffect for StorageEffect {
    fn perform(self: Box<Self>) -> Message {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("session storage runtime must build")
            .block_on(self.perform_async())
            .into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

struct StorageCompletion {
    session_id: SessionId,
    run_id: RunId,
    result: Result<(), StorageError>,
}

impl StorageCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for StorageCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        model
            .agent
            .runtime_mut()
            .complete_storage(
                &self.session_id,
                self.run_id,
                self.result.map_err(|error| error.to_string()),
            )
            .map(|error| {
                let error = error.chars().take(512).collect::<String>();
                crate::notifications::publish_status(
                    model,
                    StatusMessage::error(format!("Agent session storage failed: {error}")),
                    Some("Agent"),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready(runtime: &AgentRuntime, now: i64) -> FinalizedRun {
        match finalized_run(runtime, None, now) {
            Finalization::Ready(run) => *run,
            Finalization::NotFinalized => panic!("run should be finalized"),
            Finalization::Invalid(error) => panic!("finalization failed: {error}"),
        }
    }

    #[test]
    fn finalized_record_uses_runtime_timestamps_without_ui_state() {
        let mut runtime = AgentRuntime::default();
        let run = runtime.begin("question".to_string(), None, 10).unwrap().id;
        runtime.session_mut().complete(run);
        let created = runtime.session_created_at();
        let started = runtime.run_started_at(run);

        let record = ready(&runtime, 123);

        assert_eq!(record.session.created_at, created.unwrap());
        assert_eq!(record.run.created_at, started.unwrap());
        assert_eq!(record.run.completed_at, Some(123));
        assert_eq!(runtime.session_created_at(), created);
        assert_eq!(runtime.run_started_at(run), started);
    }

    #[test]
    fn running_run_is_not_finalized() {
        let mut runtime = AgentRuntime::default();
        runtime.begin("question".to_string(), None, 10).unwrap();

        assert!(matches!(
            finalized_run(&runtime, None, 20),
            Finalization::NotFinalized
        ));
    }

    #[test]
    fn missing_run_metadata_is_an_explicit_invalid_outcome() {
        let mut runtime = AgentRuntime::default();
        runtime
            .restore_session(
                SessionSnapshot {
                    id: SessionId::from("stored"),
                    run: Some(AgentRun {
                        id: RunId(4),
                        status: eggplant_agent::RunStatus::Complete,
                    }),
                    messages: Vec::new(),
                    tool_events: Vec::new(),
                },
                10,
            )
            .unwrap();

        assert!(matches!(
            finalized_run(&runtime, None, 20),
            Finalization::Invalid(FinalizationError::MissingRunStartedAt(RunId(4)))
        ));
    }

    #[test]
    fn finalized_record_projects_only_the_current_run() {
        let mut runtime = AgentRuntime::default();
        runtime.set_provider_metadata("test".to_string(), Some("model".to_string()));
        let first = runtime
            .begin("first question".to_string(), None, 10)
            .unwrap()
            .id;
        runtime.session_mut().append_delta(first, "first answer");
        runtime.session_mut().complete(first);
        let second = runtime
            .begin("second question".to_string(), None, 30)
            .unwrap()
            .id;
        runtime.session_mut().append_delta(second, "second answer");
        let call = eggplant_agent::ToolCall {
            id: "read-1".to_string(),
            name: "read".to_string(),
            arguments: "{}".to_string(),
        };
        runtime.session_mut().record_tool_call(second, &call);
        runtime.session_mut().complete_tool_call(
            second,
            eggplant_agent::ToolResult {
                call_id: call.id,
                status: eggplant_agent::ToolResultStatus::Success,
                content: "contents".to_string(),
                truncated: false,
            },
        );
        runtime.session_mut().complete(second);

        let record = ready(&runtime, 40);

        assert_eq!(record.run.id, second);
        assert_eq!(record.run.provider, "test");
        assert_eq!(record.run.model.as_deref(), Some("model"));
        assert_eq!(record.session.title.as_deref(), Some("first question"));
        assert_eq!(
            record
                .messages
                .iter()
                .map(|message| message.text.as_str())
                .collect::<Vec<_>>(),
            ["second question", "second answer"]
        );
        assert_eq!(
            record
                .messages
                .iter()
                .map(|message| (message.sequence, message.created_at))
                .collect::<Vec<_>>(),
            [(0, 30), (1, 40)]
        );
        assert_eq!(record.tool_calls.len(), 1);
        assert_eq!(record.tool_calls[0].run_id, second);
        assert_eq!(record.tool_calls[0].sequence, 0);
        assert_eq!(record.tool_calls[0].created_at, 30);
        assert_eq!(record.tool_calls[0].completed_at, Some(40));
    }

    #[test]
    fn stored_transcript_rehydrates_through_pages_and_continues_run_ids() {
        let (store, _directory) = crate::test_utils::session_store();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let mut source = AgentRuntime::default();
                let first = source
                    .begin("first question".to_string(), None, 10)
                    .unwrap()
                    .id;
                source.session_mut().append_delta(first, "first answer");
                let call = eggplant_agent::ToolCall {
                    id: "read-1".to_string(),
                    name: "read".to_string(),
                    arguments: "{}".to_string(),
                };
                source.session_mut().record_tool_call(first, &call);
                source.session_mut().complete_tool_call(
                    first,
                    eggplant_agent::ToolResult {
                        call_id: call.id,
                        status: eggplant_agent::ToolResultStatus::Success,
                        content: "contents".to_string(),
                        truncated: false,
                    },
                );
                source.session_mut().complete(first);
                store
                    .finalize_run(ready(&source, 20), CancellationToken::default())
                    .await
                    .unwrap();
                let second = source
                    .begin("second question".to_string(), None, 30)
                    .unwrap()
                    .id;
                source.session_mut().append_delta(second, "second answer");
                source.session_mut().complete(second);
                store
                    .finalize_run(ready(&source, 40), CancellationToken::default())
                    .await
                    .unwrap();

                let session_id = source.session().id().clone();
                let LoadedSession {
                    snapshot,
                    created_at,
                } = load_session_page(
                    store.as_ref(),
                    session_id.clone(),
                    CancellationToken::default(),
                    1,
                )
                .await
                .unwrap();
                let mut restored = AgentRuntime::default();
                let outcome = restored.restore_session(snapshot, created_at).unwrap();

                assert_eq!(outcome.session_id, session_id);
                assert_eq!(outcome.last_run_id, Some(second));
                assert_eq!(restored.session_created_at(), Some(10));
                assert_eq!(restored.session().messages().len(), 4);
                assert_eq!(restored.session().tool_events().len(), 1);
                assert_eq!(restored.provider_history().len(), 4);
                let third = restored
                    .begin("third question".to_string(), None, 50)
                    .unwrap();
                assert_eq!(third.id, RunId(second.0 + 1));
                assert_eq!(restored.provider_history().len(), 4);
            });
    }

    #[test]
    fn stale_storage_completion_does_not_change_the_newer_run() {
        let mut model = Model::default();
        let session_id = model.agent.session().id().clone();
        let first = model
            .agent
            .session_mut()
            .begin("first".to_string())
            .unwrap();
        model.agent.runtime_mut().complete_storage(
            &session_id,
            first,
            Err("first failure".to_string()),
        );
        model.agent.session_mut().complete(first);
        let second = model
            .agent
            .session_mut()
            .begin("second".to_string())
            .unwrap();

        let effect = Box::new(StorageCompletion {
            session_id,
            run_id: first,
            result: Ok(()),
        })
        .update(&mut model);

        assert!(effect.is_none());
        assert_eq!(model.agent.storage_error(), Some("first failure"));
        assert_eq!(model.agent.session().run().unwrap().id, second);
    }
}
