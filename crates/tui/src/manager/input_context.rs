use crate::{
    agent::AgentInputMode,
    data_object::{AppPage, Context, FocusTarget, Model, Visibility},
    tools::ToolKind,
};

struct ContextRule {
    context: Context,
    matches: fn(&Model) -> bool,
}

impl ContextRule {
    const fn new(context: Context, matches: fn(&Model) -> bool) -> Self {
        Self { context, matches }
    }
}

const SHELL_RULES: &[ContextRule] = &[
    ContextRule::new(Context::VcsCommit, |model| {
        model.active_page == AppPage::VcsCommit
    }),
    ContextRule::new(Context::VcsHistory, |model| {
        model.active_page == AppPage::VcsHistory
    }),
    ContextRule::new(Context::VCS, |model| model.active_page == AppPage::Vcs),
    ContextRule::new(Context::AgentChatInsert, |model| {
        agent_panel_is(model, AgentInputMode::Insert)
    }),
    ContextRule::new(Context::AgentChat, |model| {
        agent_panel_is(model, AgentInputMode::Normal)
    }),
    ContextRule::new(Context::Explorer, |model| {
        model.current_focus == FocusTarget::FileExplorer
    }),
    ContextRule::new(Context::Welcome, |model| {
        model.current_focus == FocusTarget::Editor && model.editor.active_buffer().is_none()
    }),
    ContextRule::new(Context::AppNormal, |_| true),
];

/// Resolve which context owns input, giving transient workflows precedence over the shell.
///
/// The stack contains only transient workflows with an explicit lifetime: pickers, dialogs,
/// pending operators, and similar states that are pushed on entry and popped on exit. It must
/// not be used to synchronize shell state such as focus, the active page, or Agent panel mode.
///
/// When no transient workflow is active, the shell context is derived from source-of-truth UI
/// state. This avoids stale stack entries when focus, buffers, pages, or panel modes change.
pub fn input_context(model: &Model) -> Option<Context> {
    model
        .context_stack
        .last()
        .copied()
        .or_else(|| shell_context(model))
}

/// Resolve the persistent shell surface underneath all transient workflows.
fn shell_context(model: &Model) -> Option<Context> {
    SHELL_RULES
        .iter()
        .find(|rule| (rule.matches)(model))
        .map(|rule| rule.context)
}

fn agent_panel_is(model: &Model, mode: AgentInputMode) -> bool {
    model.current_focus == FocusTarget::ToolsPanel
        && model.tools.visibility == Visibility::Displayed
        && model.tools.active_tool == Some(ToolKind::Agent)
        && model.agent.panel_mode() == mode
}

#[cfg(test)]
mod tests {
    use super::*;

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

        model.agent.enter_insert_mode();
        assert_eq!(input_context(&model), Some(Context::AgentChatInsert));
        model.agent.enter_normal_mode();
        assert_eq!(input_context(&model), Some(Context::AgentChat));

        model.current_focus = FocusTarget::Editor;
        assert_eq!(input_context(&model), Some(Context::Welcome));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn transient_workflows_take_precedence_until_popped() {
        let mut model = Model {
            current_focus: FocusTarget::FileExplorer,
            ..Model::default()
        };
        assert_eq!(input_context(&model), Some(Context::Explorer));

        model.context_stack.push(Context::FilePicker);
        assert_eq!(input_context(&model), Some(Context::FilePicker));

        model.current_focus = FocusTarget::ToolsPanel;
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        assert_eq!(input_context(&model), Some(Context::FilePicker));

        model.context_stack.pop();
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
