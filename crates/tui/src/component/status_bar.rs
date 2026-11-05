use editor::EditMode;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Paragraph, Widget},
};

#[derive(Debug, Clone, Copy)]
pub struct StatusBar {
    mode: EditMode,
}

impl StatusBar {
    pub fn new(mode: EditMode) -> Self {
        Self { mode }
    }
}

impl Widget for StatusBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode = match self.mode {
            EditMode::Normal => "NORMAL",
            EditMode::Insert => "INSERT",
        };
        Paragraph::new(format!(" {mode}")).render(area, buf);
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
                .draw(|frame| frame.render_widget(StatusBar::new(mode), frame.area()))
                .unwrap();

            let expected = match mode {
                EditMode::Normal => " NORMAL",
                EditMode::Insert => " INSERT",
            };
            terminal.backend().assert_buffer_lines([expected]);
        }
    }
}
