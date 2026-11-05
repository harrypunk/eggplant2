use editor::EditMode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::data_object::{StatusKind, StatusMessage};

#[derive(Debug, Clone, Copy)]
pub struct StatusBar<'a> {
    mode: EditMode,
    message: Option<&'a StatusMessage>,
}

impl<'a> StatusBar<'a> {
    pub fn new(mode: EditMode, message: Option<&'a StatusMessage>) -> Self {
        Self { mode, message }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode = match self.mode {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
        };
        let mut spans = vec![Span::from(format!(" {mode}"))];
        if let Some(message) = self.message {
            let style = match message.kind {
                StatusKind::Error => Style::new().fg(Color::Red),
                StatusKind::Info => Style::new(),
            };
            spans.push(Span::styled(format!(" | {}", message.text), style));
        }
        Paragraph::new(Line::from(spans)).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_current_mode() {
        for mode in [EditMode::Normal, EditMode::Insert] {
            let backend = TestBackend::new(7, 1);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal
                .draw(|frame| frame.render_widget(StatusBar::new(mode, None), frame.area()))
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
        let backend = TestBackend::new(30, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    StatusBar::new(EditMode::Normal, Some(&message)),
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
