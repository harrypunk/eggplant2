mod approval;
mod commands;
mod completion;
mod context;
mod dialog;
mod panel;
mod prompt;
mod prompt_command;
mod run;
mod storage;
mod surface;
mod tools;
mod ui;
mod work;
mod write;

pub(crate) use commands::handle_input;
pub use commands::{commands, general_action_subscriptions, handle_command, handle_general_action};
pub(crate) use dialog::{resume_overlay, suspend_for_overlay};
pub(crate) use panel::{close_panel_input, focus_panel_input};
pub(crate) use prompt_command::{PromptCommand, PromptCommandMovement, PromptCommandState};
pub use ui::register_ui;
pub(crate) use ui::render_panel;

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AgentDialogState {
    #[default]
    Closed,
    Open,
    Suspended,
}

#[derive(Debug)]
pub struct AgentState {
    session: AgentSession,
    prompt: prompt::AgentPrompt,
    dialog: AgentDialogState,
    panel_mode: AgentInputMode,
    help_visible: bool,
    context: Option<EditorContext>,
    provider: Option<Arc<dyn AgentProvider>>,
    run: Option<run::AgentRunState>,
    store: Option<Arc<dyn SessionStore>>,
    storage_error: Option<String>,
    session_created_at: Option<i64>,
    provider_name: String,
    model: Option<String>,
}

impl Default for AgentState {
    fn default() -> Self {
        Self {
            session: AgentSession::default(),
            prompt: prompt::AgentPrompt::default(),
            dialog: AgentDialogState::Closed,
            panel_mode: AgentInputMode::Normal,
            help_visible: false,
            context: None,
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

impl AgentState {
    pub fn prompt(&self) -> &str {
        self.prompt.value()
    }

    pub fn prompt_state(&self) -> &TextInput {
        self.prompt.input()
    }

    pub fn command_state(&self) -> &PromptCommandState {
        self.prompt.command_state()
    }

    pub fn quick_fill_prompt_command(&mut self, movement: PromptCommandMovement) -> bool {
        self.prompt.quick_fill(movement)
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

    pub fn dialog_state(&self) -> AgentDialogState {
        self.dialog
    }

    /// Whether an inline workflow exists, including while temporarily suspended.
    pub fn dialog_open(&self) -> bool {
        self.dialog != AgentDialogState::Closed
    }

    pub fn suspend_dialog(&mut self) {
        if self.dialog == AgentDialogState::Open {
            self.dialog = AgentDialogState::Suspended;
        }
    }

    pub fn resume_dialog(&mut self) {
        if self.dialog == AgentDialogState::Suspended {
            self.dialog = AgentDialogState::Open;
        }
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

    pub fn prepare_prompt(&mut self, context: Option<EditorContext>, label: String) {
        self.prompt.prepare_dialog_prefill(label);
        self.context = context;
    }

    pub fn prepare_panel_prompt(&mut self) {
        self.prompt.prepare_panel();
        self.context = None;
        self.panel_mode = AgentInputMode::Normal;
        self.help_visible = false;
    }

    pub fn open_dialog(&mut self, context: Option<EditorContext>, label: String) {
        self.prepare_prompt(context, label);
        self.dialog = AgentDialogState::Open;
    }

    pub fn close_dialog(&mut self) {
        self.dialog = AgentDialogState::Closed;
    }

    pub fn handle_input_event(&mut self, event: TextInputEvent) {
        self.prompt.edit(event);
    }

    pub fn backspace(&mut self) {
        self.prompt.backspace();
    }

    pub fn begin_prompt(&mut self) -> Option<RunId> {
        let prompt = self.prompt.submission_text()?;
        let run_id = self.session.begin(prompt)?;
        self.prompt.complete_submission();
        let now = chrono::Utc::now().timestamp_millis();
        self.session_created_at.get_or_insert(now);
        self.run = Some(run::AgentRunState::new(run_id, now, self.context.clone()));
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

    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_session_store(&mut self, store: Arc<dyn SessionStore>) {
        self.store = Some(store);
    }

    fn session_store(&self) -> Option<Arc<dyn SessionStore>> {
        self.store.clone()
    }

    fn storage_error(&self) -> Option<&str> {
        self.storage_error.as_deref()
    }

    fn session_created_at(&self) -> Option<i64> {
        self.session_created_at
    }

    fn run_started_at(&self, run_id: RunId) -> Option<i64> {
        self.run.as_ref().and_then(|run| run.started_at(run_id))
    }

    fn run_context(&self, run_id: RunId) -> Option<&EditorContext> {
        self.run.as_ref().and_then(|run| run.editor_context(run_id))
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

    fn suspend(&mut self, run_id: RunId, continuation: AgentContinuation) {
        if let Some(run) = &mut self.run {
            run.suspend(run_id, continuation);
        }
    }

    fn take_suspension(&mut self, run_id: RunId) -> Option<AgentContinuation> {
        self.run
            .as_mut()
            .and_then(|run| run.take_suspension(run_id))
    }

    fn clear_suspension(&mut self) {
        if let Some(run) = &mut self.run {
            run.clear_suspension();
        }
    }
}
