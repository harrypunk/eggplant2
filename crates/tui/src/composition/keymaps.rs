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
        .extend(crate::editing::KEYMAPS.iter().copied())
        .extend(crate::line_jump::KEYMAPS.iter().copied())
        .extend(crate::view_align::KEYMAPS.iter().copied())
        .extend(crate::change::KEYMAPS.iter().copied())
        .extend(crate::yank::KEYMAPS.iter().copied())
        .extend(crate::buffer_jump::KEYMAPS.iter().copied())
        .extend(crate::buffer_navigation::KEYMAPS.iter().copied())
        .extend(crate::shell::KEYMAPS.iter().copied())
        .extend(crate::explorer::KEYMAPS.iter().copied())
        .extend(crate::vcs::KEYMAPS.iter().copied())
}

pub fn from_config(
    config: &KeymapConfig,
    commands: &CommandRegistry,
) -> color_eyre::Result<Registry> {
    Registry::from_config(config, commands, &catalog())
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::{data_object::Context, manager::keymap_contribution::ContextKeymap};

    #[test]
    fn catalog_covers_every_current_context_exactly_once() {
        let contexts = catalog()
            .iter()
            .map(|entry| entry.context)
            .collect::<Vec<_>>();
        let unique = contexts.iter().collect::<HashSet<_>>();

        assert_eq!(contexts.len(), 36);
        assert_eq!(unique.len(), 36);
    }

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
