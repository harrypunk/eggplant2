use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use crate::{data_object::FocusState, tools::ToolKind};

/// Side-panel shell. Tool-specific components will be delegated to from here.
#[derive(Debug)]
pub struct ToolsPanel {
    active_tool: Option<ToolKind>,
    focus: FocusState,
}

impl ToolsPanel {
    pub fn new(active_tool: Option<ToolKind>, focus: FocusState) -> Self {
        Self { active_tool, focus }
    }

    fn content(&self) -> &'static str {
        match self.active_tool {
            None => "No tool selected",
            Some(ToolKind::Search) => "Search",
            Some(ToolKind::Agent) => "Agent",
            Some(ToolKind::Info) => "Info",
        }
    }

    fn border_type(&self) -> BorderType {
        match self.focus {
            FocusState::Focused => BorderType::Double,
            FocusState::Unfocused => BorderType::Plain,
        }
    }
}

impl Widget for ToolsPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::bordered()
            .border_type(self.border_type())
            .title("Tools");
        Paragraph::new(self.content())
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_empty_and_selected_tool_states() {
        assert_eq!(
            ToolsPanel::new(None, FocusState::Unfocused).content(),
            "No tool selected"
        );
        assert_eq!(
            ToolsPanel::new(Some(ToolKind::Search), FocusState::Focused).content(),
            "Search"
        );
    }
}
