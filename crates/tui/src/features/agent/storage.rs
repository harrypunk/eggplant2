use std::{path::Path, sync::Arc};

use eggplant_agent::{CancellationToken, MessageRole, RunId, SessionId};
use eggplant_agent_storage::{
    FinalizedRun, MessageRecord, RunRecord, SessionRecord, SessionStore, StorageError,
    ToolCallRecord,
};

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
