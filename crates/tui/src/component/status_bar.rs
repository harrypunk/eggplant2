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
    pending_input: Option<&'a str>,
    styles: &'a UiTheme,
}

impl<'a> StatusBar<'a> {
    pub fn new(
        mode: EditMode,
        message: Option<&'a StatusMessage>,
        pending_input: Option<&'a str>,
        styles: &'a UiTheme,
    ) -> Self {
        Self {
            mode,
            message,
            pending_input,
            styles,
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (mode, mode_style) = match self.mode {
            EditMode::Normal => ("NORMAL", self.styles.status_normal),
            EditMode::Insert => ("INSERT", self.styles.status_insert),
            EditMode::Visual => ("VISUAL", self.styles.selection),
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
        if let Some(pending) = self.pending_input {
            spans.push(Span::styled(
                format!(" | {pending}"),
                self.styles.base.patch(self.styles.key_hint),
            ));
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
        for mode in [EditMode::Normal, EditMode::Insert, EditMode::Visual] {
            let backend = TestBackend::new(7, 1);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|frame| {
                    frame.render_widget(StatusBar::new(mode, None, None, theme.ui()), frame.area())
                })
                .unwrap();

            let expected = match mode {
                EditMode::Normal => " NORMAL",
                EditMode::Insert => " INSERT",
                EditMode::Visual => " VISUAL",
            };
            let content = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert_eq!(content, expected);
        }
    }

    #[test]
    fn renders_transient_pending_input() {
        let theme = Theme::default();
        let backend = TestBackend::new(20, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(EditMode::Normal, None, Some("12g"), theme.ui()),
                    frame.area(),
                )
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("NORMAL | 12g"));
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
                    StatusBar::new(EditMode::Normal, Some(&message), None, theme.ui()),
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
