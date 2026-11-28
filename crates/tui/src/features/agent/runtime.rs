use std::sync::Arc;

use eggplant_agent::{
    AgentContinuation, AgentEvent, AgentProvider, AgentSession, CancellationToken, ProviderMessage,
    RunId, SessionId, SessionRestoreError, SessionSnapshot, ThinkingId, ThinkingOption,
};
use eggplant_agent_storage::SessionStore;

use super::{EditorContext, run::AgentRunState};

/// Runtime state and lifecycle operations independent of Agent presentation.
#[derive(Debug)]
pub struct AgentRuntime {
    session: AgentSession,
    provider: Option<Arc<dyn AgentProvider>>,
    run: Option<AgentRunState>,
    store: Option<Arc<dyn SessionStore>>,
    storage_error: Option<String>,
    session_created_at: Option<i64>,
    provider_name: String,
    model: Option<String>,
    thinking_id: Option<ThinkingId>,
}

impl Default for AgentRuntime {
    fn default() -> Self {
        Self {
            session: AgentSession::default(),
            provider: None,
            run: None,
            store: None,
            storage_error: None,
            session_created_at: None,
            provider_name: "fake".to_string(),
            model: None,
            thinking_id: None,
        }
    }
}

/// Semantic result of reducing one provider event.
pub enum AgentApplyOutcome {
    None,
    ApprovalRequired(AgentContinuation),
}

/// Runtime outcome emitted after a session run starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunStarted {
    pub id: RunId,
    pub started_at: i64,
}

/// Semantic result of replacing a finalized session without presentation side effects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRestored {
    pub session_id: SessionId,
    pub last_run_id: Option<RunId>,
}

impl AgentRuntime {
    /// Begin one session run without producing presentation side effects.
    pub fn begin(
        &mut self,
        prompt: String,
        context: Option<EditorContext>,
        now: i64,
    ) -> Option<RunStarted> {
        if prompt.trim().is_empty() {
            return None;
        }
        let run_id = self.session.begin(prompt)?;
        self.session_created_at.get_or_insert(now);
        self.run = Some(AgentRunState::new(run_id, now, context));
        Some(RunStarted {
            id: run_id,
            started_at: now,
        })
    }

    pub fn restore_session(
        &mut self,
        snapshot: SessionSnapshot,
        created_at: i64,
    ) -> Result<SessionRestored, SessionRestoreError> {
        if self.session.is_running() {
            return Err(SessionRestoreError::RunInProgress);
        }
        let session = AgentSession::restore(snapshot)?;
        let restored = SessionRestored {
            session_id: session.id().clone(),
            last_run_id: session.run().map(|run| run.id),
        };
        self.session = session;
        self.run = None;
        self.session_created_at = Some(created_at);
        self.storage_error = None;
        Ok(restored)
    }

    pub fn apply_event(&mut self, run_id: RunId, event: AgentEvent) -> AgentApplyOutcome {
        match event {
            AgentEvent::TextDelta(delta) => {
                self.session.append_delta(run_id, &delta);
                AgentApplyOutcome::None
            }
            AgentEvent::ThinkingDelta(delta) => {
                self.session.append_thinking_delta(run_id, &delta);
                AgentApplyOutcome::None
            }
            AgentEvent::Usage(usage) => {
                self.session.record_usage(run_id, usage);
                AgentApplyOutcome::None
            }
            AgentEvent::Complete => {
                self.session.complete(run_id);
                AgentApplyOutcome::None
            }
            AgentEvent::ToolCalls(calls) => {
                calls.into_iter().for_each(|call| {
                    self.session.record_tool_call(run_id, &call);
                });
                AgentApplyOutcome::None
            }
            AgentEvent::ToolResults(results) => {
                results.into_iter().for_each(|result| {
                    self.session.complete_tool_call(run_id, result);
                });
                AgentApplyOutcome::None
            }
            AgentEvent::ApprovalRequired {
                approval,
                continuation,
            } => {
                if self.session.request_approval(run_id, approval) {
                    self.suspend(run_id, continuation.clone());
                    AgentApplyOutcome::ApprovalRequired(continuation)
                } else {
                    AgentApplyOutcome::None
                }
            }
            AgentEvent::Failed(error) => {
                self.session.fail(run_id, error);
                AgentApplyOutcome::None
            }
        }
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }

    pub fn provider_history(&self) -> Vec<ProviderMessage> {
        self.session.provider_history()
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.provider_name = "custom".to_string();
        self.provider = Some(provider);
    }

    pub fn set_provider_metadata(&mut self, name: String, model: Option<String>) {
        self.provider_name = name;
        self.model = model;
    }

    pub fn clear_provider(&mut self) {
        self.provider = None;
        self.provider_name = "fake".to_string();
        self.model = None;
    }

    pub fn provider(&self) -> Option<Arc<dyn AgentProvider>> {
        self.provider.clone()
    }

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn thinking_id(&self) -> Option<ThinkingId> {
        self.thinking_id.clone()
    }

    pub fn set_thinking_id(&mut self, id: ThinkingId) {
        self.thinking_id = Some(id);
    }

    pub fn thinking_options(&self) -> &[ThinkingOption] {
        self.provider
            .as_ref()
            .map_or(&[], |provider| provider.thinking_options())
    }

    pub fn set_session_store(&mut self, store: Arc<dyn SessionStore>) {
        self.store = Some(store);
    }

    pub(crate) fn session_store(&self) -> Option<Arc<dyn SessionStore>> {
        self.store.clone()
    }

    pub(crate) fn storage_error(&self) -> Option<&str> {
        self.storage_error.as_deref()
    }

    pub(crate) fn session_created_at(&self) -> Option<i64> {
        self.session_created_at
    }

    pub(crate) fn run_started_at(&self, run_id: RunId) -> Option<i64> {
        self.run.as_ref().and_then(|run| run.started_at(run_id))
    }

    pub(crate) fn run_context(&self, run_id: RunId) -> Option<&EditorContext> {
        self.run.as_ref().and_then(|run| run.editor_context(run_id))
    }

    pub(crate) fn complete_storage(
        &mut self,
        session_id: &SessionId,
        run_id: RunId,
        result: Result<(), String>,
    ) -> Option<String> {
        let current = self.session.id() == session_id
            && self.session.run().is_some_and(|run| run.id == run_id);
        if !current {
            return None;
        }
        self.storage_error = result.err();
        self.storage_error.clone()
    }

    pub fn begin_cancellation(&mut self) -> CancellationToken {
        self.run
            .as_ref()
            .map(|run| run.cancellation())
            .unwrap_or_default()
    }

    pub fn cancel_active(&self) {
        if let Some(run) = &self.run {
            run.cancel();
        }
    }

    pub(crate) fn suspend(&mut self, run_id: RunId, continuation: AgentContinuation) {
        if let Some(run) = &mut self.run {
            run.suspend(run_id, continuation);
        }
    }

    pub(crate) fn take_suspension(&mut self, run_id: RunId) -> Option<AgentContinuation> {
        self.run
            .as_mut()
            .and_then(|run| run.take_suspension(run_id))
    }

    pub(crate) fn clear_suspension(&mut self) {
        if let Some(run) = &mut self.run {
            run.clear_suspension();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_begin_has_no_presentation_requirements() {
        let mut runtime = AgentRuntime::default();
        assert!(runtime.begin("".to_string(), None, 1).is_none());

        let first = runtime.begin("hello".to_string(), None, 123).unwrap();
        assert_eq!(runtime.session_created_at(), Some(123));
        assert!(runtime.session().is_running());
        assert_eq!(runtime.session().run().unwrap().id, first.id);
    }

    #[test]
    fn restore_does_not_replace_an_active_session() {
        let mut runtime = AgentRuntime::default();
        let session_id = runtime.session().id().clone();
        let active = runtime.begin("active".to_string(), None, 10).unwrap().id;
        let snapshot = SessionSnapshot {
            id: SessionId::from("stored"),
            run: Some(eggplant_agent::AgentRun {
                id: RunId(9),
                status: eggplant_agent::RunStatus::Complete,
            }),
            messages: Vec::new(),
            tool_events: Vec::new(),
        };

        assert_eq!(
            runtime.restore_session(snapshot, 1),
            Err(SessionRestoreError::RunInProgress)
        );
        assert_eq!(runtime.session().id(), &session_id);
        assert_eq!(runtime.session().run().unwrap().id, active);
        assert!(runtime.session().is_running());
    }
}
