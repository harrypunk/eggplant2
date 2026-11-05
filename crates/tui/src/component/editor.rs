use editor::EditorSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    widgets::{Block, BorderType, Paragraph, Widget},
};

#[derive(Debug)]
pub struct Editor {
    snapshot: Option<EditorSnapshot>,
    is_focused: bool,
}

impl Editor {
    pub fn new(snapshot: Option<EditorSnapshot>, is_focused: bool) -> Self {
        Self {
            snapshot,
            is_focused,
        }
    }

    pub fn content_area(area: Rect) -> Rect {
        Block::bordered().inner(area)
    }

    /// Calculate the terminal cursor without requiring access to a Frame.
    pub fn cursor_position(&self, area: Rect) -> Option<Position> {
        if !self.is_focused {
            return None;
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
        let title = match self.snapshot.as_ref() {
            Some(snapshot) => {
                let modified = if snapshot.modified { " [+]" } else { "" };
                format!("Editor | {}{modified}", snapshot.display_name)
            }
            None => "Editor".to_string(),
        };
        let border = if self.is_focused {
            BorderType::Double
        } else {
            BorderType::Plain
        };
        let block = Block::bordered().border_type(border).title(title);
        let inner = Self::content_area(area);
        block.render(area, buf);

        match self.snapshot {
            Some(snapshot) => EditorBuffer::new(&snapshot).render(inner, buf),
            None => Paragraph::new("No file open").render(inner, buf),
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
    use editor::EditorSession;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_snapshot_lines() {
        let mut session = EditorSession::new();
        session.create_buffer("zero\none");
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
        session.create_buffer("abcdef");
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
            .draw(|frame| frame.render_widget(Editor::new(None, false), frame.area()))
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
        session.create_buffer("hello");
        session.resize_active_view(10, 3);
        let backend = TestBackend::new(20, 4);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(Editor::new(session.active_snapshot(), true), frame.area());
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
