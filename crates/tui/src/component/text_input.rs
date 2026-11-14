use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::theme::UiTheme;

/// Single-line input with an end-of-value terminal cursor and horizontal scrolling.
#[derive(Debug, Clone, Copy)]
pub struct TextInput<'a> {
    value: &'a str,
    styles: &'a UiTheme,
}

impl<'a> TextInput<'a> {
    pub fn new(value: &'a str, styles: &'a UiTheme) -> Self {
        Self { value, styles }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        let inner = Block::bordered().inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        let width = UnicodeWidthStr::width(self.value);
        let offset = self.scroll_offset(inner);
        let column = width.saturating_sub(offset);
        let column = u16::try_from(column).unwrap_or(u16::MAX);
        if column >= inner.width {
            return None;
        }
        Some(Position::new(inner.x + column, inner.y))
    }

    fn scroll_offset(self, inner: Rect) -> usize {
        let visible = inner.width.saturating_sub(1).into();
        UnicodeWidthStr::width(self.value)
            .saturating_sub(visible)
            .min(u16::MAX.into())
    }
}

impl Widget for TextInput<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let surface = self.styles.popup_surface();
        let border = surface.patch(self.styles.border);
        let inner = Block::bordered().inner(area);
        let offset = u16::try_from(self.scroll_offset(inner)).unwrap_or(u16::MAX);
        Paragraph::new(self.value)
            .style(surface)
            .block(Block::bordered().style(surface).border_style(border))
            .scroll((0, offset))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn cursor_follows_display_width_and_scrolls_long_values() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 6, 3);
        assert_eq!(
            TextInput::new("", theme.ui()).cursor_position(area),
            Some(Position::new(1, 1))
        );
        assert_eq!(
            TextInput::new("a界b🙂c", theme.ui()).cursor_position(area),
            Some(Position::new(4, 1))
        );
    }

    #[test]
    fn tiny_inputs_do_not_publish_an_out_of_bounds_cursor() {
        let theme = Theme::default();
        for area in [Rect::new(0, 0, 0, 0), Rect::new(0, 0, 2, 2)] {
            assert_eq!(
                TextInput::new("query", theme.ui()).cursor_position(area),
                None
            );
        }
    }

    #[test]
    fn renders_the_tail_when_the_value_exceeds_the_visible_width() {
        let theme = Theme::default();
        let backend = TestBackend::new(6, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(TextInput::new("abcdef", theme.ui()), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("def"));
        assert!(!content.contains("abcdef"));
    }
}
