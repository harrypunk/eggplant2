use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::theme::UiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCursorMovement {
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEvent {
    Character(char),
    Cursor(TextCursorMovement),
}

/// Result of local component input. Feature effects depend only on this stable interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextInputChange {
    content_changed: bool,
}

impl TextInputChange {
    pub fn content_changed(self) -> bool {
        self.content_changed
    }

    fn cursor_moved() -> Self {
        Self {
            content_changed: false,
        }
    }

    fn text_changed() -> Self {
        Self {
            content_changed: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TextCursor {
    offset: usize,
}

impl TextCursor {
    fn end(value: &str) -> Self {
        Self {
            offset: value.chars().count(),
        }
    }

    fn apply(self, value: &str, movement: TextCursorMovement) -> Self {
        let offset = self.offset(value);
        let offset = match movement {
            TextCursorMovement::Left => offset.saturating_sub(1),
            TextCursorMovement::Right => (offset + 1).min(value.chars().count()),
        };
        Self { offset }
    }

    fn insert(self, value: &mut String, character: char) -> Self {
        let offset = self.offset(value);
        value.insert(self.byte_index(value), character);
        Self { offset: offset + 1 }
    }

    fn backspace(self, value: &mut String) -> Option<Self> {
        let offset = self.offset(value);
        if offset == 0 {
            return None;
        }
        let cursor = Self { offset: offset - 1 };
        value.remove(cursor.byte_index(value));
        Some(cursor)
    }

    fn byte_index(self, value: &str) -> usize {
        value
            .char_indices()
            .nth(self.offset(value))
            .map(|(index, _)| index)
            .unwrap_or(value.len())
    }

    fn offset(self, value: &str) -> usize {
        self.offset.min(value.chars().count())
    }
}

/// Stateful single-line editor owned by a popup feature.
///
/// Value, cursor movement, and text editing are local component concerns. Features
/// forward events and react only to content changes, not cursor implementation details.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TextInput {
    value: String,
    cursor: TextCursor,
}

impl TextInput {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = TextCursor::end(&value);
        Self { value, cursor }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        *self = Self::new(value);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn insert(&mut self, character: char) {
        self.cursor = self.cursor.insert(&mut self.value, character);
    }

    pub fn backspace(&mut self) -> bool {
        match self.cursor.backspace(&mut self.value) {
            Some(cursor) => {
                self.cursor = cursor;
                true
            }
            None => false,
        }
    }

    pub fn handle_event(&mut self, event: TextInputEvent) -> TextInputChange {
        match event {
            TextInputEvent::Character(character) => {
                self.insert(character);
                TextInputChange::text_changed()
            }
            TextInputEvent::Cursor(movement) => {
                self.cursor = self.cursor.apply(&self.value, movement);
                TextInputChange::cursor_moved()
            }
        }
    }

    pub fn view<'a>(&'a self, styles: &'a UiTheme) -> TextInputView<'a> {
        TextInputView {
            value: &self.value,
            cursor: self.cursor,
            styles,
        }
    }

    pub fn cursor_position(&self, area: Rect, styles: &UiTheme) -> Option<Position> {
        self.view(styles).cursor_position(area)
    }
}

impl std::ops::Deref for TextInput {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

/// Stateless cursor-aware rendering of a [`TextInput`] component.
#[derive(Debug, Clone, Copy)]
pub struct TextInputView<'a> {
    value: &'a str,
    cursor: TextCursor,
    styles: &'a UiTheme,
}

impl<'a> TextInputView<'a> {
    pub fn new(value: &'a str, styles: &'a UiTheme) -> Self {
        Self {
            value,
            cursor: TextCursor::end(value),
            styles,
        }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        let inner = Block::bordered().inner(area);
        if inner.width == 0 || inner.height == 0 {
            return None;
        }
        let width = self.cursor_width();
        let offset = self.scroll_offset(inner);
        let column = width.saturating_sub(offset);
        let column = u16::try_from(column).unwrap_or(u16::MAX);
        if column >= inner.width {
            return None;
        }
        Some(Position::new(inner.x + column, inner.y))
    }

    fn cursor_width(self) -> usize {
        let index = self.cursor.byte_index(self.value);
        UnicodeWidthStr::width(&self.value[..index])
    }

    fn scroll_offset(self, inner: Rect) -> usize {
        let visible = inner.width.saturating_sub(1).into();
        self.cursor_width()
            .saturating_sub(visible)
            .min(u16::MAX.into())
    }
}

impl Widget for TextInputView<'_> {
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
    fn component_moves_and_edits_at_unicode_scalar_boundaries() {
        let mut input = TextInput::default();
        ['a', '界', '🙂', 'b']
            .into_iter()
            .for_each(|character| input.insert(character));
        assert!(
            !input
                .handle_event(TextInputEvent::Cursor(TextCursorMovement::Left))
                .content_changed()
        );
        assert!(
            !input
                .handle_event(TextInputEvent::Cursor(TextCursorMovement::Left))
                .content_changed()
        );
        assert!(
            input
                .handle_event(TextInputEvent::Character('x'))
                .content_changed()
        );
        assert_eq!(input.value(), "a界x🙂b");
        assert!(input.backspace());
        assert_eq!(input.value(), "a界🙂b");
        assert_eq!(input.cursor.byte_index(input.value()), "a界".len());
        input.handle_event(TextInputEvent::Cursor(TextCursorMovement::Right));
        input.handle_event(TextInputEvent::Cursor(TextCursorMovement::Right));
        assert_eq!(input.cursor, TextCursor::end(input.value()));
    }

    #[test]
    fn cursor_position_tracks_the_component_cursor_instead_of_the_value_end() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 6, 3);
        let mut input = TextInput::new("abcdef");
        input.handle_event(TextInputEvent::Cursor(TextCursorMovement::Left));
        input.handle_event(TextInputEvent::Cursor(TextCursorMovement::Left));
        assert_eq!(
            input.cursor_position(area, theme.ui()),
            Some(Position::new(4, 1))
        );

        let input = TextInput::new("abc");
        assert_eq!(
            input.cursor_position(area, theme.ui()),
            Some(Position::new(4, 1))
        );
    }

    #[test]
    fn default_view_keeps_end_of_value_cursor_behavior() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 6, 3);
        assert_eq!(
            TextInputView::new("", theme.ui()).cursor_position(area),
            Some(Position::new(1, 1))
        );
        assert_eq!(
            TextInputView::new("a界b🙂c", theme.ui()).cursor_position(area),
            Some(Position::new(4, 1))
        );
    }

    #[test]
    fn tiny_inputs_do_not_publish_an_out_of_bounds_cursor() {
        let theme = Theme::default();
        for area in [Rect::new(0, 0, 0, 0), Rect::new(0, 0, 2, 2)] {
            assert_eq!(
                TextInputView::new("query", theme.ui()).cursor_position(area),
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
            .draw(|frame| {
                frame.render_widget(TextInputView::new("abcdef", theme.ui()), frame.area())
            })
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
