use ratatui::{Frame, layout::Rect};

use crate::{
    component::agent::{AgentDialog, AgentDialogProps, AgentHelp, AgentPanel, AgentPanelProps},
    data_object::{FocusState, FocusTarget, Model},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("agent", dialog_overlay)?;
    ui.register_overlay("agent_help", help_overlay)
}

fn dialog_overlay(model: &Model) -> Option<OverlayContribution> {
    model.agent.dialog_open().then(|| {
        OverlayContribution::new(
            "agent",
            OverlayKind::Modal,
            OverlayPlacement::Centered(AgentDialog::size(dialog_props(model))),
            render_dialog,
        )
        .with_cursor(dialog_cursor)
    })
}

fn dialog_props(model: &Model) -> AgentDialogProps<'_> {
    AgentDialogProps {
        session: model.agent.session(),
        prompt: model.agent.prompt_state(),
        storage_error: model.agent.storage_error(),
        styles: model.theme.ui(),
    }
}

fn render_dialog(model: &Model, frame: &mut Frame, area: Rect) {
    AgentDialog::render(dialog_props(model), frame, area);
}

fn dialog_cursor(model: &Model, area: Rect) -> Option<ratatui::layout::Position> {
    AgentDialog::cursor(dialog_props(model), area)
}

fn help_overlay(model: &Model) -> Option<OverlayContribution> {
    model.agent.help_visible().then(|| {
        OverlayContribution::new(
            "agent_help",
            OverlayKind::Modal,
            OverlayPlacement::Centered(ratatui::layout::Size::new(54, 12)),
            render_help,
        )
    })
}

fn render_help(model: &Model, frame: &mut Frame, area: Rect) {
    AgentHelp::render(model.theme.ui(), frame, area);
}

pub(crate) fn render_panel(model: &Model, frame: &mut Frame, area: Rect) {
    let focus = if model.current_focus == FocusTarget::ToolsPanel {
        FocusState::Focused
    } else {
        FocusState::Unfocused
    };
    AgentPanel::render(
        AgentPanelProps {
            session: model.agent.session(),
            prompt: model.agent.prompt_state(),
            mode: model.agent.panel_mode(),
            storage_error: model.agent.storage_error(),
            focus,
            styles: model.theme.ui(),
        },
        frame,
        area,
    );
}
