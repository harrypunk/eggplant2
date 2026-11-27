use std::sync::Arc;

use eggplant_agent::{
    AgentContinuation, AgentProvider, AgentSession, CancellationToken, RunId, SessionId,
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
        }
    }
}

impl AgentRuntime {
    /// Begin one session run without producing presentation side effects.
    pub fn begin(
        &mut self,
        prompt: String,
        context: Option<EditorContext>,
        now: i64,
    ) -> Option<RunId> {
        if prompt.trim().is_empty() {
            return None;
        }
        let run_id = self.session.begin(prompt)?;
        self.session_created_at.get_or_insert(now);
        self.run = Some(AgentRunState::new(run_id, now, context));
        Some(run_id)
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
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
        assert_eq!(runtime.session().run().unwrap().id, first);
    }
}
