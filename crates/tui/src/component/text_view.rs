use editor::{LineChange, LineSnapshot, TextViewSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

/// Shared read-only document viewport for editor buffers and text previews.
#[derive(Debug, Clone, Copy)]
pub struct TextView<'a> {
    snapshot: &'a TextViewSnapshot,
    theme: &'a Theme,
}

impl<'a> TextView<'a> {
    pub fn new(snapshot: &'a TextViewSnapshot, theme: &'a Theme) -> Self {
        Self { snapshot, theme }
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

    fn gutter_width(snapshot: &TextViewSnapshot) -> u16 {
        let digits = snapshot.line_count.max(1).to_string().len();
        u16::try_from(digits + 1).unwrap_or(u16::MAX)
    }

    fn apply_selection(&self, text_area: Rect, buf: &mut Buffer) {
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
}

impl Widget for TextView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [gutter_area, text_area] = Layout::horizontal([
            Constraint::Length(Self::gutter_width(self.snapshot)),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(area);
        let horizontal_offset = u16::try_from(self.snapshot.horizontal_offset).unwrap_or(u16::MAX);
        let number_width = usize::from(Self::gutter_width(self.snapshot)).saturating_sub(1);
        let line_numbers = self
            .snapshot
            .lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let base = self.theme.ui().base.patch(self.theme.ui().muted);
                let marker = match line.change {
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
            })
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
        self.apply_selection(text_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::HighlightSpan;
    use helix_view::theme::Loader as HelixThemeLoader;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::*;

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
}
