mod component;
mod help;
mod storage;
mod tools;
mod work;
mod write;

pub use component::{AgentDialog, AgentPanel};
pub(crate) use work::{close_panel_input, focus_panel_input};
pub use work::{handle_command, register_commands, register_text_input, register_ui};

use crate::component::text_input::{TextInput, TextInputEvent};
use editor::BufferId;
use eggplant_agent::{
    AgentContinuation, AgentProvider, AgentSession, CancellationToken, RunId, SessionId,
};
use eggplant_agent_storage::SessionStore;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorContext {
    pub buffer: BufferId,
    pub path: std::path::PathBuf,
    pub revision: i32,
    pub line: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AgentInputMode {
    #[default]
    Normal,
    Insert,
}

impl AgentInputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Insert => "Insert",
        }
    }
}

#[derive(Debug, Clone)]
struct SuspendedRun {
    run_id: RunId,
    continuation: AgentContinuation,
}

#[derive(Debug)]
pub struct AgentState {
    session: AgentSession,
    prompt: TextInput,
    has_draft: bool,
    dialog_open: bool,
    panel_mode: AgentInputMode,
    help_visible: bool,
    context: Option<EditorContext>,
    provider: Option<Arc<dyn AgentProvider>>,
    active_cancellation: Option<CancellationToken>,
    suspended: Option<SuspendedRun>,
    store: Arc<dyn SessionStore>,
    storage_error: Option<String>,
    session_created_at: Option<i64>,
    run_started_at: Option<(RunId, i64)>,
    run_context: Option<(RunId, EditorContext)>,
    provider_name: String,
    model: Option<String>,
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            session: AgentSession::default(),
            prompt: TextInput::default(),
            has_draft: false,
            dialog_open: false,
            panel_mode: AgentInputMode::Normal,
            help_visible: false,
            context: None,
            provider: None,
            active_cancellation: None,
            suspended: None,
            store: Arc::new(eggplant_agent_storage::InMemoryStore::new()),
            storage_error: None,
            session_created_at: None,
            run_started_at: None,
            run_context: None,
            provider_name: "fake".to_string(),
            model: None,
        }
    }
}

impl AgentState {
    pub fn prompt(&self) -> &str {
        self.prompt.value()
    }

    pub fn prompt_state(&self) -> &TextInput {
        &self.prompt
    }

    pub fn panel_mode(&self) -> AgentInputMode {
        self.panel_mode
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub fn enter_insert_mode(&mut self) {
        self.panel_mode = AgentInputMode::Insert;
        self.help_visible = false;
    }

    pub fn enter_normal_mode(&mut self) {
        self.panel_mode = AgentInputMode::Normal;
    }

    pub fn toggle_help(&mut self) {
        self.help_visible = !self.help_visible;
    }

    pub fn close_help(&mut self) {
        self.help_visible = false;
    }

    pub fn dialog_open(&self) -> bool {
        self.dialog_open
    }

    pub fn session(&self) -> &AgentSession {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut AgentSession {
        &mut self.session
    }

    pub fn context(&self) -> Option<&EditorContext> {
        self.context.as_ref()
    }

    pub fn prepare_prompt(&mut self, context: EditorContext, label: String) {
        if !self.has_draft {
            self.prompt.set_value(label);
            self.has_draft = true;
        }
        self.context = Some(context);
    }

    pub fn prepare_panel_prompt(&mut self) {
        self.context = None;
        self.panel_mode = AgentInputMode::Normal;
        self.help_visible = false;
    }

    pub fn open_dialog(&mut self, context: EditorContext, label: String) {
        self.prepare_prompt(context, label);
        self.dialog_open = true;
    }

    pub fn close_dialog(&mut self) {
        self.dialog_open = false;
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        self.prompt.handle_event(event);
        self.has_draft = true;
    }

    pub fn backspace(&mut self) {
        self.prompt.backspace();
        self.has_draft = true;
    }

    pub fn begin_prompt(&mut self) -> Option<RunId> {
        let prompt = self.prompt.trim().to_string();
        if prompt.is_empty() {
            return None;
        }
        let run_id = self.session.begin(prompt)?;
        self.run_started_at = Some((run_id, chrono::Utc::now().timestamp_millis()));
        self.run_context = self.context.clone().map(|context| (run_id, context));
        self.prompt.clear();
        self.has_draft = false;
        Some(run_id)
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.provider_name = "custom".to_string();
        self.provider = Some(provider);
    }

    pub fn set_provider_metadata(&mut self, name: String, model: Option<String>) {
        self.provider_name = name;
        self.model = model;
    }

    pub fn provider(&self) -> Option<Arc<dyn AgentProvider>> {
        self.provider.clone()
    }

    pub fn set_session_store(&mut self, store: Arc<dyn SessionStore>) {
        self.store = store;
    }

    fn session_store(&self) -> Arc<dyn SessionStore> {
        self.store.clone()
    }

    fn storage_error(&self) -> Option<&str> {
        self.storage_error.as_deref()
    }

    fn session_created_at(&mut self, now: i64) -> i64 {
        *self.session_created_at.get_or_insert(now)
    }

    fn run_started_at(&self, run_id: RunId) -> Option<i64> {
        self.run_started_at
            .filter(|(id, _)| *id == run_id)
            .map(|(_, started)| started)
    }

    fn run_context(&self, run_id: RunId) -> Option<&EditorContext> {
        self.run_context
            .as_ref()
            .filter(|(id, _)| *id == run_id)
            .map(|(_, context)| context)
    }

    fn complete_storage(
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
        let cancellation = CancellationToken::default();
        self.active_cancellation = Some(cancellation.clone());
        cancellation
    }

    pub fn cancel_active(&self) {
        if let Some(cancellation) = &self.active_cancellation {
            cancellation.cancel();
        }
    }

    fn suspend(&mut self, run_id: RunId, continuation: AgentContinuation) {
        self.suspended = Some(SuspendedRun {
            run_id,
            continuation,
        });
    }

    fn take_suspension(&mut self, run_id: RunId) -> Option<AgentContinuation> {
        self.suspended
            .as_ref()
            .is_some_and(|suspended| suspended.run_id == run_id)
            .then_some(())
            .and_then(|_| self.suspended.take())
            .map(|suspended| suspended.continuation)
    }

    fn clear_suspension(&mut self) {
        self.suspended = None;
    }
}

/// Minimal editor boundary required by the agent workflow.
pub trait AgentContextProvider {
    fn agent_context(&self) -> Result<EditorContext, String>;
}
