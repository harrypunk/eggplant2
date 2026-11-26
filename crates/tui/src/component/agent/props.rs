use eggplant_agent::AgentSession;

use super::AgentPromptProps;
use crate::theme::UiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentModelNotice {
    Unavailable,
}

impl AgentModelNotice {
    pub fn text(self) -> &'static str {
        match self {
            Self::Unavailable => "No model available · /provider or Space a i to log in",
        }
    }
}

#[derive(Clone, Default)]
pub struct AgentFooterProps<'a> {
    pub provider: &'a str,
    pub model: Option<&'a str>,
    pub context_window: Option<u64>,
}

impl<'a> AgentFooterProps<'a> {
    pub fn new(provider: &'a str, model: Option<&'a str>, context_window: Option<u64>) -> Self {
        Self {
            provider,
            model,
            context_window,
        }
    }
}

/// Read-only inputs shared by every presentation of the Agent workflow.
#[derive(Clone)]
pub struct AgentViewProps<'a> {
    pub session: &'a AgentSession,
    pub prompt: AgentPromptProps<'a>,
    pub storage_error: Option<&'a str>,
    pub model_notice: Option<AgentModelNotice>,
    pub footer: AgentFooterProps<'a>,
    pub styles: &'a UiTheme,
}
