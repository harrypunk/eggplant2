use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, Clear, Widget},
};

/// Reusable visual container for popups, pickers, and dialogs.
///
/// The page owns placement and input routing owns modality. `Popup` only clears
/// the background, draws the frame, and renders its content in the inner area.
#[derive(Debug)]
pub struct Popup<W> {
    title: Line<'static>,
    content: W,
}

impl<W> Popup<W> {
    pub fn new(title: impl Into<Line<'static>>, content: W) -> Self {
        Self {
            title: title.into(),
            content,
        }
    }
}

impl<W: Widget> Widget for Popup<W> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered().title(self.title);
        let inner = block.inner(area);

        Clear.render(area, buf);
        block.render(area, buf);
        self.content.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    use super::*;

    #[test]
    fn renders_title_border_and_content() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let popup = Popup::new("Picker", Paragraph::new("content"));
                frame.render_widget(popup, frame.area());
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Picker"));
        assert!(content.contains("content"));
        assert!(content.contains('┌'));
    }
}
