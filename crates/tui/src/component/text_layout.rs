use std::ops::ControlFlow::{Break, Continue};

use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    text::Line,
    widgets::{Paragraph, Widget},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TextFlow {
    #[default]
    SingleLine,
    Wrapped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayLine {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextPoint {
    pub row: usize,
    pub column: usize,
}

/// Pure display-column to UTF-8 byte mapping shared by text and editor layouts.
pub fn byte_at_display_width(value: &str, column: usize) -> usize {
    let result = value
        .char_indices()
        .try_fold(0, |width, (index, character)| {
            if width >= column {
                Break(index)
            } else {
                Continue(width + UnicodeWidthChar::width(character).unwrap_or(0))
            }
        });
    match result {
        Continue(_) => value.len(),
        Break(index) => index,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextLayout {
    flow: TextFlow,
    lines: Vec<DisplayLine>,
    cursor: TextPoint,
}

impl TextLayout {
    pub fn new(value: &str, cursor_byte: usize, width: u16, flow: TextFlow) -> Self {
        let width = width as usize;
        match flow {
            TextFlow::SingleLine => {
                let cursor_column = UnicodeWidthStr::width(&value[..cursor_byte.min(value.len())]);
                Self {
                    flow,
                    lines: vec![DisplayLine {
                        start: 0,
                        end: value.len(),
                        text: value.to_string(),
                    }],
                    cursor: TextPoint {
                        row: 0,
                        column: cursor_column,
                    },
                }
            }
            TextFlow::Wrapped => Self::wrapped(value, cursor_byte, width),
        }
    }

    fn wrapped(value: &str, cursor_byte: usize, width: usize) -> Self {
        let mut builder = WrapBuilder::new(cursor_byte);
        value.char_indices().for_each(|(index, character)| {
            builder.push(value, index, character, width);
        });
        let (lines, cursor) = builder.finish(value);
        Self {
            flow: TextFlow::Wrapped,
            lines,
            cursor,
        }
    }

    pub fn lines(&self) -> &[DisplayLine] {
        &self.lines
    }

    pub fn cursor(&self) -> TextPoint {
        self.cursor
    }

    pub fn preferred_height(&self) -> u16 {
        u16::try_from(self.lines.len()).unwrap_or(u16::MAX)
    }

    pub fn cursor_position(&self, area: Rect) -> Option<Position> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let column = self
            .cursor
            .column
            .saturating_sub(self.horizontal_offset(area.width));
        if column >= area.width as usize {
            return None;
        }
        let row = self
            .cursor
            .row
            .saturating_sub(self.vertical_offset(area.height));
        if row >= area.height as usize {
            return None;
        }
        Some(Position::new(area.x + column as u16, area.y + row as u16))
    }

    pub fn render(&self, area: Rect, buffer: &mut Buffer) {
        match self.flow {
            TextFlow::SingleLine => {
                let offset = self.horizontal_offset(area.width).min(u16::MAX as usize) as u16;
                Paragraph::new(self.lines[0].text.clone())
                    .scroll((0, offset))
                    .render(area, buffer);
            }
            TextFlow::Wrapped => {
                let offset = self.vertical_offset(area.height);
                let lines = self
                    .lines
                    .iter()
                    .skip(offset)
                    .take(area.height as usize)
                    .map(|line| Line::raw(line.text.clone()))
                    .collect::<Vec<_>>();
                Paragraph::new(lines).render(area, buffer);
            }
        }
    }

    fn horizontal_offset(&self, width: u16) -> usize {
        match self.flow {
            TextFlow::SingleLine => self
                .cursor
                .column
                .saturating_sub(width.saturating_sub(1) as usize),
            TextFlow::Wrapped => 0,
        }
    }

    pub fn vertical_offset(&self, height: u16) -> usize {
        match self.flow {
            TextFlow::SingleLine => 0,
            TextFlow::Wrapped => self
                .cursor
                .row
                .saturating_sub((height as usize).saturating_sub(1)),
        }
    }
}

#[derive(Debug)]
struct WrapBuilder {
    lines: Vec<DisplayLine>,
    start: usize,
    width: usize,
    cursor: Option<TextPoint>,
    cursor_byte: usize,
}

impl WrapBuilder {
    fn new(cursor_byte: usize) -> Self {
        Self {
            lines: Vec::new(),
            start: 0,
            width: 0,
            cursor: None,
            cursor_byte,
        }
    }

    fn push(&mut self, value: &str, index: usize, character: char, max_width: usize) {
        if character == '\n' {
            self.record_cursor(index);
            self.finish_line(value, index);
            self.start = index + 1;
            return;
        }
        let character_width = UnicodeWidthChar::width(character).unwrap_or(0);
        if self.width > 0 && self.width + character_width > max_width {
            self.finish_line(value, index);
            self.start = index;
        }
        self.record_cursor(index);
        self.width += character_width;
    }

    fn finish(mut self, value: &str) -> (Vec<DisplayLine>, TextPoint) {
        self.record_cursor(value.len());
        self.finish_line(value, value.len());
        let cursor = self.cursor.unwrap_or(TextPoint { row: 0, column: 0 });
        (self.lines, cursor)
    }

    fn record_cursor(&mut self, index: usize) {
        if self.cursor.is_none() && index == self.cursor_byte {
            self.cursor = Some(TextPoint {
                row: self.lines.len(),
                column: self.width,
            });
        }
    }

    fn finish_line(&mut self, value: &str, end: usize) {
        self.lines.push(DisplayLine {
            start: self.start,
            end,
            text: value[self.start..end].to_string(),
        });
        self.start = end;
        self.width = 0;
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn display_width_mapping_is_pure_and_unicode_aware() {
        let value = "a界b🙂c";
        assert_eq!(byte_at_display_width(value, 0), 0);
        assert_eq!(byte_at_display_width(value, 1), 1);
        assert_eq!(byte_at_display_width(value, 3), 4);
        assert_eq!(byte_at_display_width(value, 4), 5);
        assert_eq!(byte_at_display_width(value, 6), 9);
        assert_eq!(byte_at_display_width(value, 100), value.len());
    }

    #[test]
    fn wrapped_layout_measures_unicode_lines_and_places_cursor() {
        let value = "a界b🙂c";
        let layout = TextLayout::new(value, "a界b".len(), 4, TextFlow::Wrapped);
        assert_eq!(layout.preferred_height(), 2);
        assert_eq!(
            layout.cursor_position(Rect::new(0, 0, 4, 2)),
            Some(Position::new(0, 1))
        );

        let end = TextLayout::new(value, value.len(), 4, TextFlow::Wrapped);
        assert_eq!(
            end.cursor_position(Rect::new(0, 0, 4, 1)),
            Some(Position::new(3, 0))
        );
    }

    #[test]
    fn wrapped_render_uses_the_same_vertical_scroll_as_the_cursor() {
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let layout = TextLayout::new("a界b🙂c", "a界b🙂c".len(), 4, TextFlow::Wrapped);
        terminal
            .draw(|frame| layout.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains('🙂'));
        assert!(content.contains('c'));
        assert!(!content.contains("a界b"));
    }

    #[test]
    fn wrapped_layout_supports_explicit_newlines_and_empty_text() {
        let layout = TextLayout::new("one\ntwo", 4, 3, TextFlow::Wrapped);
        assert_eq!(layout.preferred_height(), 2);
        assert_eq!(
            layout.cursor_position(Rect::new(0, 0, 3, 2)),
            Some(Position::new(0, 1))
        );
        assert_eq!(
            TextLayout::new("", 0, 1, TextFlow::Wrapped).preferred_height(),
            1
        );
    }
}
