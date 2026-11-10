use editor::EditMode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{
    data_object::{StatusKind, StatusMessage},
    theme::UiTheme,
};

#[derive(Debug, Clone, Copy)]
pub struct StatusBar<'a> {
    mode: EditMode,
    message: Option<&'a StatusMessage>,
    styles: &'a UiTheme,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: EditMode, message: Option<&'a StatusMessage>, styles: &'a UiTheme) -> Self {
        Self {
            mode,
            message,
            styles,
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (mode, mode_style) = match self.mode {
            EditMode::Normal => ("NORMAL", self.styles.status_normal),
            EditMode::Insert => ("INSERT", self.styles.status_insert),
        };
        let mut spans = vec![Span::styled(
            format!(" {mode}"),
            self.styles.base.patch(mode_style),
        )];
        if let Some(message) = self.message {
            let style = match message.kind {
                StatusKind::Error => self.styles.base.patch(self.styles.error),
                StatusKind::Info => self.styles.base,
            };
            spans.push(Span::styled(format!(" | {}", message.text), style));
        }
        Paragraph::new(Line::from(spans))
            .style(self.styles.base)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_current_mode() {
        let theme = Theme::default();
        for mode in [EditMode::Normal, EditMode::Insert] {
            let backend = TestBackend::new(7, 1);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|frame| {
                    frame.render_widget(StatusBar::new(mode, None, theme.ui()), frame.area())
                })
                .unwrap();

            let expected = match mode {
                EditMode::Normal => " NORMAL",
                EditMode::Insert => " INSERT",
            };
            terminal.backend().assert_buffer_lines([expected]);
        }
    }

    #[test]
    fn renders_error_message() {
        let message = StatusMessage::error("read failed");
        let theme = Theme::default();
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(EditMode::Normal, Some(&message), theme.ui()),
                    frame.area(),
                );
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("NORMAL | read failed"));
    }
}
