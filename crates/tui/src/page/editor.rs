use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout},
};

use crate::component::{
    editor::Editor as EditorComponent, file_explorer::FileExplorer, tools_panel::ToolsPanel,
};
use crate::data_object::Model;

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let [explorer_area, editor_area, tools_area] = Layout::horizontal([
            Constraint::Length(28),
            Constraint::Fill(1),
            Constraint::Length(32),
        ])
        .flex(Flex::Start)
        .areas(frame.area());

        frame.render_widget(FileExplorer, explorer_area);
        frame.render_widget(EditorComponent::new(&model.edit_mode), editor_area);
        frame.render_widget(ToolsPanel, tools_area);
    }
}
