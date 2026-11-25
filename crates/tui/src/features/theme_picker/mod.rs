mod catalog;
mod component;
mod state;

pub use catalog::{ThemeProviderId, ThemeProviderRegistry, ThemeProviderSpec, ThemeSection};
pub use component::{ThemePicker, overlay};
pub use state::{ThemePickerEntry, ThemePickerState};

use crate::{
    config::ThemeConfig,
    data_object::{CommandId, Context, GeneralAction, ModalController, Model, StatusMessage},
    manager::{effect::Effect, general_action::GeneralActionSubscription},
    theme::{Theme, ThemePersistenceState},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemePickerCommand {
    Open,
    Close,
    Next,
    Previous,
    Apply,
}

impl ThemePickerCommand {
    const ALL: [Self; 5] = [
        Self::Open,
        Self::Close,
        Self::Next,
        Self::Previous,
        Self::Apply,
    ];

    pub(crate) const fn id(self) -> CommandId {
        match self {
            Self::Open => CommandId("open_theme_picker"),
            Self::Close => CommandId("close_theme_picker"),
            Self::Next => CommandId("theme_picker_next"),
            Self::Previous => CommandId("theme_picker_previous"),
            Self::Apply => CommandId("apply_theme"),
        }
    }

    fn from_id(id: CommandId) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }
}

/// The only application capabilities needed by theme-picker commands.
struct ThemePickerAdapter<'a> {
    state: &'a mut ThemePickerState,
    theme: &'a mut Theme,
    persistence: &'a mut ThemePersistenceState,
    modal: &'a mut ModalController,
    status: &'a mut Option<StatusMessage>,
}

impl ThemePickerAdapter<'_> {
    fn handle(&mut self, command: ThemePickerCommand) -> Option<Effect> {
        match command {
            ThemePickerCommand::Open => self.open(),
            ThemePickerCommand::Close => {
                self.close();
                None
            }
            ThemePickerCommand::Next => {
                self.state.select_next();
                None
            }
            ThemePickerCommand::Previous => {
                self.state.select_previous();
                None
            }
            ThemePickerCommand::Apply => self.apply(),
        }
    }

    fn open(&mut self) -> Option<Effect> {
        if !self.modal.open(Context::ThemePicker) {
            return None;
        }
        self.state.open(self.theme.builtin(), self.theme.name());
        crate::theme::start_discovery(self.state)
    }

    fn close(&mut self) {
        self.state.close();
        self.modal.close(Context::ThemePicker);
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

pub(crate) fn handle_input(
    model: &mut Model,
    event: crate::component::text_input::TextInputEvent,
) -> Option<Effect> {
    model.theme_picker.handle_query_event(event);
    None
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![GeneralActionSubscription::new(
        Context::ThemePicker,
        handle_general_action,
    )]
}

/// General modal actions operate directly on theme-picker state and workflows.
pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => {
            model.theme_picker.select_next();
            None
        }
        GeneralAction::Previous => {
            model.theme_picker.select_previous();
            None
        }
        GeneralAction::Activate => apply(model),
        GeneralAction::Back => {
            close_theme_picker(model);
            None
        }
    }
}

fn close_theme_picker(model: &mut Model) {
    ThemePickerAdapter {
        state: &mut model.theme_picker,
        theme: &mut model.theme,
        persistence: &mut model.theme_persistence,
        modal: &mut model.modal,
        status: &mut model.status,
    }
    .close();
}

fn apply(model: &mut Model) -> Option<Effect> {
    if !model.theme_picker.selection_is_visible() {
        return None;
    }
    if let Some(path) = model
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
            close_theme_picker(model);
        }
        return effect;
    }

    let Model {
        theme_picker,
        theme,
        theme_persistence,
        modal,
        status,
        ..
    } = model;
    ThemePickerAdapter {
        state: theme_picker,
        theme,
        persistence: theme_persistence,
        modal,
        status,
    }
    .handle(ThemePickerCommand::Apply)
}

/// Registered composition-root adapter for theme-picker commands.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    let Some(command) = ThemePickerCommand::from_id(id) else {
        model.status = Some(StatusMessage::error(format!(
            "unknown theme command `{}`",
            id.as_str()
        )));
        return None;
    };
    if command == ThemePickerCommand::Apply {
        return apply(model);
    }

    let Model {
        theme_picker,
        theme,
        theme_persistence,
        modal,
        status,
        ..
    } = model;
    ThemePickerAdapter {
        state: theme_picker,
        theme,
        persistence: theme_persistence,
        modal,
        status,
    }
    .handle(command)
}

#[cfg(test)]
mod tests {
    use crate::component::text_input::TextInputEvent;
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{data_object::Message, manager::message_manager::MessageManager, theme::ThemeName};

    #[test]
    fn command_ids_convert_to_typed_commands() {
        for command in ThemePickerCommand::ALL {
            assert_eq!(ThemePickerCommand::from_id(command.id()), Some(command));
        }
        assert_eq!(
            ThemePickerCommand::from_id(CommandId("unknown_theme_command")),
            None
        );
    }

    #[test]
    fn theme_menu_applies_selection_and_escape_preserves_the_active_theme() {
        let manager = MessageManager::default();
        let mut model = Model::default();

        manager.update(&mut model, Message::CommandMenuStart);
        assert!(
            manager
                .update(
                    &mut model,
                    Message::CommandMenuKey(KeyCode::Char('u').into())
                )
                .is_none()
        );
        assert!(
            manager
                .update(
                    &mut model,
                    Message::CommandMenuKey(KeyCode::Char('t').into())
                )
                .is_some()
        );
        assert!(model.theme_picker.is_open());
        assert_eq!(
            crate::test_utils::input_context(&model),
            Some(Context::ThemePicker)
        );
        assert!(model.context_stack.is_empty());
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::Default)
        );

        manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Next.id()),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(ThemePickerCommand::Apply.id())
                )
                .is_some()
        );
        assert_eq!(model.theme.builtin(), Some(ThemeName::TokyoNight));
        assert!(!model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), None);

        manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Open.id()),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Next.id()),
        );
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::CatppuccinMocha)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Close.id()),
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
        assert!(model.modal.open(Context::ThemePicker));

        for character in "xyz".chars() {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
        }
        assert_eq!(model.theme_picker.query(), "xyz");
        assert!(model.theme_picker.visible_entries().is_empty());
        assert!(
            manager
                .update(
                    &mut model,
                    Message::InvokeCommand(ThemePickerCommand::Apply.id())
                )
                .is_none()
        );

        for _ in 0..3 {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::DeleteBackward),
            );
        }
        for character in "tokyo".chars() {
            manager.update(
                &mut model,
                Message::TextInput(TextInputEvent::Insert(character)),
            );
        }
        assert_eq!(
            model.theme_picker.selected(),
            &state::ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Apply.id()),
        );
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

        let effect = manager.update(
            &mut model,
            Message::InvokeCommand(ThemePickerCommand::Apply.id()),
        );
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
