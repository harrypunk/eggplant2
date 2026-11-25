use ratatui::{
    Frame,
    layout::{Margin, Rect},
};

use crate::{
    component::agent::{
        AgentDialog, AgentHelp, AgentPanel, AgentPanelProps, AgentPromptProps, AgentViewProps,
    },
    data_object::{Context, FocusState, FocusTarget, Model},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("agent", dialog_overlay)?;
    ui.register_overlay("agent_help", help_overlay)
}

fn dialog_overlay(model: &Model) -> Option<OverlayContribution> {
    (model.modal.is_active(Context::AgentChatDialog)
        || model.modal.is_active(Context::AgentApproval))
    .then(|| {
        OverlayContribution::new(
            "agent",
            OverlayKind::Modal,
            OverlayPlacement::MainPanelFill(Margin::new(4, 2)),
            render_dialog,
        )
        .with_cursor(dialog_cursor)
    })
}

fn view_props(model: &Model) -> AgentViewProps<'_> {
    AgentViewProps {
        session: model.agent.session(),
        prompt: AgentPromptProps::new(model.agent.prompt_state()).with_commands(
            prompt_commands(model),
            model.agent.command_state().selected(),
        ),
        storage_error: model.agent.storage_error(),
        styles: model.theme.ui(),
    }
}

fn render_dialog(model: &Model, frame: &mut Frame, area: Rect) {
    AgentDialog::render(&view_props(model), frame, area);
}

fn dialog_cursor(model: &Model, area: Rect) -> Option<ratatui::layout::Position> {
    AgentDialog::cursor(&view_props(model), area)
}

fn help_overlay(model: &Model) -> Option<OverlayContribution> {
    model.modal.is_active(Context::AgentChatHelp).then(|| {
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

fn prompt_commands(model: &Model) -> Option<Vec<super::PromptCommand>> {
    match super::surface::active(model) {
        Some(super::surface::AgentSurface::Dialog) => {
            model.agent.command_state().commands(model.agent.prompt())
        }
        Some(super::surface::AgentSurface::Panel)
            if super::panel::insert_input_available(model) =>
        {
            model.agent.command_state().commands(model.agent.prompt())
        }
        _ => None,
    }
}

pub(crate) fn render_panel(model: &Model, frame: &mut Frame, area: Rect) {
    let focus = if model.current_focus == FocusTarget::ToolsPanel {
        FocusState::Focused
    } else {
        FocusState::Unfocused
    };
    AgentPanel::render(
        &AgentPanelProps {
            shared: view_props(model),
            mode: model.agent.panel_mode(),
            focus,
        },
        frame,
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data_object::Visibility, tools::ToolKind};

    #[test]
    fn overlays_follow_modal_ownership() {
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
        assert!(dialog_overlay(&model).is_none());

        assert!(model.modal.open(Context::AgentChatDialog));
        assert!(dialog_overlay(&model).is_some());
        model.modal.close(Context::AgentChatDialog);
        assert!(dialog_overlay(&model).is_none());

        model.agent.toggle_help();
        assert!(help_overlay(&model).is_none());
        assert!(model.modal.open(Context::AgentChatHelp));
        assert!(help_overlay(&model).is_some());
        model.modal.close(Context::AgentChatHelp);
        assert!(help_overlay(&model).is_none());
    }

    #[test]
    fn dialog_fills_the_available_area_inside_its_margins() {
        let mut model = Model::default();
        assert!(model.modal.open(Context::AgentChatDialog));
        let overlay = dialog_overlay(&model).unwrap();
        let main = Rect::new(10, 20, 120, 40);

        assert_eq!(
            overlay.area(main, main),
            Rect::new(14, 22, 112, 36),
            "dialog should grow with the main panel rather than use a fixed size"
        );
    }

    #[test]
    fn commands_are_shared_by_panel_and_dialog() {
        let mut model = Model {
            current_focus: FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        model.agent.enter_insert_mode();
        model.agent.prompt.set_value("/p");
        assert_eq!(
            prompt_commands(&model),
            Some(vec![crate::agent::PromptCommand::ProviderLogin])
        );

        model.agent.open_dialog(None, String::new());
        assert!(model.modal.open(Context::AgentChatDialog));
        model.agent.prompt.set_value("/p");
        assert_eq!(
            prompt_commands(&model),
            Some(vec![crate::agent::PromptCommand::ProviderLogin])
        );

        model.agent.prompt.set_value("plain prompt");
        assert!(prompt_commands(&model).is_none());
    }
}
