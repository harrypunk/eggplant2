use ratatui::{Frame, layout::Rect};

use crate::{
    data_object::{Context, Model},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement, UiRegistry},
};

use super::component::ProviderLogout;

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("provider_logout", overlay)
}

fn overlay(model: &Model) -> Option<OverlayContribution> {
    model.modal.is_active(Context::ProviderLogout).then(|| {
        OverlayContribution::new(
            "provider_logout",
            OverlayKind::Modal,
            OverlayPlacement::Centered(ProviderLogout::size(&model.provider_logout)),
            render,
        )
    })
}

fn render(model: &Model, frame: &mut Frame, area: Rect) {
    ProviderLogout::render(&model.provider_logout, model.theme.ui(), frame, area);
}
