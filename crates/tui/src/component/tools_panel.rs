use ratatui::{
    buffer::Buffer,
    layout::Rect,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use crate::{data_object::FocusState, theme::UiTheme, tools::ToolKind};

/// Side-panel shell. Tool-specific components will be delegated to from here.
#[derive(Debug)]
pub struct ToolsPanel<'a> {
    active_tool: Option<ToolKind>,
    focus: FocusState,
    styles: &'a UiTheme,
}

impl<'a> ToolsPanel<'a> {
    pub fn new(active_tool: Option<ToolKind>, focus: FocusState, styles: &'a UiTheme) -> Self {
        Self {
            active_tool,
            focus,
            styles,
        }
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

impl Widget for ToolsPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_style = match self.focus {
            FocusState::Focused => self.styles.focused_border,
            FocusState::Unfocused => self.styles.border,
        };
        let block = Block::bordered()
            .border_type(self.border_type())
            .border_style(self.styles.base.patch(border_style))
            .style(self.styles.base)
            .title("Tools");
        Paragraph::new(self.content())
            .style(self.styles.base)
            .block(block)
            .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_empty_and_selected_tool_states() {
        let theme = Theme::default();
        assert_eq!(
            ToolsPanel::new(None, FocusState::Unfocused, theme.ui()).content(),
            "No tool selected"
        );
        assert_eq!(
            ToolsPanel::new(Some(ToolKind::Search), FocusState::Focused, theme.ui()).content(),
            "Search"
        );
    }
}
