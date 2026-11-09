use editor::{LineSnapshot, TextViewSnapshot};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::theme::Theme;

/// Shared read-only document viewport for editor buffers and text previews.
#[derive(Debug, Clone, Copy)]
pub struct TextView<'a> {
    snapshot: &'a TextViewSnapshot,
    theme: Theme,
}

impl<'a> TextView<'a> {
    pub fn new(snapshot: &'a TextViewSnapshot, theme: Theme) -> Self {
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

    fn line(&self, snapshot: &LineSnapshot) -> Line<'static> {
        let mut spans = Vec::new();
        let mut offset = 0;
        for highlight in &snapshot.highlights {
            if highlight.start_byte > offset {
                spans.push(Span::raw(
                    snapshot.text[offset..highlight.start_byte].to_string(),
                ));
            }
            spans.push(Span::styled(
                snapshot.text[highlight.start_byte..highlight.end_byte].to_string(),
                self.theme.highlight(highlight.kind),
            ));
            offset = highlight.end_byte;
        }
        if offset < snapshot.text.len() {
            spans.push(Span::raw(snapshot.text[offset..].to_string()));
        }
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
            .map(|(index, _)| format!("{:>number_width$} ", self.snapshot.first_line + index + 1))
            .collect::<Vec<_>>()
            .join("\n");
        let lines = self
            .snapshot
            .lines
            .iter()
            .map(|line| self.line(line))
            .collect::<Vec<_>>();

        Paragraph::new(line_numbers).render(gutter_area, buf);
        Paragraph::new(lines)
            .scroll((0, horizontal_offset))
            .render(text_area, buf);
    }
}

#[cfg(test)]
mod tests {
    use editor::{HighlightKind, HighlightSpan};
    use ratatui::style::{Color, Modifier};

    use super::*;

    #[test]
    fn applies_the_shared_theme_to_semantic_spans() {
        let mut snapshot = TextViewSnapshot::plain(vec!["fn main".to_string()]);
        snapshot.lines[0].highlights.push(HighlightSpan {
            start_byte: 0,
            end_byte: 2,
            kind: HighlightKind::Keyword,
        });
        let view = TextView::new(&snapshot, Theme);

        let line = view.line(&snapshot.lines[0]);
        assert_eq!(line.spans[0].content.as_ref(), "fn");
        assert_eq!(line.spans[0].style.fg, Some(Color::Magenta));
        assert!(line.spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(line.spans[1].content.as_ref(), " main");
    }
}
