use ratatui::{Frame, layout::Rect};

use crate::{
    component::agent::ModelSelector,
    data_object::{Context, Model},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("model_selector", overlay)
}

fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.modal.is_active(Context::ModelSelector).then(|| {
        OverlayContribution::new(
            "model_selector",
            OverlayKind::Modal,
            OverlayPlacement::Centered(ModelSelector::size(&model.model_selector)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    ModelSelector::render(&model.model_selector, model.theme.ui(), frame, area);
}
