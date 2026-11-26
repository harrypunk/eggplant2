use crate::data_object::{Context, Model};

/// Presentation-independent identity of the active Agent workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentSurface {
    Dialog,
    Panel,
}

pub fn active(model: &Model) -> Option<AgentSurface> {
    match (
        model.modal.is_active(Context::AgentChatDialog) && model.agent.dialog_open(),
        super::panel::input_available(model),
    ) {
        (true, _) => Some(AgentSurface::Dialog),
        (false, true) => Some(AgentSurface::Panel),
        (false, false) => None,
    }
}

/// Select the one surface whose editable prompt may present command candidates.
pub fn prompt_active(model: &Model) -> Option<AgentSurface> {
    active(model).filter(|surface| match surface {
        AgentSurface::Dialog => true,
        AgentSurface::Panel => super::panel::insert_input_available(model),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{FocusTarget, Visibility},
        tools::ToolKind,
    };

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
