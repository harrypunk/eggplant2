use crate::{config::ThemeConfig, data_object::Model, manager::effect::Effect};

/// Compose initial feature work without embedding optional addons in the terminal runtime.
pub fn initial_effects(model: &mut Model, source: &ThemeConfig) -> Option<Effect> {
    Effect::combine(
        crate::code_navigation::synchronize(model),
        Effect::combine(
            crate::theme::start_loading(source, model),
            Effect::combine(
                crate::vcs::refresh(model),
                crate::vcs::sync_active_buffer(model),
            ),
        ),
    )
}
