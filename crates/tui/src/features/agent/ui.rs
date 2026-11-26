use ratatui::{
    Frame,
    layout::{Margin, Rect},
};

use crate::{
    component::agent::{
        AgentDialog, AgentFooterProps, AgentHelp, AgentModelNotice, AgentPanel, AgentPanelProps,
        AgentPromptProps, AgentViewProps,
    },
    data_object::{Context, FocusState, FocusTarget, Model},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

use super::surface::AgentSurface;

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

fn view_props(model: &Model, surface: AgentSurface) -> AgentViewProps<'_> {
    let commands = prompt_commands(model, surface);
    let selected_command = commands
        .as_ref()
        .and_then(|_| model.agent.command_state().selected());
    AgentViewProps {
        session: model.agent.session(),
        prompt: AgentPromptProps::new(model.agent.prompt_state())
            .with_commands(commands, selected_command),
        storage_error: model.agent.storage_error(),
        model_notice: model
            .agent
            .provider()
            .is_none()
            .then_some(AgentModelNotice::Unavailable),
        footer: AgentFooterProps::new(
            model.agent.provider_name(),
            model.agent.model(),
            model
                .agent
                .provider()
                .and_then(|provider| provider.context_window()),
        ),
        styles: model.theme.ui(),
    }
}

fn render_dialog(model: &Model, frame: &mut Frame, area: Rect) {
    AgentDialog::render(&view_props(model, AgentSurface::Dialog), frame, area);
}

fn dialog_cursor(model: &Model, area: Rect) -> Option<ratatui::layout::Position> {
    AgentDialog::cursor(&view_props(model, AgentSurface::Dialog), area)
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

fn prompt_commands(model: &Model, surface: AgentSurface) -> Option<Vec<super::PromptCommand>> {
    super::surface::prompt_active(model)
        .filter(|active| *active == surface)
        .and_then(|_| model.agent.command_state().commands(model.agent.prompt()))
}

pub(crate) fn render_panel(model: &Model, frame: &mut Frame, area: Rect) {
    let focus = if model.current_focus == FocusTarget::ToolsPanel {
        FocusState::Focused
    } else {
        FocusState::Unfocused
    };
    AgentPanel::render(
        &AgentPanelProps {
            shared: view_props(model, AgentSurface::Panel),
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

    fn view_commands(
        model: &Model,
        surface: AgentSurface,
    ) -> Option<Vec<crate::agent::PromptCommand>> {
        view_props(model, surface)
            .prompt
            .commands()
            .map(|commands| commands.to_vec())
    }

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
    fn missing_provider_maps_the_shared_notice_to_both_surfaces() {
        let model = Model::default();

        for surface in [AgentSurface::Dialog, AgentSurface::Panel] {
            let props = view_props(&model, surface);
            assert_eq!(props.model_notice, Some(AgentModelNotice::Unavailable));
            assert!(props.model_notice.unwrap().text().contains("/provider"));
        }
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
    fn command_menu_is_projected_only_to_the_prompt_active_surface() {
        let mut model = Model {
            current_focus: FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        model.agent.enter_insert_mode();
        model.agent.prompt.set_value("/p");
        let provider = Some(vec![crate::agent::PromptCommand::ProviderLogin]);
        assert_eq!(view_commands(&model, AgentSurface::Panel), provider);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);

        model.agent.open_dialog(None, String::new());
        assert!(model.modal.open(Context::AgentChatDialog));
        assert_eq!(view_commands(&model, AgentSurface::Dialog), provider);
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);

        model.modal.close(Context::AgentChatDialog);
        assert_eq!(view_commands(&model, AgentSurface::Panel), provider);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);

        model.agent.enter_normal_mode();
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);
        model.agent.enter_insert_mode();
        model.agent.prompt.set_value("plain prompt");
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);
    }
}
