use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect, Size},
    widgets::{Block, Widget},
};

use super::text_layout::{TextFlow, TextLayout};
use crate::theme::UiTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextCursorMovement {
    Left,
    Right,
}

/// Semantic text-editing operations, independent of physical key bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEvent {
    Insert(char),
    DeleteBackward,
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
            TextInputEvent::Insert(character) => {
                self.insert(character);
                TextInputChange::text_changed()
            }
            TextInputEvent::Cursor(movement) => {
                self.cursor = self.cursor.apply(&self.value, movement);
                TextInputChange::cursor_moved()
            }
            TextInputEvent::DeleteBackward => match self.backspace() {
                true => TextInputChange::text_changed(),
                false => TextInputChange::cursor_moved(),
            },
        }
    }

    pub fn view<'a>(&'a self, styles: &'a UiTheme) -> TextInputView<'a> {
        self.view_with_flow(styles, TextFlow::SingleLine)
    }

    pub fn view_with_flow<'a>(&'a self, styles: &'a UiTheme, flow: TextFlow) -> TextInputView<'a> {
        TextInputView {
            value: &self.value,
            cursor: self.cursor,
            flow,
            styles,
            style: &PLAIN_TEXT_INPUT_STYLE,
        }
    }

    pub fn measure(&self, width: u16, flow: TextFlow) -> Size {
        let layout = TextLayout::new(
            &self.value,
            self.cursor.byte_index(&self.value),
            width.saturating_sub(2),
            flow,
        );
        Size::new(width, layout.preferred_height().saturating_add(2))
    }

    pub fn cursor_position(&self, area: Rect, styles: &UiTheme) -> Option<Position> {
        self.view(styles).cursor_position(area)
    }

    pub fn cursor_position_with_flow(
        &self,
        area: Rect,
        styles: &UiTheme,
        flow: TextFlow,
    ) -> Option<Position> {
        self.view_with_flow(styles, flow).cursor_position(area)
    }
}

impl std::ops::Deref for TextInput {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

/// Original value and cursor positions supplied to a view-style implementation.
#[derive(Debug, Clone, Copy)]
pub struct TextInputStyleInput<'a> {
    pub value: &'a str,
    pub cursor_byte: usize,
    pub cursor_offset: usize,
}

/// Owned text and cursor byte produced by a view style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextInputStyleOutput {
    pub text: String,
    pub cursor_byte: usize,
}

/// Presentation transformation independent of text editing and cursor movement.
pub trait TextInputViewStyle: std::fmt::Debug {
    fn style(&self, input: TextInputStyleInput<'_>) -> TextInputStyleOutput;
}

#[derive(Debug)]
struct PlainTextInputStyle;

static PLAIN_TEXT_INPUT_STYLE: PlainTextInputStyle = PlainTextInputStyle;

impl TextInputViewStyle for PlainTextInputStyle {
    fn style(&self, input: TextInputStyleInput<'_>) -> TextInputStyleOutput {
        TextInputStyleOutput {
            text: input.value.to_string(),
            cursor_byte: input.cursor_byte,
        }
    }
}

/// Stateless cursor-aware rendering of a [`TextInput`] component.
#[derive(Debug, Clone, Copy)]
pub struct TextInputView<'a> {
    value: &'a str,
    cursor: TextCursor,
    flow: TextFlow,
    styles: &'a UiTheme,
    style: &'a dyn TextInputViewStyle,
}

impl<'a> TextInputView<'a> {
    pub fn new(value: &'a str, styles: &'a UiTheme) -> Self {
        Self {
            value,
            cursor: TextCursor::end(value),
            flow: TextFlow::SingleLine,
            styles,
            style: &PLAIN_TEXT_INPUT_STYLE,
        }
    }

    pub fn with_style(mut self, style: &'a dyn TextInputViewStyle) -> Self {
        self.style = style;
        self
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        self.layout(area)
            .cursor_position(Block::bordered().inner(area))
    }

    fn layout(self, area: Rect) -> TextLayout {
        let output = self.style.style(TextInputStyleInput {
            value: self.value,
            cursor_byte: self.cursor.byte_index(self.value),
            cursor_offset: self.cursor.offset(self.value),
        });
        TextLayout::new(
            &output.text,
            output.cursor_byte,
            Block::bordered().inner(area).width,
            self.flow,
        )
    }
}

impl Widget for TextInputView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let surface = self.styles.popup_surface();
        let border = surface.patch(self.styles.border);
        Block::bordered()
            .style(surface)
            .border_style(border)
            .render(area, buf);
        self.layout(area).render(Block::bordered().inner(area), buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[derive(Debug)]
    struct MaskStyle;

    impl TextInputViewStyle for MaskStyle {
        fn style(&self, input: TextInputStyleInput<'_>) -> TextInputStyleOutput {
            let text = "•".repeat(input.value.chars().count());
            let cursor_byte = text
                .char_indices()
                .nth(input.cursor_offset)
                .map(|(index, _)| index)
                .unwrap_or(text.len());
            TextInputStyleOutput { text, cursor_byte }
        }
    }

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
                .handle_event(TextInputEvent::Insert('x'))
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
    fn injected_mask_style_hides_the_value_and_tracks_scalar_cursor_position() {
        let theme = Theme::default();
        let area = Rect::new(0, 0, 10, 3);
        let mut input = TextInput::new("a界🙂");
        input.handle_event(TextInputEvent::Cursor(TextCursorMovement::Left));
        let style = MaskStyle;
        let view = input.view(theme.ui()).with_style(&style);
        assert_eq!(view.cursor_position(area), Some(Position::new(3, 1)));

        let mut terminal = Terminal::new(TestBackend::new(10, 3)).unwrap();
        terminal
            .draw(|frame| frame.render_widget(view, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("•••"));
        assert!(!content.contains('a'));
        assert!(!content.contains('界'));
        assert!(!content.contains('🙂'));
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
    fn wrapped_measurement_grows_with_visual_lines() {
        let input = TextInput::new("abc");
        assert_eq!(input.measure(20, TextFlow::SingleLine), Size::new(20, 3));
        assert_eq!(input.measure(20, TextFlow::Wrapped), Size::new(20, 3));

        let input = TextInput::new("12345678901234567890");
        assert_eq!(input.measure(10, TextFlow::Wrapped), Size::new(10, 5));
    }

    #[test]
    fn delete_backward_event_reports_actual_content_changes() {
        let mut input = TextInput::new("界");
        assert!(
            input
                .handle_event(TextInputEvent::DeleteBackward)
                .content_changed()
        );
        assert_eq!(input.value(), "");
        assert!(
            !input
                .handle_event(TextInputEvent::DeleteBackward)
                .content_changed()
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
