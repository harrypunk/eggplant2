use ratatui::{buffer::Buffer, layout::Rect, style::Style};

use crate::{
    data_object::{CommandId, Model, StatusMessage},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::Effect,
    },
    ui::{
        EditorDecorationContext, EditorDecorationContribution, EditorDecorationPhase, UiRegistry,
    },
};

/// Independent cursor-line policy; geometry remains a TextView capability.
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
}

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    commands.register(Command::feature(
        CommandId("cursor_line_toggle"),
        "toggle cursor-line background",
        handle_command,
    ))
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_editor_decoration("cursor_line", decoration)
}

fn decoration(model: &Model) -> Option<EditorDecorationContribution> {
    model.cursor_line.enabled().then(|| {
        EditorDecorationContribution::new(
            "cursor_line",
            EditorDecorationPhase::BeforeSelection,
            render,
        )
    })
}

fn render(context: EditorDecorationContext<'_>, area: Rect, buffer: &mut Buffer) {
    let Some(cursor) = context.cursor else {
        return;
    };
    let Some(background) = context.styles.cursor_line.bg else {
        return;
    };
    context
        .rows
        .iter()
        .filter(|row| row.logical_line == cursor.row)
        .for_each(|row| {
            if let Ok(screen_row) = u16::try_from(row.screen_row)
                && screen_row < area.height
            {
                buffer.set_style(
                    Rect::new(area.x, area.y + screen_row, area.width, 1),
                    Style::new().bg(background),
                );
            }
        });
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
    use editor::{CursorPosition, TextViewSnapshot};

    use super::*;
    use crate::{theme::Theme, ui::EditorVisualRow};

    #[test]
    fn registration_requires_enablement() {
        let mut model = Model::default();
        assert!(decoration(&model).is_some());
        model.cursor_line.toggle();
        assert!(decoration(&model).is_none());
    }

    #[test]
    fn render_paints_only_the_cursor_logical_line() {
        let theme = Theme::default();
        let snapshot = TextViewSnapshot::plain(vec!["a".to_string(), "b".to_string()]);
        let rows = [
            EditorVisualRow {
                logical_line: 0,
                screen_row: 0,
            },
            EditorVisualRow {
                logical_line: 0,
                screen_row: 1,
            },
            EditorVisualRow {
                logical_line: 1,
                screen_row: 2,
            },
        ];
        let area = Rect::new(0, 0, 2, 3);
        let mut buffer = Buffer::with_lines(["a ", "a ", "b "]);
        render(
            EditorDecorationContext {
                snapshot: &snapshot,
                cursor: Some(CursorPosition { row: 0, column: 0 }),
                rows: &rows,
                styles: theme.ui(),
            },
            area,
            &mut buffer,
        );

        let background = theme.ui().cursor_line.bg.unwrap();
        assert_eq!(buffer[(0, 0)].bg, background);
        assert_eq!(buffer[(0, 1)].bg, background);
        assert_ne!(buffer[(0, 2)].bg, background);
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
