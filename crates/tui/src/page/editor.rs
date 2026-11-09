use editor::EditorSnapshot;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect, Size},
};

use crate::component::{
    editor::{Editor as EditorComponent, EditorBuffer},
    file_explorer::FileExplorer,
    file_picker::FilePicker,
    key_hints::KeyHints,
    status_bar::StatusBar,
    tools_panel::ToolsPanel,
};
use crate::data_object::{FocusState, FocusTarget, Model, Visibility};

#[derive(Debug, Default)]
pub struct Editor;

impl Editor {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = layout_areas(
            frame.area(),
            model.explorer_visibility,
            model.tools.visibility,
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
            frame.render_widget(
                ToolsPanel::new(
                    model.tools.active_tool,
                    focus_for(model.current_focus, FocusTarget::ToolsPanel),
                ),
                area,
            );
        });
        frame.render_widget(
            StatusBar::new(model.editor.mode(), model.status.as_ref()),
            areas.status,
        );
        match model.command_menu.items() {
            [] => {}
            _ => {
                let hints = KeyHints::new(&model.command_menu);
                let area = key_hints_area(frame.area(), hints.size());
                frame.render_widget(hints, area);
            }
        }
        match model.file_picker.visibility {
            Visibility::Displayed => {
                let area = file_picker_area(areas.main);
                frame.render_widget(FilePicker::new(&model.file_picker), area);
            }
            Visibility::Hidden => {}
        }

        cursor
            .into_iter()
            .filter(|_| {
                model.command_menu.items().is_empty()
                    && model.file_picker.visibility == Visibility::Hidden
            })
            .for_each(|cursor| frame.set_cursor_position(cursor));
    }
}

struct PageLayout {
    main: Rect,
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
        main,
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

fn key_hints_area(area: Rect, size: Size) -> Rect {
    let width = size.width.min(area.width);
    let height = size.height.min(area.height.saturating_sub(1));
    let [_, popup_band, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Length(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    let [_, popup] = Layout::horizontal([Constraint::Fill(1), Constraint::Length(width)])
        .flex(Flex::Start)
        .areas(popup_band);
    popup
}

fn file_picker_area(area: Rect) -> Rect {
    let width = area.width.saturating_sub(2).min(160);
    let height = area.height.saturating_sub(2).min(36);
    let [_, popup_band, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .flex(Flex::Start)
    .areas(area);
    let [_, popup, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .flex(Flex::Start)
    .areas(popup_band);
    popup
}

/// The text area used both for Helix viewport sizing and terminal cursor placement.
pub fn editor_content_area(
    area: Rect,
    explorer: Visibility,
    tools: Visibility,
    snapshot: Option<&EditorSnapshot>,
) -> Rect {
    let content = EditorComponent::content_area(layout_areas(area, explorer, tools).editor);
    match snapshot {
        Some(snapshot) => EditorBuffer::text_area(content, snapshot),
        None => content,
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::test_utils::create_buffer;

    #[test]
    fn places_and_hides_cursor_based_on_editor_focus() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        let content = editor_content_area(
            Rect::new(0, 0, 100, 10),
            Visibility::Displayed,
            Visibility::Hidden,
            model.editor.active_snapshot().as_ref(),
        );
        model
            .editor
            .resize_active_view(content.width, content.height);
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((30, 1));

        model.current_focus = FocusTarget::FileExplorer;
        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }

    #[test]
    fn renders_top_tabs_and_bottom_status_without_editor_box() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
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
    fn shows_centered_file_picker_and_hides_editor_cursor() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        model.file_picker.visibility = Visibility::Displayed;
        let page = Editor;

        terminal.draw(|frame| page.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Files"));
        assert!(!content.contains("Search"));
        assert!(!content.contains("Preview"));
        assert!(!terminal.backend().cursor_visible());
        let area = file_picker_area(Rect::new(0, 0, 100, 9));
        assert_eq!(area.width, 98);
        assert_eq!(area.height, 7);
        assert_eq!(area.x, 1);
        assert_eq!(area.y, 1);
        let large = file_picker_area(Rect::new(0, 0, 200, 50));
        assert_eq!(large.width, 160);
        assert_eq!(large.height, 36);
    }

    #[test]
    fn clamps_key_hints_to_the_available_area() {
        let area = key_hints_area(
            Rect::new(0, 0, 8, 3),
            Size {
                width: 16,
                height: 5,
            },
        );

        assert_eq!(area.width, 8);
        assert_eq!(area.height, 2);
        assert_eq!(area.x + area.width, 8);
        assert_eq!(area.y + area.height, 2);
    }

    #[test]
    fn shows_bottom_right_key_hints_and_hides_cursor() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        model
            .command_menu
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
        assert!(content.contains("q quit"));
        assert!(!terminal.backend().cursor_visible());

        let hints = KeyHints::new(&model.command_menu);
        let area = key_hints_area(Rect::new(0, 0, 100, 10), hints.size());
        assert_eq!(area.x + area.width, 100);
        assert_eq!(area.y + area.height, 9);
        assert_eq!(area.height, 6);
        assert!(area.width < 100);
    }
}
