use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Widget},
};

#[derive(Debug, Default)]
pub struct FileExplorer;

impl Widget for FileExplorer {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Block::bordered().title("File Explorer").render(area, buf);
    }
}
