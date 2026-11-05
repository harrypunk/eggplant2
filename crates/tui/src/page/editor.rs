use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    widgets::Block,
};

use crate::component::{
    editor::Editor as EditorComponent, file_explorer::FileExplorer, tools_panel::ToolsPanel,
};
use crate::data_object::{FocusTarget, Model};

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let [explorer_area, editor_area, tools_area] = layout_areas(frame.area());
        let snapshot = model.editor.active_snapshot();
        let cursor = snapshot.as_ref().and_then(|snapshot| snapshot.cursor);

        frame.render_widget(FileExplorer::new(model.workspace.as_ref()), explorer_area);
        frame.render_widget(EditorComponent::new(snapshot), editor_area);
        frame.render_widget(ToolsPanel, tools_area);

        if model.current_focus == FocusTarget::Editor
            && let Some(cursor) = cursor
        {
            let content_area = editor_content_area(frame.area());
            let column = u16::try_from(cursor.column).unwrap_or(u16::MAX);
            let row = u16::try_from(cursor.row).unwrap_or(u16::MAX);
            frame.set_cursor_position((
                content_area.x.saturating_add(column),
                content_area.y.saturating_add(row),
            ));
        }
    }
}

fn layout_areas(area: Rect) -> [Rect; 3] {
    Layout::horizontal([
        Constraint::Length(28),
        Constraint::Fill(1),
        Constraint::Length(32),
    ])
    .flex(Flex::Start)
    .areas(area)
}

/// The text area used both for Helix viewport sizing and terminal cursor placement.
pub fn editor_content_area(area: Rect) -> Rect {
    Block::bordered().inner(layout_areas(area)[1])
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn places_and_hides_cursor_based_on_editor_focus() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        model.editor.create_buffer("hello");
        let content = editor_content_area(Rect::new(0, 0, 100, 10));
        model
            .editor
            .resize_active_view(content.width, content.height);
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((29, 1));

        model.current_focus = FocusTarget::FileExplorer;
        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }
}
