use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
};

use crate::component::{
    editor::Editor as EditorComponent, file_explorer::FileExplorer, status_bar::StatusBar,
    tools_panel::ToolsPanel, which_key::WhichKey,
};
use crate::data_object::{FocusState, FocusTarget, Model, Visibility};

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = layout_areas(
            frame.area(),
            model.explorer_visibility,
            model.tools_visibility,
        );
        let snapshot = model.editor.active_snapshot();
        let editor = EditorComponent::new(
            snapshot,
            focus_for(model.current_focus, FocusTarget::Editor),
        );
        let cursor = editor.cursor_position(areas.editor);

        areas.explorer.into_iter().for_each(|area| {
            frame.render_widget(
                FileExplorer::new(
                    model.workspace.as_ref(),
                    &model.explorer,
                    focus_for(model.current_focus, FocusTarget::FileExplorer),
                ),
                area,
            );
        });
        frame.render_widget(editor, areas.editor);
        areas.tools.into_iter().for_each(|area| {
            frame.render_widget(ToolsPanel, area);
        });
        frame.render_widget(
            StatusBar::new(model.editor.mode(), model.status.as_ref()),
            areas.status,
        );
        match model.leader.items() {
            [] => {}
            _ => {
                frame.render_widget(WhichKey::new(&model.leader), which_key_area(frame.area()));
            }
        }

        cursor
            .into_iter()
            .filter(|_| model.leader.items().is_empty())
            .for_each(|cursor| frame.set_cursor_position(cursor));
    }
}

struct PageLayout {
    explorer: Option<Rect>,
    editor: Rect,
    tools: Option<Rect>,
    status: Rect,
}

fn layout_areas(area: Rect, explorer: Visibility, tools: Visibility) -> PageLayout {
    let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
        .flex(Flex::Start)
        .areas(area);
    let [explorer_area, editor, tools_area] = Layout::horizontal([
        panel_constraint(explorer, 28),
        Constraint::Fill(1),
        panel_constraint(tools, 32),
    ])
    .flex(Flex::Start)
    .areas(main);

    PageLayout {
        explorer: panel_region(explorer, explorer_area),
        editor,
        tools: panel_region(tools, tools_area),
        status,
    }
}

fn panel_constraint(visibility: Visibility, width: u16) -> Constraint {
    match visibility {
        Visibility::Displayed => Constraint::Length(width),
        Visibility::Hidden => Constraint::Length(0),
    }
}

fn panel_region(visibility: Visibility, area: Rect) -> Option<Rect> {
    match visibility {
        Visibility::Displayed => Some(area),
        Visibility::Hidden => None,
    }
}

fn focus_for(current: FocusTarget, component: FocusTarget) -> FocusState {
    match current == component {
        true => FocusState::Focused,
        false => FocusState::Unfocused,
    }
}

fn which_key_area(area: Rect) -> Rect {
    let [_, popup, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    popup
}

/// The text area used both for Helix viewport sizing and terminal cursor placement.
pub fn editor_content_area(area: Rect, explorer: Visibility, tools: Visibility) -> Rect {
    EditorComponent::content_area(layout_areas(area, explorer, tools).editor)
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
        let content = editor_content_area(
            Rect::new(0, 0, 100, 10),
            Visibility::Displayed,
            Visibility::Hidden,
        );
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
        let areas = layout_areas(
            Rect::new(0, 0, 100, 10),
            Visibility::Displayed,
            Visibility::Hidden,
        );
        assert_eq!(areas.editor.y, 0);
        assert!(areas.tools.is_none());
        assert_eq!(areas.status.y, 9);
        let hidden = layout_areas(
            Rect::new(0, 0, 100, 10),
            Visibility::Hidden,
            Visibility::Hidden,
        );
        assert!(hidden.explorer.is_none());
        assert_eq!(hidden.editor.width, 100);
    }

    #[test]
    fn shows_which_key_menu_and_hides_cursor() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        crate::test_utils::create_buffer(&mut model, "hello");
        model
            .leader
            .handle_key(crossterm::event::KeyCode::Char(' '));
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Leader"));
        assert!(content.contains("q q quit"));
        assert!(!terminal.backend().cursor_visible());
    }
}
