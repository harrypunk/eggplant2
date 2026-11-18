mod component;
mod state;

pub use component::{ThemePicker, overlay};
pub use state::{ThemePickerEntry, ThemePickerState};

use crate::{
    config::ThemeConfig,
    data_object::{CommandId, Context, ModalController, Model, StatusMessage},
    manager::effect::Effect,
    theme::Theme,
};

/// The only application capabilities needed by theme-picker commands.
struct ThemePickerAdapter<'a> {
    state: &'a mut ThemePickerState,
    theme: &'a mut Theme,
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
        let state::ThemePickerEntry::Builtin(selected) = self.state.selected() else {
            return None;
        };
        match Theme::from_builtin(*selected) {
            Ok(theme) => {
                *self.theme = theme;
                *self.status = None;
                self.close();
            }
            Err(error) => {
                *self.status = Some(StatusMessage::error(format!(
                    "failed to apply theme `{}`: {error}",
                    selected.as_str()
                )));
            }
        }
        None
    }
}

/// Registered composition-root adapter for theme-picker commands.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if id.as_str() == "apply_theme"
        && let state::ThemePickerEntry::Ghostty { path, .. } = model.theme_picker.selected().clone()
    {
        let source = ThemeConfig::Ghostty {
            provider: "ghostty".to_string(),
            path,
            fallback: model.theme.name().to_string(),
        };
        let effect = crate::theme::start_loading(&source, model);
        if effect.is_some() {
            ThemePickerAdapter {
                state: &mut model.theme_picker,
                theme: &mut model.theme,
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
        context_stack,
        modal,
        status,
        ..
    } = model;
    ThemePickerAdapter {
        state: theme_picker,
        theme,
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
            &state::ThemePickerEntry::Builtin(ThemeName::Default)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::Builtin(ThemeName::TokyoNight)
        );
        assert!(
            manager
                .update(&mut model, Message::InvokeCommand(CommandId("apply_theme")))
                .is_none()
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
            &state::ThemePickerEntry::Builtin(ThemeName::TokyoNight)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::Builtin(ThemeName::CatppuccinMocha)
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
    fn ghostty_selection_loads_through_the_provider_effect() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("local-theme");
        std::fs::write(&path, "background = #101010\nforeground = #f0f0f0\n").unwrap();
        let manager = MessageManager::default();
        let mut model = Model::default();
        model.theme_picker.open(Some(ThemeName::Default), "default");
        let revision = model.theme_picker.begin_discovery().unwrap();
        assert!(model.theme_picker.complete_discovery(
            revision,
            vec![state::ThemePickerEntry::Ghostty {
                name: "local-theme".to_string(),
                path: path.clone(),
            }],
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
