mod catalog;
mod component;
mod state;

pub use catalog::{ThemeProviderId, ThemeProviderRegistry, ThemeProviderSpec, ThemeSection};
pub use component::{ThemePicker, overlay};
pub use state::{ThemePickerEntry, ThemePickerState};

use crate::{
    config::ThemeConfig,
    data_object::{CommandId, Context, ModalController, Model, StatusMessage},
    manager::{effect::Effect, text_input_registry::TextInputRegistry},
    theme::{Theme, ThemePersistenceState},
};

/// The only application capabilities needed by theme-picker commands.
struct ThemePickerAdapter<'a> {
    state: &'a mut ThemePickerState,
    theme: &'a mut Theme,
    persistence: &'a mut ThemePersistenceState,
    contexts: &'a mut Vec<Context>,
    modal: &'a mut ModalController,
    status: &'a mut Option<StatusMessage>,
}

impl ThemePickerAdapter<'_> {
    fn handle(&mut self, id: CommandId) -> Option<Effect> {
        match id.as_str() {
            "open_theme_picker" => self.open(),
            "close_theme_picker" => {
                self.close();
                None
            }
            "theme_picker_next" => {
                self.state.select_next();
                None
            }
            "theme_picker_previous" => {
                self.state.select_previous();
                None
            }
            "apply_theme" => self.apply(),
            "theme_picker_backspace" => {
                self.state.backspace_query();
                None
            }
            _ => {
                *self.status = Some(StatusMessage::error(format!(
                    "unknown theme command `{}`",
                    id.as_str()
                )));
                None
            }
        }
    }

    fn open(&mut self) -> Option<Effect> {
        if !self.modal.open(Context::ThemePicker) {
            return None;
        }
        self.state.open(self.theme.builtin(), self.theme.name());
        if self.contexts.last() != Some(&Context::ThemePicker) {
            self.contexts.push(Context::ThemePicker);
        }
        crate::theme::start_discovery(self.state)
    }

    fn close(&mut self) {
        self.state.close();
        self.modal.close(Context::ThemePicker);
        self.contexts
            .pop_if(|context| context == &Context::ThemePicker);
    }

    fn apply(&mut self) -> Option<Effect> {
        let selected = self.state.selected().builtin_name()?;
        match Theme::from_builtin(selected) {
            Ok(theme) => {
                *self.theme = theme;
                *self.status = None;
                self.close();
                crate::theme::persist_theme(
                    ThemeConfig::Name(selected.as_str().to_string()),
                    self.persistence,
                )
            }
            Err(error) => {
                *self.status = Some(StatusMessage::error(format!(
                    "failed to apply theme `{}`: {error}",
                    selected.as_str()
                )));
                None
            }
        }
    }
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::ThemePicker, insert_char)
}

fn insert_char(model: &mut Model, character: char) -> Option<Effect> {
    model.theme_picker.push_query(character);
    None
}

/// Registered composition-root adapter for theme-picker commands.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if id.as_str() == "apply_theme" && !model.theme_picker.selection_is_visible() {
        return None;
    }
    if id.as_str() == "apply_theme"
        && let Some(path) = model
            .theme_picker
            .selected()
            .file_path()
            .map(std::path::Path::to_path_buf)
    {
        let source = ThemeConfig::Ghostty {
            provider: model
                .theme_picker
                .selected()
                .provider()
                .as_str()
                .to_string(),
            path,
            fallback: model
                .theme
                .builtin()
                .unwrap_or_default()
                .as_str()
                .to_string(),
        };
        let effect = crate::theme::start_loading_and_persist(&source, model);
        if effect.is_some() {
            ThemePickerAdapter {
                state: &mut model.theme_picker,
                theme: &mut model.theme,
                persistence: &mut model.theme_persistence,
                contexts: &mut model.context_stack,
                modal: &mut model.modal,
                status: &mut model.status,
            }
            .close();
        }
        return effect;
    }

    let Model {
        theme_picker,
        theme,
        theme_persistence,
        context_stack,
        modal,
        status,
        ..
    } = model;
    ThemePickerAdapter {
        state: theme_picker,
        theme,
        persistence: theme_persistence,
        contexts: context_stack,
        modal,
        status,
    }
    .handle(id)
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{data_object::Message, manager::message_manager::MessageManager, theme::ThemeName};

    #[test]
    fn theme_menu_applies_selection_and_escape_preserves_the_active_theme() {
        let manager = MessageManager::default();
        let mut model = Model::default();

        for key in [' ', 'u'] {
            assert!(
                manager
                    .update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)))
                    .is_none()
            );
        }
        assert!(
            manager
                .update(&mut model, Message::CommandMenuKey(KeyCode::Char('t')))
                .is_some()
        );
        assert!(model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), Some(&Context::ThemePicker));
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::Default)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        assert!(
            manager
                .update(&mut model, Message::InvokeCommand(CommandId("apply_theme")))
                .is_some()
        );
        assert_eq!(model.theme.builtin(), Some(ThemeName::TokyoNight));
        assert!(!model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), None);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("open_theme_picker")),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::CatppuccinMocha)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_theme_picker")),
        );
        assert_eq!(model.theme.builtin(), Some(ThemeName::TokyoNight));
        assert!(!model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), None);
    }

    #[test]
    fn query_input_filters_results_and_rejects_no_match_apply() {
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.theme_picker.open(Some(ThemeName::Default), "default");
        model.context_stack.push(Context::ThemePicker);

        for character in "xyz".chars() {
            manager.update(&mut model, Message::TextInput(character));
        }
        assert_eq!(model.theme_picker.query(), "xyz");
        assert!(model.theme_picker.visible_entries().is_empty());
        assert!(
            manager
                .update(&mut model, Message::InvokeCommand(CommandId("apply_theme")))
                .is_none()
        );

        for _ in 0..3 {
            manager.update(
                &mut model,
                Message::InvokeCommand(CommandId("theme_picker_backspace")),
            );
        }
        for character in "tokyo".chars() {
            manager.update(&mut model, Message::TextInput(character));
        }
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        manager.update(&mut model, Message::InvokeCommand(CommandId("apply_theme")));
        assert_eq!(model.theme.builtin(), Some(ThemeName::TokyoNight));
    }

    #[test]
    fn ghostty_selection_loads_through_the_provider_effect() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("local-theme");
        std::fs::write(&path, "background = #101010\nforeground = #f0f0f0\n").unwrap();
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.theme_picker.open(Some(ThemeName::Default), "default");
        let revision = model
            .theme_picker
            .begin_discovery(ThemeProviderId::GHOSTTY)
            .unwrap();
        assert!(model.theme_picker.complete_discovery(
            revision,
            vec![state::ThemePickerEntry::ghostty(
                "local-theme",
                path.clone()
            )],
        ));
        for _ in 0..3 {
            model.theme_picker.select_next();
        }

        let effect = manager.update(&mut model, Message::InvokeCommand(CommandId("apply_theme")));
        assert!(!model.theme_picker.is_open());
        let Some(Effect::Feature(request)) = effect else {
            panic!("expected Ghostty load effect");
        };
        let Message::FeatureCompletion(completion) = request.perform() else {
            panic!("expected Ghostty load completion");
        };
        completion.update(&mut model);

        assert_eq!(model.theme.name(), "local-theme");
        assert_eq!(model.theme.builtin(), None);
    }
}
