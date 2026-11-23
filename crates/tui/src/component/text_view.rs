use editor::{LineChange, LineSnapshot, TextViewSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Position, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use super::text_layout::{DisplayLine, TextFlow, TextLayout, TextPoint, byte_at_display_width};
use crate::{
    theme::Theme,
    ui::{
        EditorDecorationContext, EditorDecorationContribution, EditorDecorationPhase,
        EditorVisualRow,
    },
};

/// Shared read-only document viewport for editor buffers and text previews.
#[derive(Debug, Clone, Copy)]
pub struct TextView<'a> {
    snapshot: &'a TextViewSnapshot,
    theme: &'a Theme,
    flow: TextFlow,
    cursor: Option<editor::CursorPosition>,
    decorations: &'a [EditorDecorationContribution],
}

impl<'a> TextView<'a> {
    pub fn new(snapshot: &'a TextViewSnapshot, theme: &'a Theme) -> Self {
        Self::with_flow(snapshot, theme, TextFlow::SingleLine)
    }

    pub fn with_flow(snapshot: &'a TextViewSnapshot, theme: &'a Theme, flow: TextFlow) -> Self {
        Self {
            snapshot,
            theme,
            flow,
            cursor: None,
            decorations: &[],
        }
    }

    pub fn with_cursor(mut self, cursor: Option<editor::CursorPosition>) -> Self {
        self.cursor = cursor;
        self
    }

    pub fn with_decorations(mut self, decorations: &'a [EditorDecorationContribution]) -> Self {
        self.decorations = decorations;
        self
    }

    /// Text region shared by rendering, viewport sizing, and cursor placement.
    pub fn text_area(area: Rect, snapshot: &TextViewSnapshot) -> Rect {
        let [_, text] = Layout::horizontal([
            Constraint::Length(Self::gutter_width(snapshot)),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(area);
        text
    }

    pub fn cursor_position(&self, area: Rect) -> Option<Position> {
        match self.flow {
            TextFlow::SingleLine => self.cursor.and_then(|cursor| {
                let column = u16::try_from(cursor.column).ok()?;
                let row = u16::try_from(cursor.row).ok()?;
                (column < area.width && row < area.height)
                    .then_some(Position::new(area.x + column, area.y + row))
            }),
            TextFlow::Wrapped => self.wrapped_rows(area.width).cursor_position(area),
        }
    }

    fn gutter_width(snapshot: &TextViewSnapshot) -> u16 {
        let digits = snapshot.line_count.max(1).to_string().len();
        u16::try_from(digits + 1).unwrap_or(u16::MAX)
    }

    fn line(&self, snapshot: &LineSnapshot) -> Line<'static> {
        let base = self.theme.ui().base;
        let text = &snapshot.text;
        let trailing_offset = snapshot
            .highlights
            .last()
            .map_or(0, |highlight| highlight.end_byte);
        let spans = std::iter::once(0)
            .chain(
                snapshot
                    .highlights
                    .iter()
                    .map(|highlight| highlight.end_byte),
            )
            .zip(&snapshot.highlights)
            .flat_map(|(offset, highlight)| {
                [
                    (highlight.start_byte > offset).then(|| {
                        Span::styled(text[offset..highlight.start_byte].to_string(), base)
                    }),
                    Some(Span::styled(
                        text[highlight.start_byte..highlight.end_byte].to_string(),
                        base.patch(self.theme.highlight(&highlight.highlights)),
                    )),
                ]
            })
            .flatten()
            .chain(
                (trailing_offset < text.len())
                    .then(|| Span::styled(text[trailing_offset..].to_string(), base)),
            )
            .collect::<Vec<_>>();
        Line::from(spans)
    }

    fn line_number(&self, index: usize, change: Option<LineChange>) -> Line<'static> {
        let number_width = usize::from(Self::gutter_width(self.snapshot)).saturating_sub(1);
        let base = self.theme.ui().base.patch(self.theme.ui().muted);
        let marker = match change {
            Some(LineChange::Added) => ("│", self.theme.ui().diff_add),
            Some(LineChange::Deleted) => ("│", self.theme.ui().diff_remove),
            Some(LineChange::Modified) => ("│", self.theme.ui().diff_hunk),
            None => (" ", base),
        };
        Line::from(vec![
            Span::styled(
                format!("{:>number_width$}", self.snapshot.first_line + index + 1),
                base,
            ),
            Span::styled(marker.0, base.patch(marker.1)),
        ])
    }

    fn continuation_gutter(&self) -> Line<'static> {
        let width = usize::from(Self::gutter_width(self.snapshot));
        Line::raw(" ".repeat(width))
    }
}

#[derive(Debug)]
struct WrappedRow {
    gutter: Option<Line<'static>>,
    content: Line<'static>,
    logical_line: usize,
    start: usize,
    end: usize,
}

#[derive(Debug)]
struct WrappedLineInput<'a> {
    logical_line: usize,
    snapshot_line: &'a LineSnapshot,
    layout: TextLayout,
    styled: Line<'static>,
}

#[derive(Debug)]
struct WrappedView {
    rows: Vec<WrappedRow>,
    cursor: Option<TextPoint>,
}

impl WrappedView {
    fn empty() -> Self {
        Self {
            rows: Vec::new(),
            cursor: None,
        }
    }

    fn push_line(self, view: &TextView, input: WrappedLineInput) -> Self {
        let Self { rows, cursor } = self;
        let (rows, cursor) = input.layout.lines().iter().enumerate().fold(
            (rows, cursor),
            |(mut rows, cursor), (segment, display)| {
                let cursor = cursor_at_segment(cursor, &input.layout, segment, rows.len());
                rows.push(row_for_display(view, &input, segment, display));
                (rows, cursor)
            },
        );
        Self { rows, cursor }
    }

    fn vertical_offset(&self, height: u16) -> usize {
        self.cursor
            .map(|cursor| cursor.row)
            .unwrap_or(0)
            .saturating_sub((height as usize).saturating_sub(1))
    }

    fn cursor_position(&self, area: Rect) -> Option<Position> {
        if area.width == 0 || area.height == 0 {
            return None;
        }
        let cursor = self.cursor?;
        let offset = self.vertical_offset(area.height);
        if cursor.row < offset || cursor.row >= offset + area.height as usize {
            return None;
        }
        Some(Position::new(
            area.x + (cursor.column.min(area.width.saturating_sub(1) as usize) as u16),
            area.y + (cursor.row - offset) as u16,
        ))
    }
}

impl TextView<'_> {
    fn wrapped_rows(&self, width: u16) -> WrappedView {
        self.snapshot.lines.iter().enumerate().fold(
            WrappedView::empty(),
            |view, (logical_line, snapshot_line)| {
                view.push_line(self, self.line_input(logical_line, snapshot_line, width))
            },
        )
    }

    fn line_input<'a>(
        &'a self,
        logical_line: usize,
        snapshot_line: &'a LineSnapshot,
        width: u16,
    ) -> WrappedLineInput<'a> {
        let cursor_byte = self.cursor_byte(snapshot_line, logical_line);
        WrappedLineInput {
            logical_line,
            snapshot_line,
            layout: TextLayout::new(&snapshot_line.text, cursor_byte, width, TextFlow::Wrapped),
            styled: self.line(snapshot_line),
        }
    }

    fn cursor_byte(&self, snapshot_line: &LineSnapshot, logical_line: usize) -> usize {
        self.cursor
            .filter(|cursor| cursor.row == logical_line)
            .map(|cursor| {
                byte_at_display_width(
                    &snapshot_line.text,
                    self.snapshot.horizontal_offset + cursor.column,
                )
            })
            .unwrap_or(snapshot_line.text.len())
    }
}

fn row_for_display(
    view: &TextView,
    input: &WrappedLineInput,
    segment: usize,
    display: &DisplayLine,
) -> WrappedRow {
    WrappedRow {
        gutter: (segment == 0)
            .then(|| view.line_number(input.logical_line, input.snapshot_line.change)),
        content: slice_line(&input.styled, display),
        logical_line: input.logical_line,
        start: display.start,
        end: display.end,
    }
}

fn cursor_at_segment(
    current: Option<TextPoint>,
    layout: &TextLayout,
    segment: usize,
    row: usize,
) -> Option<TextPoint> {
    current.or_else(|| {
        (layout.cursor().row == segment).then_some(TextPoint {
            row,
            column: layout.cursor().column,
        })
    })
}

impl TextView<'_> {
    fn render_single_line(self, area: Rect, buf: &mut Buffer) {
        let [gutter_area, text_area] = Layout::horizontal([
            Constraint::Length(Self::gutter_width(self.snapshot)),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(area);
        let horizontal_offset = u16::try_from(self.snapshot.horizontal_offset).unwrap_or(u16::MAX);
        let line_numbers = self
            .snapshot
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| self.line_number(index, line.change))
            .collect::<Vec<_>>();
        let lines = self
            .snapshot
            .lines
            .iter()
            .map(|line| self.line(line))
            .collect::<Vec<_>>();

        Paragraph::new(line_numbers)
            .style(self.theme.ui().base.patch(self.theme.ui().muted))
            .render(gutter_area, buf);
        Paragraph::new(lines)
            .style(self.theme.ui().base)
            .scroll((0, horizontal_offset))
            .render(text_area, buf);
        let rows = self
            .snapshot
            .lines
            .iter()
            .enumerate()
            .map(|(screen_row, _)| EditorVisualRow {
                logical_line: screen_row,
                screen_row,
            })
            .collect::<Vec<_>>();
        self.render_decorations(area, buf, &rows, EditorDecorationPhase::BeforeSelection);
        self.apply_selection_single_line(text_area, buf);
        self.render_decorations(area, buf, &rows, EditorDecorationPhase::AfterSelection);
    }

    fn render_wrapped(self, area: Rect, buf: &mut Buffer) {
        let [gutter_area, text_area] = Layout::horizontal([
            Constraint::Length(Self::gutter_width(self.snapshot)),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(area);
        let view = self.wrapped_rows(text_area.width);
        let offset = view.vertical_offset(text_area.height);
        let visible = view
            .rows
            .iter()
            .skip(offset)
            .take(text_area.height as usize);
        let line_numbers = visible
            .clone()
            .map(|row| {
                row.gutter
                    .clone()
                    .unwrap_or_else(|| self.continuation_gutter())
            })
            .collect::<Vec<_>>();
        let lines = visible.map(|row| row.content.clone()).collect::<Vec<_>>();
        Paragraph::new(line_numbers)
            .style(self.theme.ui().base.patch(self.theme.ui().muted))
            .render(gutter_area, buf);
        Paragraph::new(lines)
            .style(self.theme.ui().base)
            .render(text_area, buf);
        let rows = view
            .rows
            .iter()
            .skip(offset)
            .take(text_area.height as usize)
            .enumerate()
            .map(|(screen_row, row)| EditorVisualRow {
                logical_line: row.logical_line,
                screen_row,
            })
            .collect::<Vec<_>>();
        self.render_decorations(area, buf, &rows, EditorDecorationPhase::BeforeSelection);
        view.rows
            .iter()
            .skip(offset)
            .take(text_area.height as usize)
            .enumerate()
            .for_each(|(row_index, row)| {
                self.apply_selection_wrapped(text_area, buf, row_index, row);
            });
        self.render_decorations(area, buf, &rows, EditorDecorationPhase::AfterSelection);
    }

    fn render_decorations(
        &self,
        area: Rect,
        buf: &mut Buffer,
        rows: &[EditorVisualRow],
        phase: EditorDecorationPhase,
    ) {
        let context = EditorDecorationContext {
            snapshot: self.snapshot,
            cursor: self.cursor,
            rows,
            styles: self.theme.ui(),
        };
        self.decorations
            .iter()
            .filter(|decoration| decoration.phase == phase)
            .for_each(|decoration| (decoration.render)(context, area, buf));
    }

    fn apply_selection_single_line(&self, text_area: Rect, buf: &mut Buffer) {
        self.snapshot
            .lines
            .iter()
            .enumerate()
            .filter_map(|(row, line)| {
                let range = line.selection.as_ref()?;
                let start = UnicodeWidthStr::width(&line.text[..range.start]);
                let end = UnicodeWidthStr::width(&line.text[..range.end]);
                let offset = self.snapshot.horizontal_offset;
                let viewport_start = offset;
                let viewport_end = offset + usize::from(text_area.width);
                let start = start.max(viewport_start);
                let end = end.min(viewport_end);
                if end <= start {
                    return None;
                }
                let row = u16::try_from(row).ok()?;
                if row >= text_area.height {
                    return None;
                }
                Some(Rect::new(
                    text_area.x + u16::try_from(start - offset).ok()?,
                    text_area.y + row,
                    u16::try_from(end - start).ok()?,
                    1,
                ))
            })
            .for_each(|area| buf.set_style(area, self.theme.ui().selection));
    }

    fn apply_selection_wrapped(
        &self,
        text_area: Rect,
        buf: &mut Buffer,
        row_index: usize,
        row: &WrappedRow,
    ) {
        let snapshot = &self.snapshot.lines[row.logical_line];
        let Some(selection) = &snapshot.selection else {
            return;
        };
        let start = selection.start.max(row.start);
        let end = selection.end.min(row.end);
        if start >= end {
            return;
        }
        let start_width = UnicodeWidthStr::width(&snapshot.text[row.start..start]);
        let end_width = UnicodeWidthStr::width(&snapshot.text[row.start..end]);
        if end_width <= start_width {
            return;
        }
        let area = Rect::new(
            text_area.x + u16::try_from(start_width).unwrap_or(u16::MAX),
            text_area.y + u16::try_from(row_index).unwrap_or(u16::MAX),
            u16::try_from(end_width - start_width).unwrap_or(u16::MAX),
            1,
        );
        buf.set_style(area, self.theme.ui().selection);
    }
}

impl Widget for TextView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        match self.flow {
            TextFlow::SingleLine => self.render_single_line(area, buf),
            TextFlow::Wrapped => self.render_wrapped(area, buf),
        }
    }
}

fn slice_line(line: &Line<'static>, display: &DisplayLine) -> Line<'static> {
    let mut offset = 0;
    let spans = line
        .spans
        .iter()
        .filter_map(|span| {
            let span_start = offset;
            let span_end = span_start + span.content.len();
            offset = span_end;
            let start = span_start.max(display.start);
            let end = span_end.min(display.end);
            (start < end).then(|| {
                Span::styled(
                    span.content[start - span_start..end - span_start].to_string(),
                    span.style,
                )
            })
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use editor::HighlightSpan;
    use helix_view::theme::Loader as HelixThemeLoader;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier, Style},
    };

    use super::*;

    fn test_cursor_line_decoration() -> EditorDecorationContribution {
        fn render(context: EditorDecorationContext<'_>, area: Rect, buffer: &mut Buffer) {
            let Some(cursor) = context.cursor else {
                return;
            };
            context
                .rows
                .iter()
                .filter(|row| row.logical_line == cursor.row)
                .for_each(|row| {
                    if let Ok(screen_row) = u16::try_from(row.screen_row)
                        && screen_row < area.height
                    {
                        buffer.set_style(
                            Rect::new(area.x, area.y + screen_row, area.width, 1),
                            Style::new().bg(Color::Blue),
                        );
                    }
                });
        }
        EditorDecorationContribution::new(
            "test_cursor_line",
            EditorDecorationPhase::BeforeSelection,
            render,
        )
    }

    #[test]
    fn applies_the_shared_theme_to_exact_syntax_spans() {
        let mut snapshot = TextViewSnapshot::plain(vec!["fn main".to_string()]);
        let recognized = HelixThemeLoader::new(&[]).default();
        let keyword = recognized.find_highlight_exact("keyword").unwrap();
        snapshot.lines[0].highlights.push(HighlightSpan {
            start_byte: 0,
            end_byte: 2,
            highlights: vec![keyword],
        });
        let theme = Theme::default();
        let view = TextView::new(&snapshot, &theme);

        let line = view.line(&snapshot.lines[0]);
        assert_eq!(line.spans[0].content.as_ref(), "fn");
        assert_eq!(line.spans[0].style.fg, Some(Color::Magenta));
        assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(line.spans[1].content.as_ref(), " main");
    }

    #[test]
    fn renders_diff_markers_with_theme_colors() {
        let mut snapshot = TextViewSnapshot::plain(vec![
            "added".to_string(),
            "modified".to_string(),
            "deleted".to_string(),
        ]);
        snapshot.lines[0].change = Some(LineChange::Added);
        snapshot.lines[1].change = Some(LineChange::Modified);
        snapshot.lines[2].change = Some(LineChange::Deleted);
        let theme = Theme::default();
        let backend = TestBackend::new(12, 3);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(TextView::new(&snapshot, &theme), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("1│"));
        assert!(content.contains("2│"));
        assert!(content.contains("3│"));
        let cells = terminal.backend().buffer().content();
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "│" && cell.fg == Color::Green)
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "│" && cell.fg == Color::LightBlue)
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "│" && cell.fg == Color::Red)
        );
    }

    #[test]
    fn applies_selection_over_unicode_cells_without_replacing_text() {
        let mut snapshot = TextViewSnapshot::plain(vec!["a界b".to_string()]);
        snapshot.lines[0].selection = Some(1..4);
        let backend = TestBackend::new(6, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                frame.render_widget(TextView::new(&snapshot, &Theme::default()), frame.area())
            })
            .unwrap();

        let cells = terminal.backend().buffer().content();
        assert!(
            cells.iter().any(|cell| {
                cell.symbol() == "界" && cell.modifier.contains(Modifier::REVERSED)
            })
        );
        assert!(
            cells.iter().any(|cell| {
                cell.symbol() == "a" && !cell.modifier.contains(Modifier::REVERSED)
            })
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "b" && !cell.modifier.contains(Modifier::REVERSED))
        );
    }

    #[test]
    fn cursor_line_paints_only_the_active_row_including_the_gutter() {
        let snapshot = TextViewSnapshot::plain(vec!["zero".to_string(), "one".to_string()]);
        let theme = Theme::default();
        let decorations = [test_cursor_line_decoration()];
        let view = TextView::new(&snapshot, &theme)
            .with_cursor(Some(editor::CursorPosition { row: 1, column: 0 }))
            .with_decorations(&decorations);
        let mut terminal = Terminal::new(TestBackend::new(12, 2)).unwrap();
        terminal
            .draw(|frame| view.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        let rows = terminal
            .backend()
            .buffer()
            .content()
            .chunks(12)
            .collect::<Vec<_>>();
        assert!(rows[0].iter().all(|cell| cell.bg != Color::Blue));
        assert!(rows[1].iter().all(|cell| cell.bg == Color::Blue));
    }

    #[test]
    fn cursor_line_covers_wrapped_rows_and_yields_to_selection() {
        let snapshot = TextViewSnapshot::plain(vec!["abcdef".to_string(), "next".to_string()]);
        let theme = Theme::default();
        let decorations = [test_cursor_line_decoration()];
        let view = TextView::with_flow(&snapshot, &theme, TextFlow::Wrapped)
            .with_cursor(Some(editor::CursorPosition { row: 0, column: 4 }))
            .with_decorations(&decorations);
        let mut wrapped = Terminal::new(TestBackend::new(7, 2)).unwrap();
        wrapped
            .draw(|frame| view.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert!(
            wrapped
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.bg == Color::Blue)
        );

        let mut selected_snapshot = TextViewSnapshot::plain(vec!["abc".to_string()]);
        selected_snapshot.lines[0].selection = Some(0..1);
        let theme = Theme::from_name("tokyonight").unwrap();
        let selected = TextView::new(&selected_snapshot, &theme)
            .with_cursor(Some(editor::CursorPosition { row: 0, column: 0 }))
            .with_decorations(&decorations);
        let mut terminal = Terminal::new(TestBackend::new(6, 1)).unwrap();
        terminal
            .draw(|frame| selected.render(frame.area(), frame.buffer_mut()))
            .unwrap();
        assert_eq!(
            terminal.backend().buffer()[(2, 0)].bg,
            theme.ui().selection.bg.unwrap()
        );
        assert_eq!(terminal.backend().buffer()[(3, 0)].bg, Color::Blue);
    }

    #[test]
    fn wrapped_view_preserves_spans_gutters_and_cursor_visibility() {
        let mut snapshot = TextViewSnapshot::plain(vec!["abcdef".to_string(), "next".to_string()]);
        snapshot.first_line = 0;
        snapshot.line_count = 2;
        let cursor = Some(editor::CursorPosition { row: 0, column: 4 });
        let theme = Theme::default();
        let view = TextView::with_flow(&snapshot, &theme, TextFlow::Wrapped).with_cursor(cursor);
        let backend = TestBackend::new(8, 2);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| view.render(frame.area(), frame.buffer_mut()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("1 ab"));
        assert!(content.contains("cd"));
        assert!(view.cursor_position(Rect::new(0, 0, 5, 2)).is_some());
    }
}
