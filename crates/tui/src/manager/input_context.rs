use crate::{
    agent::AgentInputMode,
    data_object::{AppPage, Context, FocusTarget, Model, Visibility},
    tools::ToolKind,
};

/// Derive active input routing from source-of-truth UI and feature state.
///
/// The stored stack still owns modal/workflow contexts during migration. Shell
/// variants are selectors, not state that focus and lifecycle code must maintain.
pub fn input_context(model: &Model) -> Option<Context> {
    model.context_stack.last().copied().map(|context| {
        if model.active_page != AppPage::Editor {
            return context;
        }
        match context {
            Context::AppNormal
            | Context::Explorer
            | Context::AgentChat
            | Context::AgentChatInsert
                if model.current_focus == FocusTarget::ToolsPanel
                    && model.tools.visibility == Visibility::Displayed
                    && model.tools.active_tool == Some(ToolKind::Agent) =>
            {
                match model.agent.panel_mode() {
                    AgentInputMode::Normal => Context::AgentChat,
                    AgentInputMode::Insert => Context::AgentChatInsert,
                }
            }
            Context::AppNormal
                if model.current_focus == FocusTarget::Editor
                    && model.editor.active_buffer().is_none() =>
            {
                Context::Welcome
            }
            context => context,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normal_model() -> Model {
        let mut model = Model::default();
        model.context_stack.push(Context::AppNormal);
        model
    }

    #[test]
    fn derives_welcome_only_for_the_focused_empty_editor() {
        let mut model = normal_model();
        assert_eq!(input_context(&model), Some(Context::Welcome));

        model.current_focus = FocusTarget::FileExplorer;
        assert_eq!(input_context(&model), Some(Context::AppNormal));

        model.current_focus = FocusTarget::Editor;
        crate::test_utils::create_buffer(&mut model, "text");
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }

    #[test]
    fn derives_agent_panel_modes_from_focus_and_local_state() {
        let mut model = normal_model();
        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::AgentChat));

        model.agent.enter_insert_mode();
        assert_eq!(input_context(&model), Some(Context::AgentChatInsert));
        model.agent.enter_normal_mode();
        assert_eq!(input_context(&model), Some(Context::AgentChat));

        model.current_focus = FocusTarget::Editor;
        assert_eq!(input_context(&model), Some(Context::Welcome));
    }

    #[test]
    fn stored_workflow_and_modal_contexts_take_precedence() {
        let mut model = normal_model();
        model.context_stack.push(Context::Explorer);
        assert_eq!(input_context(&model), Some(Context::Explorer));
        model.context_stack.push(Context::FilePicker);
        assert_eq!(input_context(&model), Some(Context::FilePicker));

        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::FilePicker));
    }

    #[test]
    fn welcome_and_agent_are_not_derived_on_other_pages() {
        let mut model = normal_model();
        model.active_page = AppPage::Vcs;
        assert_eq!(input_context(&model), Some(Context::AppNormal));

        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }
}
