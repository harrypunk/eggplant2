use ratatui::{
    Frame,
    layout::{Rect, Size},
};

use crate::{
    component::agent::ProviderLogin,
    data_object::Model,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("provider_login", overlay)
}

fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.provider_login.is_open().then(|| {
        OverlayContribution::new(
            "provider_login",
            OverlayKind::Modal,
            OverlayPlacement::Centered(Size::new(58, 8)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    ProviderLogin::render(model.theme.ui(), frame, area);
}
