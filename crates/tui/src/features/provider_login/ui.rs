use ratatui::{Frame, layout::Rect};

use crate::{
    component::agent::ProviderLogin,
    data_object::Model,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("provider_login", overlay)
}

fn overlay(model: &Model) -> Option<OverlayContribution> {
    model
        .modal
        .is_active(crate::data_object::Context::ProviderLogin)
        .then(|| {
            OverlayContribution::new(
                "provider_login",
                OverlayKind::Modal,
                OverlayPlacement::Centered(ProviderLogin::size(&model.provider_login)),
                render,
            )
            .with_cursor(cursor)
        })
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    ProviderLogin::render(&model.provider_login, model.theme.ui(), frame, area);
}

fn cursor(model: &Model, area: Rect) -> Option<ratatui::layout::Position> {
    ProviderLogin::cursor(&model.provider_login, model.theme.ui(), area)
}
