use crate::{
    config::KeymapConfig,
    manager::{
        command_registry::CommandRegistry, keyboard_input_registry::Registry,
        keymap_contribution::KeymapCatalog,
    },
};

/// Assemble static feature declarations without giving the runtime registry feature dependencies.
pub fn catalog() -> KeymapCatalog {
    KeymapCatalog::default()
        .extend(crate::provider_login::KEYMAPS.iter().copied())
        .extend(crate::model_selector::KEYMAPS.iter().copied())
        .extend(crate::agent::KEYMAPS.iter().copied())
        .extend(crate::welcome::KEYMAPS.iter().copied())
        .extend(crate::file_picker::KEYMAPS.iter().copied())
        .extend(crate::theme_picker::KEYMAPS.iter().copied())
        .extend(crate::buffer_grep::KEYMAPS.iter().copied())
        .extend(crate::project_grep::KEYMAPS.iter().copied())
        .extend(crate::code_navigation::KEYMAPS.iter().copied())
}

pub fn from_config(
    config: &KeymapConfig,
    commands: &CommandRegistry,
) -> color_eyre::Result<Registry> {
    Registry::from_config(config, commands, &catalog())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data_object::Context, manager::keymap_contribution::ContextKeymap};

    #[test]
    fn duplicate_feature_contexts_are_rejected() {
        let duplicate = ContextKeymap::new("second", Context::ModelSelector);
        let catalog = catalog().extend([duplicate]);

        let error =
            Registry::from_config(&KeymapConfig::new(), &CommandRegistry::default(), &catalog)
                .unwrap_err();

        assert!(error.to_string().contains("duplicate keymap declaration"));
        assert!(error.to_string().contains("ModelSelector"));
    }
}
