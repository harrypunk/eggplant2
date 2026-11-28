use std::{collections::BTreeMap, path::Path, sync::Arc};

use eggplant_agent::{
    AgentRun, CancellationToken, ChatMessage, MessageRole, RunId, SessionId, SessionSnapshot,
    ToolEvent,
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

use super::AgentRuntime;

pub fn persist_finalized(model: &mut Model) -> Option<Effect> {
    let store = model.agent.runtime().session_store()?;
    let workspace = model.workspace.as_ref().map(|workspace| workspace.root());
    let run = finalized_run(
        model.agent.runtime(),
        workspace,
        chrono::Utc::now().timestamp_millis(),
    )?;
    Some(Effect::feature(StorageEffect {
        store,
        session_id: run.session.id.clone(),
        run_id: run.run.id,
        run,
    }))
}

fn finalized_run(
    runtime: &AgentRuntime,
    workspace: Option<&Path>,
    now: i64,
) -> Option<FinalizedRun> {
    let (run_id, status) = runtime
        .session()
        .run()
        .map(|run| (run.id, run.status.clone()))?;
    if status.is_running() {
        return None;
    }
    let started = runtime.run_started_at(run_id)?;
    let created = runtime.session_created_at()?;
    let session_id = runtime.session().id().clone();
    let editor_context = runtime.run_context(run_id).map(|context| {
        serde_json::json!({
            "buffer_id": context.buffer.as_raw(),
            "path": context.path,
            "revision": context.revision,
            "line": context.line,
        })
        .to_string()
    });
    let messages = runtime
        .session()
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
        .collect::<Vec<_>>();
    let title = runtime
        .session()
        .messages()
        .iter()
        .find(|message| message.role == MessageRole::User)
        .map(|message| message.text.chars().take(160).collect::<String>());
    let tool_calls = runtime
        .session()
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
        .collect();
    Some(FinalizedRun {
        session: SessionRecord {
            id: session_id.clone(),
            created_at: created,
            updated_at: now,
            workspace: workspace.map(|root| root.to_string_lossy().into_owned()),
            title,
        },
        run: RunRecord {
            id: run_id,
            session_id,
            status,
            created_at: started,
            completed_at: Some(now),
            provider: runtime.provider_name().to_string(),
            model: runtime.model().map(str::to_string),
            editor_context,
        },
        messages,
        tool_calls,
    })
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

    #[test]
    fn finalized_record_uses_runtime_timestamps_without_ui_state() {
        let mut runtime = AgentRuntime::default();
        let run = runtime.begin("question".to_string(), None, 10).unwrap().id;
        runtime.session_mut().complete(run);
        let created = runtime.session_created_at();
        let started = runtime.run_started_at(run);

        let record = finalized_run(&runtime, None, 123).unwrap();

        assert_eq!(record.session.created_at, created.unwrap());
        assert_eq!(record.run.created_at, started.unwrap());
        assert_eq!(record.run.completed_at, Some(123));
        assert_eq!(runtime.session_created_at(), created);
        assert_eq!(runtime.run_started_at(run), started);
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
                    .finalize_run(
                        finalized_run(&source, None, 20).unwrap(),
                        CancellationToken::default(),
                    )
                    .await
                    .unwrap();
                let second = source
                    .begin("second question".to_string(), None, 30)
                    .unwrap()
                    .id;
                source.session_mut().append_delta(second, "second answer");
                source.session_mut().complete(second);
                store
                    .finalize_run(
                        finalized_run(&source, None, 40).unwrap(),
                        CancellationToken::default(),
                    )
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
