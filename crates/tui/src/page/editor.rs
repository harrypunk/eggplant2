use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
};

use crate::component::{
    editor::Editor as EditorComponent, file_explorer::FileExplorer, status_bar::StatusBar,
    tools_panel::ToolsPanel,
};
use crate::data_object::{FocusTarget, Model};

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = layout_areas(frame.area());
        let snapshot = model.editor.active_snapshot();
        let editor = EditorComponent::new(snapshot, model.current_focus == FocusTarget::Editor);
        let cursor = editor.cursor_position(areas.editor);

        frame.render_widget(
            FileExplorer::new(
                model.workspace.as_ref(),
                &model.explorer,
                model.current_focus == FocusTarget::FileExplorer,
            ),
            areas.explorer,
        );
        frame.render_widget(editor, areas.editor);
        if let Some(tools) = areas.tools {
            frame.render_widget(ToolsPanel, tools);
        }
        frame.render_widget(
            StatusBar::new(model.editor.mode(), model.status.as_ref()),
            areas.status,
        );

        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
    }
}

struct PageLayout {
    explorer: Rect,
    editor: Rect,
    tools: Option<Rect>,
    status: Rect,
}

fn layout_areas(area: Rect) -> PageLayout {
    const SHOW_TOOLS: bool = false;

    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
        .flex(Flex::Start)
        .areas(area);
    let [explorer, editor, tools] = Layout::horizontal([
        Constraint::Length(28),
        Constraint::Fill(1),
        Constraint::Length(if SHOW_TOOLS { 32 } else { 0 }),
    ])
    .flex(Flex::Start)
    .areas(main);

    PageLayout {
        explorer,
        editor,
        tools: SHOW_TOOLS.then_some(tools),
        status,
    }
}

/// The text area used both for Helix viewport sizing and terminal cursor placement.
pub fn editor_content_area(area: Rect) -> Rect {
    EditorComponent::content_area(layout_areas(area).editor)
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
        crate::test_utils::create_buffer(&mut model, "hello");
        let content = editor_content_area(Rect::new(0, 0, 100, 10));
        model
            .editor
            .resize_active_view(content.width, content.height);
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((28, 1));

        model.current_focus = FocusTarget::FileExplorer;
        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }

    #[test]
    fn renders_top_tabs_and_bottom_status_without_editor_box() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "hello");
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("NORMAL"));
        assert!(content.contains("[scratch]"));
        assert!(!content.contains("Tools"));
        assert!(!content.contains("Editor"));
        let areas = layout_areas(Rect::new(0, 0, 100, 10));
        assert_eq!(areas.editor.y, 0);
        assert!(areas.tools.is_none());
        assert_eq!(areas.status.y, 9);
    }
}
