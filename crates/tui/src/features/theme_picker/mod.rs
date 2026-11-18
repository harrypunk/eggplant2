mod component;
mod state;

pub use component::{ThemePicker, overlay};
pub use state::ThemePickerState;

use crate::{
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
    fn handle(&mut self, id: CommandId) {
        match id.as_str() {
            "open_theme_picker" => self.open(),
            "close_theme_picker" => self.close(),
            "theme_picker_next" => self.state.select_next(),
            "theme_picker_previous" => self.state.select_previous(),
            "apply_theme" => self.apply(),
            _ => {
                *self.status = Some(StatusMessage::error(format!(
                    "unknown theme command `{}`",
                    id.as_str()
                )));
            }
        }
    }

    fn open(&mut self) {
        if !self.modal.open(Context::ThemePicker) {
            return;
        }
        self.state.open(self.theme.builtin().unwrap_or_default());
        if self.contexts.last() != Some(&Context::ThemePicker) {
            self.contexts.push(Context::ThemePicker);
        }
    }

    fn close(&mut self) {
        self.state.close();
        self.modal.close(Context::ThemePicker);
        self.contexts
            .pop_if(|context| context == &Context::ThemePicker);
    }

    fn apply(&mut self) {
        let selected = self.state.selected();
        match Theme::from_builtin(selected) {
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
    }
}

/// Registered composition-root adapter for theme-picker commands.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
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
    .handle(id);
    None
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

        for key in [' ', 'u', 't'] {
            assert!(
                manager
                    .update(&mut model, Message::CommandMenuKey(KeyCode::Char(key)))
                    .is_none()
            );
        }
        assert!(model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), Some(&Context::ThemePicker));
        assert_eq!(model.theme_picker.selected(), ThemeName::Default);

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(model.theme_picker.selected(), ThemeName::TokyoNight);
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
        assert_eq!(model.theme_picker.selected(), ThemeName::TokyoNight);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("theme_picker_next")),
        );
        assert_eq!(model.theme_picker.selected(), ThemeName::CatppuccinMocha);
        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("close_theme_picker")),
        );
        assert_eq!(model.theme.builtin(), Some(ThemeName::TokyoNight));
        assert!(!model.theme_picker.is_open());
        assert_eq!(model.context_stack.last(), None);
    }
}
