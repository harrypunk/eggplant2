use crate::{
    data_object::{Context, Model},
    manager::input_context::input_context,
    tools::ToolKind,
};

/// Presentation-independent identity of the active Agent workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSurface {
    Dialog,
    Panel,
}

pub fn active(model: &Model) -> Option<AgentSurface> {
    match input_context(model)? {
        Context::AgentChatDialog if model.agent.dialog_open() => Some(AgentSurface::Dialog),
        Context::AgentChat | Context::AgentChatInsert
            if model.tools.active_tool == Some(ToolKind::Agent) =>
        {
            Some(AgentSurface::Panel)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_object::{FocusTarget, Visibility};

    #[test]
    fn selects_the_workflow_not_its_presentation_details() {
        let mut model = Model::default();
        model.agent.open_dialog(
            Some(crate::agent::EditorContext {
                buffer: editor::BufferId::from_raw(1),
                path: std::path::PathBuf::from("/workspace/main.rs"),
                revision: 1,
                line: 1,
            }),
            "main.rs:1".to_string(),
        );
        assert!(model.modal.open(Context::AgentChatDialog));
        assert_eq!(active(&model), Some(AgentSurface::Dialog));

        model.modal.close(Context::AgentChatDialog);
        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(active(&model), Some(AgentSurface::Panel));

        model.agent.enter_insert_mode();
        assert_eq!(active(&model), Some(AgentSurface::Panel));
        model.current_focus = FocusTarget::Editor;
        assert_eq!(active(&model), None);
    }
}
