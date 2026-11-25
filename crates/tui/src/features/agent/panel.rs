use crate::{
    composition::input::input_context,
    data_object::{Context, FocusTarget, Model, Visibility},
    manager::effect::Effect,
    tools::ToolKind,
};

pub(crate) fn focus_panel_input(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility != Visibility::Displayed
        || model.tools.active_tool != Some(ToolKind::Agent)
    {
        return None;
    }
    if model.workspace.is_none() {
        return super::work::notify(model, "Open a project to use the agent".to_string());
    }
    close_help(model);
    model.agent.prepare_panel_prompt();
    model.current_focus = FocusTarget::ToolsPanel;
    None
}

pub(crate) fn enter_insert_mode(model: &mut Model) -> Option<Effect> {
    if input_context(model) == Some(Context::AgentChat) {
        model.agent.enter_insert_mode();
    }
    None
}

pub(crate) fn enter_normal_mode(model: &mut Model) -> Option<Effect> {
    model.agent.enter_normal_mode();
    None
}

pub(crate) fn toggle_help(model: &mut Model) -> Option<Effect> {
    if model.agent.help_visible() {
        return close_help(model);
    }
    if input_context(model) != Some(Context::AgentChat) || !model.modal.open(Context::AgentChatHelp)
    {
        return None;
    }
    model.agent.toggle_help();
    None
}

pub(crate) fn close_help(model: &mut Model) -> Option<Effect> {
    model.agent.close_help();
    model.modal.close(Context::AgentChatHelp);
    None
}

pub(crate) fn close_panel_input(model: &mut Model) {
    close_help(model);
    model.agent.prepare_panel_prompt();
    crate::focus::focus_editor(model);
}

pub(crate) fn toggle_panel(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility == Visibility::Displayed
        && model.tools.active_tool == Some(ToolKind::Agent)
    {
        model.tools.visibility = Visibility::Hidden;
        if model.current_focus == FocusTarget::ToolsPanel {
            close_panel_input(model);
        }
    } else {
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        return focus_panel_input(model);
    }
    None
}
