use ratatui::{Frame, layout::Rect};

use crate::{
    component::agent::{
        AgentDialog, AgentFooterProps, AgentHelp, AgentModelNotice, AgentPanel, AgentPanelProps,
        AgentPromptProps, AgentReferenceProps, AgentViewProps, ViewportMetrics,
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
            OverlayPlacement::EditorPanel {
                max_width: 88,
                max_height: 26,
                margin: 2,
            },
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
    let references = prompt_references(model, surface);
    AgentViewProps {
        session: model.agent.session(),
        prompt: AgentPromptProps::new(model.agent.prompt_state())
            .with_commands(commands, selected_command)
            .with_references(references),
        navigation: model.agent.navigation(surface),
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
        .filter(|_| !model.agent.file_references_active())
        .and_then(|_| model.agent.command_state().commands(model.agent.prompt()))
}

fn prompt_references(model: &Model, surface: AgentSurface) -> Option<AgentReferenceProps> {
    super::surface::prompt_active(model)
        .filter(|active| *active == surface)
        .and_then(|_| model.agent.file_references())
        .map(|references| {
            AgentReferenceProps::new(
                references
                    .entries
                    .iter()
                    .map(|entry| entry.display.clone())
                    .collect(),
                references.selected,
                references.status,
            )
        })
}

fn panel_props(model: &Model) -> AgentPanelProps<'_> {
    let focus = if model.current_focus == FocusTarget::ToolsPanel {
        FocusState::Focused
    } else {
        FocusState::Unfocused
    };
    AgentPanelProps {
        shared: view_props(model, AgentSurface::Panel),
        mode: model.agent.panel_mode(),
        focus,
    }
}

pub(crate) fn render_panel(model: &Model, frame: &mut Frame, area: Rect) {
    AgentPanel::render(&panel_props(model), frame, area);
}

pub(super) fn navigation_metrics(model: &Model, surface: AgentSurface) -> ViewportMetrics {
    let regions = crate::page::editor::layout_regions(model);
    match surface {
        AgentSurface::Dialog => dialog_overlay(model)
            .map(|overlay| {
                let area =
                    overlay.area_with_editor(regions.main, regions.editor, model.window_area);
                AgentDialog::transcript_metrics(&view_props(model, AgentSurface::Dialog), area)
            })
            .unwrap_or_default(),
        AgentSurface::Panel => regions
            .tools
            .map(|area| AgentPanel::transcript_metrics(&panel_props(model), area))
            .unwrap_or_default(),
    }
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
        model
            .agent
            .dispatch(crate::agent::AgentUiAction::OpenDialog {
                context: Some(crate::agent::EditorContext {
                    buffer: editor::BufferId::from_raw(1),
                    path: std::path::PathBuf::from("/workspace/main.rs"),
                    revision: 1,
                    line: 1,
                }),
            });
        assert!(dialog_overlay(&model).is_none());

        assert!(model.modal.open(Context::AgentChatDialog));
        assert!(dialog_overlay(&model).is_some());
        model.modal.close(Context::AgentChatDialog);
        assert!(dialog_overlay(&model).is_none());

        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::ToggleHelp,
        ));
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
    fn dialog_is_bounded_and_centered_within_the_editor_region() {
        let mut model = Model::default();
        assert!(model.modal.open(Context::AgentChatDialog));
        let overlay = dialog_overlay(&model).unwrap();
        let frame = Rect::new(0, 0, 140, 40);
        let editor = Rect::new(28, 0, 92, 40);

        assert_eq!(
            overlay.area_with_editor(frame, editor, frame),
            Rect::new(30, 7, 88, 26),
            "dialog should preserve the explorer, tools, and editor perimeter"
        );

        let small_editor = Rect::new(28, 0, 60, 18);
        assert_eq!(
            overlay.area_with_editor(frame, small_editor, frame),
            Rect::new(29, 1, 58, 16),
            "small terminals should clamp inside the editor"
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
        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::EnterInsert,
        ));
        model.agent.set_prompt("/p");
        let provider = Some(vec![crate::agent::PromptCommand::ProviderLogin]);
        assert_eq!(view_commands(&model, AgentSurface::Panel), provider);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);

        model
            .agent
            .dispatch(crate::agent::AgentUiAction::OpenDialog { context: None });
        assert!(model.modal.open(Context::AgentChatDialog));
        assert_eq!(view_commands(&model, AgentSurface::Dialog), provider);
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);

        model.modal.close(Context::AgentChatDialog);
        assert_eq!(view_commands(&model, AgentSurface::Panel), provider);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);

        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::EnterNormal,
        ));
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);
        model.agent.dispatch(crate::agent::AgentUiAction::Panel(
            crate::agent::PanelAction::EnterInsert,
        ));
        model.agent.set_prompt("plain prompt");
        assert_eq!(view_commands(&model, AgentSurface::Panel), None);
        assert_eq!(view_commands(&model, AgentSurface::Dialog), None);
    }
}
