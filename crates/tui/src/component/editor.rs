use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Widget},
};

use crate::data_object::EditMode;

#[derive(Debug)]
pub struct Editor<'a> {
    edit_mode: &'a EditMode,
}

impl<'a> Editor<'a> {
    pub fn new(edit_mode: &'a EditMode) -> Self {
        Self { edit_mode }
    }
}

impl Widget for Editor<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mode_text = match self.edit_mode {
            EditMode::Normal => "-- NORMAL --",
            EditMode::Insert => "-- INSERT --",
        };
        Paragraph::new(mode_text)
            .block(Block::bordered().title("Editor"))
            .render(area, buf);
    }
}
