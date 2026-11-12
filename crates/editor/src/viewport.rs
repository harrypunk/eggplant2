use helix_view::{editor::GutterConfig, graphics::Rect};

/// Eggplant renders gutters and the statusline outside Helix's text viewport.
pub(crate) fn gutters() -> GutterConfig {
    GutterConfig {
        layout: Vec::new(),
        ..GutterConfig::default()
    }
}

/// Compensate for Helix View unconditionally reserving one statusline row.
pub(crate) fn area(width: u16, height: u16) -> Rect {
    Rect::new(0, 0, width, height.saturating_add(1))
}
