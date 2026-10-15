use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Widget},
};

#[derive(Debug, Default)]
pub struct ToolsPanel;

impl Widget for ToolsPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::bordered().title("Tools").render(area, buf);
    }
}
