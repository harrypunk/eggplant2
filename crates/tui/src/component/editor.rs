use editor::EditorSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position, Rect},
    widgets::{Paragraph, Widget},
};

use super::buffer_tabs::BufferTabs;
use crate::data_object::FocusState;

#[derive(Debug)]
pub struct Editor {
    snapshot: Option<EditorSnapshot>,
    focus: FocusState,
}

impl Editor {
    pub fn new(snapshot: Option<EditorSnapshot>, focus: FocusState) -> Self {
        Self { snapshot, focus }
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

        let cursor = self.snapshot.as_ref()?.cursor?;
        let content = Self::content_area(area);
        let column = u16::try_from(cursor.column).unwrap_or(u16::MAX);
        let row = u16::try_from(cursor.row).unwrap_or(u16::MAX);
        Some(Position::new(
            content.x.saturating_add(column),
            content.y.saturating_add(row),
        ))
    }
}

impl Widget for Editor {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [tabs_area, content_area] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(area);

        match self.snapshot {
            Some(snapshot) => {
                BufferTabs::new(&snapshot).render(tabs_area, buf);
                EditorBuffer::new(&snapshot).render(content_area, buf);
            }
            None => Paragraph::new("No file open").render(area, buf),
        }
    }
}

/// Render buffer text without owning or mutating editor state.
#[derive(Debug, Clone, Copy)]
pub struct EditorBuffer<'a> {
    snapshot: &'a EditorSnapshot,
}

impl<'a> EditorBuffer<'a> {
    pub fn new(snapshot: &'a EditorSnapshot) -> Self {
        Self { snapshot }
    }
}

impl Widget for EditorBuffer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let horizontal_offset = u16::try_from(self.snapshot.horizontal_offset).unwrap_or(u16::MAX);
        Paragraph::new(self.snapshot.lines.join("\n"))
            .scroll((0, horizontal_offset))
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditorSession};
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
            .draw(|frame| frame.render_widget(EditorBuffer::new(&snapshot), frame.area()))
            .unwrap();

        terminal
            .backend()
            .assert_buffer_lines(["zero      ", "one       "]);
    }

    #[test]
    fn applies_horizontal_scroll() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "abcdef");
        let mut snapshot = session.active_snapshot().unwrap();
        snapshot.lines = vec!["abcdef".to_string()];
        snapshot.horizontal_offset = 2;
        let backend = TestBackend::new(4, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(EditorBuffer::new(&snapshot), frame.area()))
            .unwrap();

        terminal.backend().assert_buffer_lines(["cdef"]);
    }

    #[test]
    fn renders_empty_editor_state() {
        let backend = TestBackend::new(20, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(Editor::new(None, FocusState::Unfocused), frame.area())
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

        terminal
            .draw(|frame| {
                frame.render_widget(
                    Editor::new(session.active_snapshot(), FocusState::Focused),
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
