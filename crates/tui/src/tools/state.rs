use crate::{
    agent::AgentState,
    data_object::{Context, Visibility},
};

/// Tools available in the shared side-panel shell.
///
/// Each implementation owns its state and behavior in a separate module. This
/// enum selects which tool receives input and rendering inside the panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    Search,
    Agent,
    Info,
    Diagnostics,
}

/// State shared by all tools in the side panel.
///
/// Hiding the panel must preserve the active tool and its independent state.
/// Tool-specific fields should live in their own modules, not accumulate here.
#[derive(Debug)]
pub struct ToolsPanelState {
    pub visibility: Visibility,
    pub active_tool: Option<ToolKind>,
}

impl ToolsPanelState {
    /// Delegate tool-specific input policy to the active tool.
    pub fn input_context(&self, agent: &AgentState) -> Option<Context> {
        match (self.visibility, self.active_tool) {
            (Visibility::Displayed, Some(ToolKind::Agent)) => Some(agent.panel_input_context()),
            _ => None,
        }
    }
}

impl Default for ToolsPanelState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            active_tool: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_starts_hidden_without_an_active_tool() {
        let state = ToolsPanelState::default();

        assert_eq!(state.visibility, Visibility::Hidden);
        assert_eq!(state.active_tool, None);
    }
}
