mod component;
mod state;

pub use component::{ThemePicker, overlay};
pub use state::ThemePickerState;

use crate::{
    data_object::{CommandId, Context, Model, StatusMessage},
    manager::effect::Effect,
    theme::Theme,
};

/// Feature-owned theme command handling. The Model adapter keeps this slice small;
/// a later shell migration can pass narrower state/capabilities without changing IDs.
pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "open_theme_picker" => open(model),
        "close_theme_picker" => close(model),
        "theme_picker_next" => {
            model.theme_picker.select_next();
            None
        }
        "theme_picker_previous" => {
            model.theme_picker.select_previous();
            None
        }
        "apply_theme" => apply(model),
        _ => {
            model.status = Some(StatusMessage::error(format!(
                "unknown theme command `{}`",
                id.as_str()
            )));
            None
        }
    }
}

fn open(model: &mut Model) -> Option<Effect> {
    model.theme_picker.open(model.theme.name());
    if model.context_stack.last() != Some(&Context::ThemePicker) {
        model.context_stack.push(Context::ThemePicker);
    }
    None
}

fn close(model: &mut Model) -> Option<Effect> {
    model.theme_picker.close();
    model
        .context_stack
        .pop_if(|context| context == &Context::ThemePicker);
    None
}

fn apply(model: &mut Model) -> Option<Effect> {
    let selected = model.theme_picker.selected();
    match Theme::from_builtin(selected) {
        Ok(theme) => {
            model.theme = theme;
            model.status = None;
            close(model);
        }
        Err(error) => {
            model.status = Some(StatusMessage::error(format!(
                "failed to apply theme `{}`: {error}",
                selected.as_str()
            )));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::{
        data_object::{Message, Visibility},
        manager::message_manager::MessageManager,
        theme::ThemeName,
    };

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
        assert_eq!(model.theme_picker.visibility, Visibility::Displayed);
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
        assert_eq!(model.theme.name(), ThemeName::TokyoNight);
        assert_eq!(model.theme_picker.visibility, Visibility::Hidden);
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
        assert_eq!(model.theme.name(), ThemeName::TokyoNight);
        assert_eq!(model.theme_picker.visibility, Visibility::Hidden);
        assert_eq!(model.context_stack.last(), None);
    }
}
