mod component;
mod work;

pub use component::{AgentDialog, AgentPanel};
pub use work::{handle_command, register_commands, register_text_input, register_ui};

use editor::BufferId;
use eggplant_agent::{AgentProvider, AgentSession, CancellationToken, RunId};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorContext {
    pub buffer: BufferId,
    pub path: std::path::PathBuf,
    pub revision: i32,
    pub line: usize,
}

#[derive(Debug, Default)]
pub struct AgentState {
    session: AgentSession,
    prompt: String,
    has_draft: bool,
    dialog_open: bool,
    context: Option<EditorContext>,
    provider: Option<Arc<dyn AgentProvider>>,
    active_cancellation: Option<CancellationToken>,
}

impl AgentState {
    pub fn prompt(&self) -> &str {
        &self.prompt
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

    pub fn open_dialog(&mut self, context: EditorContext, label: String) {
        if !self.has_draft {
            self.prompt = label;
            self.has_draft = true;
        }
        self.context = Some(context);
        self.dialog_open = true;
    }

    pub fn close_dialog(&mut self) {
        self.dialog_open = false;
    }

    pub fn insert(&mut self, character: char) {
        self.prompt.push(character);
        self.has_draft = true;
    }

    pub fn backspace(&mut self) {
        self.prompt.pop();
        self.has_draft = true;
    }

    pub fn begin_prompt(&mut self) -> Option<RunId> {
        let prompt = self.prompt.trim().to_string();
        if prompt.is_empty() {
            return None;
        }
        let run_id = self.session.begin(prompt)?;
        self.prompt.clear();
        self.has_draft = false;
        Some(run_id)
    }

    pub fn set_provider(&mut self, provider: Arc<dyn AgentProvider>) {
        self.provider = Some(provider);
    }

    pub fn provider(&self) -> Option<Arc<dyn AgentProvider>> {
        self.provider.clone()
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
}

/// Minimal editor boundary required by the agent workflow.
pub trait AgentContextProvider {
    fn agent_context(&self) -> Result<EditorContext, String>;
}
