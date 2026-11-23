use eggplant_agent::AgentSession;

use crate::{agent::PromptCommand, component::text_input::TextInput, theme::UiTheme};

/// Read-only inputs shared by every presentation of the Agent workflow.
#[derive(Clone)]
pub struct AgentViewProps<'a> {
    pub session: &'a AgentSession,
    pub prompt: &'a TextInput,
    pub commands: Option<Vec<PromptCommand>>,
    pub selected_command: Option<PromptCommand>,
    pub storage_error: Option<&'a str>,
    pub styles: &'a UiTheme,
}
