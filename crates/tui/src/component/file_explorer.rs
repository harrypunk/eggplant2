use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, Paragraph, Widget},
};

use crate::data_object::Workspace;

#[derive(Debug)]
pub struct FileExplorer<'a> {
    workspace: Option<&'a Workspace>,
}

impl<'a> FileExplorer<'a> {
    pub fn new(workspace: Option<&'a Workspace>) -> Self {
        Self { workspace }
    }
}

impl Widget for FileExplorer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let content = self
            .workspace
            .map(|workspace| workspace.root.display().to_string())
            .unwrap_or_else(|| "No folder open".to_string());
        Paragraph::new(content)
            .block(Block::bordered().title("File Explorer"))
            .render(area, buf);
    }
}
