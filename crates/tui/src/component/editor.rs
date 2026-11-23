use editor::EditorSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position, Rect},
    style::Style,
    widgets::Widget,
};

use super::{
    buffer_tabs::BufferTabs,
    text_layout::TextFlow,
    text_view::TextView,
    welcome::{Welcome, WelcomeState},
};
use crate::{data_object::FocusState, theme::Theme};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct EditorViewOptions {
    flow: TextFlow,
}

impl EditorViewOptions {
    pub fn flow(self) -> TextFlow {
        self.flow
    }

    pub fn toggle_wrap(&mut self) {
        self.flow = match self.flow {
            TextFlow::SingleLine => TextFlow::Wrapped,
            TextFlow::Wrapped => TextFlow::SingleLine,
        };
    }
}

#[derive(Debug)]
pub struct Editor<'a> {
    snapshot: Option<&'a EditorSnapshot>,
    focus: FocusState,
    theme: &'a Theme,
    flow: TextFlow,
    welcome: Option<WelcomeState>,
    cursor_line: Option<Style>,
}

impl<'a> Editor<'a> {
    pub fn new(snapshot: Option<&'a EditorSnapshot>, focus: FocusState, theme: &'a Theme) -> Self {
        Self {
            snapshot,
            focus,
            theme,
            flow: TextFlow::SingleLine,
            welcome: None,
            cursor_line: None,
        }
    }

    pub fn with_cursor_line(mut self, style: Option<Style>) -> Self {
        self.cursor_line = style;
        self
    }

    pub fn with_welcome(mut self, state: WelcomeState) -> Self {
        self.welcome = Some(state);
        self
    }

    pub fn with_flow(mut self, flow: TextFlow) -> Self {
        self.flow = flow;
        self
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
        TextView::with_flow(&snapshot.text, self.theme, self.flow)
            .with_cursor(Some(cursor))
            .cursor_position(content)
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
                EditorBuffer::with_flow(snapshot, self.theme, self.flow)
                    .with_cursor_line(self.cursor_line)
                    .render(content_area, buf);
            }
            None => Welcome::new(
                self.theme.ui(),
                self.welcome.unwrap_or_default(),
                self.focus,
            )
            .render(area, buf),
        }
    }
}

/// Render buffer text without owning or mutating editor state.
#[derive(Debug, Clone, Copy)]
pub struct EditorBuffer<'a> {
    snapshot: &'a EditorSnapshot,
    theme: &'a Theme,
    flow: TextFlow,
    cursor_line: Option<Style>,
}

impl<'a> EditorBuffer<'a> {
    pub fn new(snapshot: &'a EditorSnapshot, theme: &'a Theme) -> Self {
        Self::with_flow(snapshot, theme, TextFlow::SingleLine)
    }

    pub fn with_flow(snapshot: &'a EditorSnapshot, theme: &'a Theme, flow: TextFlow) -> Self {
        Self {
            snapshot,
            theme,
            flow,
            cursor_line: None,
        }
    }

    pub fn with_cursor_line(mut self, style: Option<Style>) -> Self {
        self.cursor_line = style;
        self
    }

    /// Text region shared by rendering, viewport sizing, and cursor placement.
    pub fn text_area(area: Rect, snapshot: &EditorSnapshot) -> Rect {
        TextView::text_area(area, &snapshot.text)
    }
}

impl Widget for EditorBuffer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        TextView::with_flow(&self.snapshot.text, self.theme, self.flow)
            .with_cursor(self.snapshot.cursor)
            .with_cursor_line(self.cursor_line)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::{CursorMovement, DocumentFactory, EditorSession, TextViewSnapshot};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Style},
    };

    use super::*;

    fn create_buffer(session: &mut EditorSession, text: &str) {
        session.insert_document(DocumentFactory::new().create(text));
    }

    fn assert_text_lines(terminal: &Terminal<TestBackend>, expected: &[&str]) {
        let width = terminal.backend().buffer().area.width as usize;
        let actual = terminal
            .backend()
            .buffer()
            .content()
            .chunks(width)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected);
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

        assert_text_lines(&terminal, &["1 zero    ", "2 one     "]);
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

        assert_text_lines(&terminal, &[" 9 eight  ", "10 nine   "]);
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

        assert_text_lines(&terminal, &["1 cd"]);
    }

    #[test]
    fn wide_character_scrolling_uses_display_columns() {
        let mut session = EditorSession::new();
        create_buffer(&mut session, "a界b🙂c");
        session.resize_active_view(2, 1);
        for _ in 0..5 {
            session
                .active_editor()
                .unwrap()
                .move_cursor(CursorMovement::Right)
                .unwrap();
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

        let mut expected = Buffer::with_lines(["1 b🙂c"]);
        expected.set_style(
            Rect::new(0, 0, 6, 1),
            Style::new().fg(Color::White).bg(Color::Black),
        );
        expected[(4, 0)].set_fg(Color::Reset).set_bg(Color::Reset);
        assert_eq!(terminal.backend().buffer(), &expected);
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
        assert_text_lines(&terminal, &["1   "]);

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
        assert!(content.contains("eggplant"));
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
