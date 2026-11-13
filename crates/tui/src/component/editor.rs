use editor::EditorSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position, Rect},
    widgets::{Paragraph, Widget},
};

use super::{buffer_tabs::BufferTabs, text_view::TextView};
use crate::{data_object::FocusState, theme::Theme};

#[derive(Debug)]
pub struct Editor<'a> {
    snapshot: Option<&'a EditorSnapshot>,
    focus: FocusState,
    theme: &'a Theme,
}

impl<'a> Editor<'a> {
    pub fn new(snapshot: Option<&'a EditorSnapshot>, focus: FocusState, theme: &'a Theme) -> Self {
        Self {
            snapshot,
            focus,
            theme,
        }
    }

    pub fn content_area(area: Rect) -> Rect {
        let [_, content] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
            .flex(Flex::Start)
            .areas(area);
        content
    }

    /// Calculate the terminal cursor without requiring access to a Frame.
    pub fn cursor_position(&self, area: Rect) -> Option<Position> {
        match self.focus {
            FocusState::Unfocused => return None,
            FocusState::Focused => {}
        }

        let snapshot = self.snapshot.as_ref()?;
        let cursor = snapshot.cursor?;
        let content = EditorBuffer::text_area(Self::content_area(area), snapshot);
        if content.width == 0 || content.height == 0 {
            return None;
        }
        let column = u16::try_from(cursor.column).unwrap_or(u16::MAX);
        let row = u16::try_from(cursor.row).unwrap_or(u16::MAX);
        if column >= content.width || row >= content.height {
            return None;
        }
        Some(Position::new(
            content.x.saturating_add(column),
            content.y.saturating_add(row),
        ))
    }
}

impl Widget for Editor<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [tabs_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(area);

        match self.snapshot {
            Some(snapshot) => {
                BufferTabs::new(snapshot, self.theme.ui()).render(tabs_area, buf);
                EditorBuffer::new(snapshot, self.theme).render(content_area, buf);
            }
            None => Paragraph::new("No file open")
                .style(self.theme.ui().base)
                .render(area, buf),
        }
    }
}

/// Render buffer text without owning or mutating editor state.
#[derive(Debug, Clone, Copy)]
pub struct EditorBuffer<'a> {
    snapshot: &'a EditorSnapshot,
    theme: &'a Theme,
}

impl<'a> EditorBuffer<'a> {
    pub fn new(snapshot: &'a EditorSnapshot, theme: &'a Theme) -> Self {
        Self { snapshot, theme }
    }

    /// Text region shared by rendering, viewport sizing, and cursor placement.
    pub fn text_area(area: Rect, snapshot: &EditorSnapshot) -> Rect {
        TextView::text_area(area, &snapshot.text)
    }
}

impl Widget for EditorBuffer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        TextView::new(&self.snapshot.text, self.theme).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::{CursorMovement, DocumentFactory, EditorSession, TextViewSnapshot};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn create_buffer(session: &mut EditorSession, text: &str) {
        session.insert_document(DocumentFactory::new().create(text));
    }

    #[test]
    fn renders_snapshot_lines() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "zero\none");
        session.resize_active_view(10, 3);
        let snapshot = session.active_snapshot().unwrap();
        let backend = TestBackend::new(10, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    EditorBuffer::new(&snapshot, &Theme::default()),
                    frame.area(),
                )
            })
            .unwrap();

        terminal
            .backend()
            .assert_buffer_lines(["1 zero    ", "2 one     "]);
    }

    #[test]
    fn numbers_scrolled_lines_and_sizes_the_gutter_from_the_document() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "eight\nnine");
        session.resize_active_view(10, 3);
        let mut snapshot = session.active_snapshot().unwrap();
        snapshot.text.first_line = 8;
        snapshot.text.line_count = 12;
        let backend = TestBackend::new(10, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    EditorBuffer::new(&snapshot, &Theme::default()),
                    frame.area(),
                )
            })
            .unwrap();

        terminal
            .backend()
            .assert_buffer_lines([" 9 eight  ", "10 nine   "]);
    }

    #[test]
    fn applies_horizontal_scroll() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "abcdef");
        let mut snapshot = session.active_snapshot().unwrap();
        snapshot.text = TextViewSnapshot::plain(vec!["abcdef".to_string()]);
        snapshot.text.horizontal_offset = 2;
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    EditorBuffer::new(&snapshot, &Theme::default()),
                    frame.area(),
                )
            })
            .unwrap();

        terminal.backend().assert_buffer_lines(["1 cd"]);
    }

    #[test]
    fn wide_character_scrolling_uses_display_columns() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "a界b🙂c");
        session.resize_active_view(2, 1);
        for _ in 0..5 {
            session.move_cursor(CursorMovement::Right).unwrap();
        }
        let snapshot = session.active_snapshot().unwrap();
        let cursor = snapshot.cursor.unwrap();
        assert!(cursor.column < 2);
        assert_eq!(snapshot.text.horizontal_offset + cursor.column, 7);

        let mut snapshot = session.active_snapshot().unwrap();
        snapshot.text = TextViewSnapshot::plain(vec!["a界b🙂c".to_string()]);
        snapshot.text.horizontal_offset = 3;
        let backend = TestBackend::new(6, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    EditorBuffer::new(&snapshot, &Theme::default()),
                    frame.area(),
                )
            })
            .unwrap();

        terminal.backend().assert_buffer_lines(["1 b🙂c"]);
    }

    #[test]
    fn renders_empty_documents_and_handles_zero_sized_views() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "");
        session.resize_active_view(10, 3);
        let snapshot = session.active_snapshot().unwrap();
        assert_eq!(snapshot.text.line_count, 1);
        assert_eq!(snapshot.text.lines.len(), 1);
        assert_eq!(snapshot.text.lines[0].text, "");
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    EditorBuffer::new(&snapshot, &Theme::default()),
                    frame.area(),
                )
            })
            .unwrap();
        terminal.backend().assert_buffer_lines(["1   "]);

        session.resize_active_view(0, 0);
        let snapshot = session.active_snapshot().unwrap();
        assert!(snapshot.text.lines.is_empty());
        assert!(snapshot.cursor.is_none());
    }

    #[test]
    fn renders_empty_editor_state() {
        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    Editor::new(None, FocusState::Unfocused, &Theme::default()),
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
        assert!(content.contains("No file open"));
    }

    #[test]
    fn renders_snapshot_inside_editor_border() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "hello");
        session.resize_active_view(10, 3);
        let backend = TestBackend::new(20, 4);
        let mut terminal = Terminal::new(backend).unwrap();
        let snapshot = session.active_snapshot();

        terminal
            .draw(|frame| {
                frame.render_widget(
                    Editor::new(snapshot.as_ref(), FocusState::Focused, &Theme::default()),
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
        assert!(content.contains("hello"));
        assert!(!content.contains("NORMAL"));
    }
}
