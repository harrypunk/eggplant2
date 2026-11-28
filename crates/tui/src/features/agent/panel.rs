use super::{AgentUiAction, PanelAction};
use crate::{
    data_object::{AppPage, Context, FocusTarget, Model, Visibility},
    manager::effect::Effect,
    tools::ToolKind,
};

pub(super) fn input_available(model: &Model) -> bool {
    model.active_page == AppPage::Editor
        && model.current_focus == FocusTarget::ToolsPanel
        && model.modal.active_context().is_none()
        && model.context_stack.is_empty()
        && model.tools.input_context(&model.agent).is_some()
}

pub(super) fn insert_input_available(model: &Model) -> bool {
    input_available(model) && model.agent.panel_mode() == super::AgentInputMode::Insert
}

fn normal_input_available(model: &Model) -> bool {
    input_available(model) && model.agent.panel_mode() == super::AgentInputMode::Normal
}

pub(crate) fn focus_panel_input(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility != Visibility::Displayed
        || model.tools.active_tool != Some(ToolKind::Agent)
    {
        return None;
    }
    if model.workspace.is_none() {
        return super::adapter::notify(model, "Open a project to use the agent".to_string());
    }
    close_help(model);
    model.agent.dispatch(AgentUiAction::PreparePanel);
    model.current_focus = FocusTarget::ToolsPanel;
    None
}

pub(crate) fn enter_insert_mode(model: &mut Model) -> Option<Effect> {
    if normal_input_available(model) {
        model
            .agent
            .dispatch(AgentUiAction::Panel(PanelAction::EnterInsert));
    }
    None
}

pub(crate) fn enter_normal_mode(model: &mut Model) -> Option<Effect> {
    model
        .agent
        .dispatch(AgentUiAction::Panel(PanelAction::EnterNormal));
    None
}

pub(crate) fn toggle_help(model: &mut Model) -> Option<Effect> {
    if model.agent.help_visible() {
        return close_help(model);
    }
    if !normal_input_available(model) || !model.modal.open(Context::AgentChatHelp) {
        return None;
    }
    model
        .agent
        .dispatch(AgentUiAction::Panel(PanelAction::ToggleHelp));
    None
}

pub(crate) fn close_help(model: &mut Model) -> Option<Effect> {
    model
        .agent
        .dispatch(AgentUiAction::Panel(PanelAction::CloseHelp));
    model.modal.close(Context::AgentChatHelp);
    None
}

pub(crate) fn close_panel_input(model: &mut Model) {
    close_help(model);
    model.agent.dispatch(AgentUiAction::PreparePanel);
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
