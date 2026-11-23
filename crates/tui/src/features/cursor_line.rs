use ratatui::style::Style;

use crate::{
    data_object::{CommandId, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
    },
    theme::UiTheme,
};

/// Independent cursor-line policy; rendering remains a TextView capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorLineState {
    enabled: bool,
}

impl Default for CursorLineState {
    fn default() -> Self {
        Self { enabled: true }
    }
}

impl CursorLineState {
    pub fn enabled(self) -> bool {
        self.enabled
    }

    pub fn toggle(&mut self) {
        self.enabled = !self.enabled;
    }

    pub fn decoration(self, has_cursor: bool, styles: &UiTheme) -> Option<Style> {
        (self.enabled && has_cursor).then_some(styles.cursor_line)
    }
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("cursor_line_toggle"),
        "toggle cursor-line background",
        handle_command,
    ))
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if id.as_str() != "cursor_line_toggle" {
        return None;
    }
    model.cursor_line.toggle();
    model.status = Some(StatusMessage::info(if model.cursor_line.enabled() {
        "Cursor line enabled"
    } else {
        "Cursor line disabled"
    }));
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoration_requires_enablement_and_a_cursor() {
        let theme = crate::theme::Theme::default();
        let mut state = CursorLineState::default();
        assert!(state.decoration(false, theme.ui()).is_none());
        assert_eq!(
            state.decoration(true, theme.ui()),
            Some(theme.ui().cursor_line)
        );
        state.toggle();
        assert!(state.decoration(true, theme.ui()).is_none());
    }

    #[test]
    fn toggle_command_changes_only_feature_state_and_status() {
        let mut model = Model::default();
        handle_command(&mut model, CommandId("cursor_line_toggle"));
        assert!(!model.cursor_line.enabled());
        assert_eq!(
            model.status.as_ref().map(|status| status.text.as_str()),
            Some("Cursor line disabled")
        );
    }
}
