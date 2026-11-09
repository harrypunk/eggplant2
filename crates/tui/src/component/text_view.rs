use editor::TextViewSnapshot;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    widgets::{Paragraph, Widget},
};

/// Shared read-only document viewport for editor buffers and text previews.
#[derive(Debug, Clone, Copy)]
pub struct TextView<'a> {
    snapshot: &'a TextViewSnapshot,
}

impl<'a> TextView<'a> {
    pub fn new(snapshot: &'a TextViewSnapshot) -> Self {
        Self { snapshot }
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

        Paragraph::new(line_numbers).render(gutter_area, buf);
        Paragraph::new(self.snapshot.lines.join("\n"))
            .scroll((0, horizontal_offset))
            .render(text_area, buf);
    }
}
