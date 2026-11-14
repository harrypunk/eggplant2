use ratatui::{
    buffer::Buffer,
    layout::Rect,
    text::Line,
    widgets::{Block, Clear, Widget},
};

use crate::theme::UiTheme;

/// Reusable visual container for popups, pickers, and dialogs.
///
/// The page owns placement and input routing owns modality. `Popup` only clears
/// the background, draws the frame, and renders its content in the inner area.
#[derive(Debug)]
pub struct Popup<'a, W> {
    title: Line<'static>,
    content: W,
    styles: &'a UiTheme,
}

impl<'a, W> Popup<'a, W> {
    pub fn new(title: impl Into<Line<'static>>, content: W, styles: &'a UiTheme) -> Self {
        Self {
            title: title.into(),
            content,
            styles,
        }
    }
}

pub fn content_area(area: Rect) -> Rect {
    Block::bordered().inner(area)
}

impl<W: Widget> Widget for Popup<'_, W> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let surface = self.styles.popup_surface();
        let block = Block::bordered()
            .border_style(surface.patch(self.styles.border))
            .style(surface)
            .title(self.title);
        let inner = content_area(area);

        Clear.render(area, buf);
        block.render(area, buf);
        self.content.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_title_border_and_content() {
        let theme = Theme::default();
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                let popup = Popup::new("Picker", Paragraph::new("content"), theme.ui());
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
