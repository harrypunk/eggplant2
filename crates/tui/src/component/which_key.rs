use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Clear, Paragraph, Widget},
};

use crate::leader::LeaderState;

#[derive(Debug, Clone, Copy)]
pub struct WhichKey<'a> {
    leader: &'a LeaderState,
}

impl<'a> WhichKey<'a> {
    pub fn new(leader: &'a LeaderState) -> Self {
        Self { leader }
    }
}

impl Widget for WhichKey<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let content = self
            .leader
            .items()
            .iter()
            .map(|item| format!("{} {}", item.key, item.description))
            .collect::<Vec<_>>()
            .join("   ");

        Clear.render(area, buf);
        Paragraph::new(content)
            .block(Block::bordered().title("Leader"))
            .render(area, buf);
    }
}
