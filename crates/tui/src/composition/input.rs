use std::sync::Arc;

use crate::{
    data_object::{AppPage, Context, Model},
    manager::input_routing::InputRouting,
    page,
};

/// Application router that delegates shell policy to the active page.
#[derive(Debug, Default)]
pub struct ApplicationInputRouting;

impl InputRouting for ApplicationInputRouting {
    fn context(&self, model: &Model) -> Option<Context> {
        model
            .modal
            .active_context()
            .or_else(|| model.context_stack.last().copied())
            .or_else(|| page_context(model))
    }
}

pub fn routing() -> Arc<dyn InputRouting> {
    Arc::new(ApplicationInputRouting)
}

fn page_context(model: &Model) -> Option<Context> {
    match model.active_page {
        AppPage::Editor => page::editor::input_context(model),
        AppPage::Vcs => Some(page::vcs::INPUT_CONTEXT),
        AppPage::VcsHistory => Some(page::vcs_history::INPUT_CONTEXT),
        AppPage::VcsCommit => page::vcs_commit::input_context(model),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data_object::{FocusTarget, Visibility},
        tools::ToolKind,
    };

    fn input_context(model: &Model) -> Option<Context> {
        ApplicationInputRouting.context(model)
    }

    #[test]
    fn derives_welcome_only_for_the_focused_empty_editor() {
        let mut model = Model::default();
        assert_eq!(input_context(&model), Some(Context::Welcome));

        model.current_focus = FocusTarget::FileExplorer;
        assert_eq!(input_context(&model), Some(Context::Explorer));

        model.current_focus = FocusTarget::Editor;
        crate::test_utils::create_buffer(&mut model, "text");
        assert_eq!(input_context(&model), Some(Context::AppNormal));
    }

    #[test]
    fn derives_agent_panel_modes_without_storing_shell_contexts() {
        let mut model = Model {
            current_focus: FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::AgentChat));

        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::EnterInsert,
        ));
        assert_eq!(input_context(&model), Some(Context::AgentChatInsert));
        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::EnterNormal,
        ));
        assert_eq!(input_context(&model), Some(Context::AgentChat));

        model.current_focus = FocusTarget::Editor;
        assert_eq!(input_context(&model), Some(Context::Welcome));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn editor_modes_follow_session_state_without_stack_entries() {
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "text");
        assert_eq!(input_context(&model), Some(Context::AppNormal));

        model.editor.enter_insert_mode();
        assert_eq!(input_context(&model), Some(Context::EditorInput));
        model.editor.enter_normal_mode();
        assert_eq!(input_context(&model), Some(Context::AppNormal));

        model.editor.enter_visual_mode();
        assert_eq!(input_context(&model), Some(Context::EditorVisual));
        model.editor.enter_visual_line_mode();
        assert_eq!(input_context(&model), Some(Context::EditorVisual));
        model.editor.enter_normal_mode();

        model.editor.enter_insert_mode();
        model.current_focus = FocusTarget::FileExplorer;
        assert_eq!(input_context(&model), Some(Context::Explorer));
        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        model.current_focus = FocusTarget::Editor;
        assert_eq!(input_context(&model), Some(Context::EditorInput));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn commit_editor_modes_follow_the_commit_session() {
        let mut model = Model {
            active_page: AppPage::VcsCommit,
            ..Model::default()
        };
        model
            .vcs
            .commit
            .editor
            .insert_document(editor::DocumentFactory::new().create(""));
        assert_eq!(input_context(&model), Some(Context::VcsCommit));

        model.vcs.commit.editor.enter_insert_mode();
        assert_eq!(input_context(&model), Some(Context::VcsCommitInput));
        model.vcs.commit.editor.enter_normal_mode();
        model.vcs.commit.editor.enter_visual_mode();
        assert_eq!(input_context(&model), Some(Context::EditorVisual));
        model.vcs.commit.editor.enter_normal_mode();
        assert_eq!(input_context(&model), Some(Context::VcsCommit));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn modal_sessions_and_transient_workflows_take_precedence_in_order() {
        let mut model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };
        assert_eq!(input_context(&model), Some(Context::Explorer));

        model.context_stack.push(Context::EditorYank);
        assert_eq!(input_context(&model), Some(Context::EditorYank));
        assert!(model.modal.open(Context::FilePicker));
        assert_eq!(input_context(&model), Some(Context::FilePicker));

        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::FilePicker));

        model.modal.close(Context::FilePicker);
        assert_eq!(input_context(&model), Some(Context::EditorYank));
        assert_eq!(model.context_stack.pop(), Some(Context::EditorYank));
        assert_eq!(input_context(&model), Some(Context::AgentChat));
    }

    #[test]
    fn non_editor_pages_supply_their_shell_context() {
        let mut model = Model {
            active_page: AppPage::Vcs,
            ..Model::default()
        };
        assert_eq!(input_context(&model), Some(Context::VCS));

        model.active_page = AppPage::VcsHistory;
        assert_eq!(input_context(&model), Some(Context::VcsHistory));
        model.active_page = AppPage::VcsCommit;
        assert_eq!(input_context(&model), Some(Context::VcsCommit));

        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::VcsCommit));
        assert!(model.context_stack.is_empty());
    }
}
