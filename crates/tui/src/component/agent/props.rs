use eggplant_agent::AgentSession;

use super::AgentPromptProps;
use crate::theme::UiTheme;

/// Read-only inputs shared by every presentation of the Agent workflow.
#[derive(Clone)]
pub struct AgentViewProps<'a> {
    pub session: &'a AgentSession,
    pub prompt: AgentPromptProps<'a>,
    pub storage_error: Option<&'a str>,
    pub styles: &'a UiTheme,
}
