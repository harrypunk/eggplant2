use crate::{
    capabilities::page_scroll::ScrollDirection, component::agent::ViewportMetrics,
    data_object::Model, manager::effect::Effect,
};

use super::{AgentInputMode, surface::AgentSurface};

pub(super) fn page_up(model: &mut Model) -> Option<Effect> {
    page(model, ScrollDirection::Up)
}

pub(super) fn page_down(model: &mut Model) -> Option<Effect> {
    page(model, ScrollDirection::Down)
}

pub(super) fn cursor_up(model: &mut Model) -> Option<Effect> {
    cursor(model, ScrollDirection::Up)
}

pub(super) fn cursor_down(model: &mut Model) -> Option<Effect> {
    cursor(model, ScrollDirection::Down)
}

fn page(model: &mut Model, direction: ScrollDirection) -> Option<Effect> {
    let page = model.page_scroll;
    let (surface, metrics) = metrics_for_active_surface(model)?;
    match surface {
        AgentSurface::Dialog => {
            model
                .agent
                .dialog_scroll_mut()
                .scroll(page, direction, metrics);
        }
        AgentSurface::Panel => match model.agent.panel_mode() {
            AgentInputMode::Normal => {
                model
                    .agent
                    .panel_navigation_mut()
                    .page(page, direction, metrics);
            }
            AgentInputMode::Insert => {
                model
                    .agent
                    .panel_navigation_mut()
                    .scroll(page, direction, metrics);
            }
        },
    }
    None
}

fn cursor(model: &mut Model, direction: ScrollDirection) -> Option<Effect> {
    if model.agent.panel_mode() != AgentInputMode::Normal {
        return None;
    }
    let Some((AgentSurface::Panel, metrics)) = metrics_for_active_surface(model) else {
        return None;
    };
    model
        .agent
        .panel_navigation_mut()
        .move_cursor(direction, metrics);
    None
}

fn metrics_for_active_surface(model: &Model) -> Option<(AgentSurface, ViewportMetrics)> {
    let surface = super::surface::active(model)?;
    Some((surface, super::ui::navigation_metrics(model, surface)))
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    use super::*;
    use crate::{
        agent::{dialog, panel, ui},
        data_object::Visibility,
        tools::ToolKind,
    };

    fn model() -> (Model, tempfile::TempDir) {
        let directory = tempfile::tempdir().unwrap();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.window_area = Rect::new(0, 0, 140, 40);
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        let run = model
            .agent
            .session_mut()
            .begin("question".to_string())
            .unwrap();
        model.agent.session_mut().append_delta(
            run,
            &"A sufficiently long Markdown response.\n\n".repeat(30),
        );
        model.agent.session_mut().complete(run);
        (model, directory)
    }

    #[test]
    fn dialog_pages_without_changing_the_editable_prompt() {
        let (mut model, _directory) = model();
        dialog::open(&mut model);
        crate::agent::handle_input(
            &mut model,
            crate::component::text_input::TextInputEvent::Insert('x'),
        );
        let metrics = ui::navigation_metrics(&model, AgentSurface::Dialog);
        assert!(metrics.content_rows > metrics.viewport_rows);

        page_up(&mut model);
        let offset = model
            .agent
            .navigation(AgentSurface::Dialog)
            .dialog()
            .offset(metrics);
        assert!(offset < metrics.max_offset());
        assert_eq!(model.agent.prompt(), "x");

        page_down(&mut model);
        assert_eq!(
            model
                .agent
                .navigation(AgentSurface::Dialog)
                .dialog()
                .offset(metrics),
            metrics.max_offset()
        );
        assert_eq!(model.agent.prompt(), "x");
    }

    #[test]
    fn panel_cursor_pages_and_resets_to_bottom_when_normal_mode_restarts() {
        let (mut model, _directory) = model();
        panel::focus_panel_input(&mut model);
        let metrics = ui::navigation_metrics(&model, AgentSurface::Panel);
        let last = metrics.content_rows - 1;
        let initial = model.agent.navigation(AgentSurface::Panel).panel();
        assert_eq!(initial.cursor_row(metrics), Some(last));
        assert_eq!(initial.offset(metrics), metrics.max_offset());

        cursor_up(&mut model);
        assert_eq!(
            model
                .agent
                .navigation(AgentSurface::Panel)
                .panel()
                .cursor_row(metrics),
            Some(last - 1)
        );
        page_up(&mut model);
        let moved = model.agent.navigation(AgentSurface::Panel).panel();
        assert!(moved.cursor_row(metrics).unwrap() < last - 1);
        assert!(moved.offset(metrics) < metrics.max_offset());

        model.agent.enter_insert_mode();
        cursor_down(&mut model);
        assert_eq!(model.agent.navigation(AgentSurface::Panel).panel(), moved);
        model.agent.enter_normal_mode();
        let reset = model.agent.navigation(AgentSurface::Panel).panel();
        assert_eq!(reset.cursor_row(metrics), Some(last));
        assert_eq!(reset.offset(metrics), metrics.max_offset());
    }

    #[test]
    fn dialog_submission_restores_bottom_after_manual_scrolling() {
        let (mut model, _directory) = model();
        dialog::open(&mut model);
        page_up(&mut model);
        model.agent.set_prompt("next question");

        model.agent.begin_prompt().unwrap();

        let metrics = ui::navigation_metrics(&model, AgentSurface::Dialog);
        assert_eq!(
            model
                .agent
                .navigation(AgentSurface::Dialog)
                .dialog()
                .offset(metrics),
            metrics.max_offset()
        );
    }

    #[test]
    fn panel_submission_restores_bottom_without_leaving_insert_mode() {
        let (mut model, _directory) = model();
        panel::focus_panel_input(&mut model);
        model.agent.enter_insert_mode();
        page_up(&mut model);
        model.agent.set_prompt("next question");

        model.agent.begin_prompt().unwrap();

        let metrics = ui::navigation_metrics(&model, AgentSurface::Panel);
        let navigation = model.agent.navigation(AgentSurface::Panel).panel();
        assert_eq!(navigation.offset(metrics), metrics.max_offset());
        assert_eq!(
            navigation.cursor_row(metrics),
            Some(metrics.content_rows - 1)
        );
        assert_eq!(model.agent.panel_mode(), AgentInputMode::Insert);
    }
}
