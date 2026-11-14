use editor::EditorSnapshot;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
};

use crate::{
    component::editor::{Editor as EditorComponent, EditorBuffer},
    data_object::{FocusState, FocusTarget, Model, Visibility},
    ui::{PanelSlot, StatusSlot, UiRegistry},
};

#[derive(Debug)]
pub struct Editor {
    ui: UiRegistry,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(UiRegistry::builtin())
    }
}

impl Editor {
    pub fn new(ui: UiRegistry) -> Self {
        Self { ui }
    }

    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = layout_areas(
            frame.area(),
            model.explorer.visibility,
            model.tools.visibility,
        );
        let overlays = self.ui.overlays(model);
        let editor_overlays = self.ui.editor_overlays(model);
        let snapshot = model.editor.active_snapshot();
        let editor = EditorComponent::new(
            snapshot.as_ref(),
            focus_for(model.current_focus, FocusTarget::Editor),
            &model.theme,
        );
        let cursor = editor.cursor_position(areas.editor);

        areas.explorer.into_iter().for_each(|area| {
            self.ui
                .render_panel(PanelSlot::Explorer, model, frame, area)
        });
        frame.render_widget(editor, areas.editor);
        if let Some(snapshot) = snapshot.as_ref() {
            let content = EditorComponent::content_area(areas.editor);
            let text_area = EditorBuffer::text_area(content, snapshot);
            editor_overlays.iter().for_each(|overlay| {
                (overlay.render)(model, snapshot, text_area, frame.buffer_mut());
            });
        }
        areas
            .tools
            .into_iter()
            .for_each(|area| self.ui.render_panel(PanelSlot::Tools, model, frame, area));
        self.ui
            .render_status(StatusSlot::Message, model, frame, areas.status);
        overlays.iter().for_each(|overlay| {
            let area = overlay.area(areas.main, frame.area());
            (overlay.render)(model, frame, area);
        });

        let overlay_cursor = overlays.last().and_then(|overlay| {
            overlay
                .cursor
                .and_then(|cursor| cursor(model, overlay.area(areas.main, frame.area())))
        });
        overlay_cursor
            .or_else(|| cursor.filter(|_| overlays.is_empty() && editor_overlays.is_empty()))
            .into_iter()
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
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, test_utils::create_buffer,
    };

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
        let page = Editor::default();

        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((30, 1));

        model.current_focus = FocusTarget::FileExplorer;
        terminal.draw(|frame| page.view(&model, frame)).unwrap();
        assert!(!terminal.backend().cursor_visible());
    }

    #[test]
    fn tiny_layouts_and_resize_never_place_a_cursor_outside_empty_text_areas() {
        let mut model = Model::default();
        create_buffer(&mut model, "a界b🙂c");
        let manager = MessageManager::default();
        let page = Editor::default();
        let sizes = [(0, 0), (1, 1), (2, 1), (3, 2), (6, 3), (10, 4)];

        for (width, height) in sizes {
            manager.update(&mut model, Message::Resize(width, height));
            let text_area = editor_content_area(
                Rect::new(0, 0, width, height),
                model.explorer.visibility,
                model.tools.visibility,
                model.editor.active_snapshot().as_ref(),
            );
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal.draw(|frame| page.view(&model, frame)).unwrap();

            if text_area.width == 0 || text_area.height == 0 {
                assert!(
                    !terminal.backend().cursor_visible(),
                    "cursor must be hidden for an empty text area at {width}x{height}"
                );
            }
        }

        model
            .command_menu
            .handle_key(crossterm::event::KeyCode::Char(' '));
        model.file_picker.visibility = Visibility::Displayed;
        model.theme_picker.open(crate::theme::ThemeName::TokyoNight);
        for (width, height) in sizes {
            manager.update(&mut model, Message::Resize(width, height));
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();

            terminal.draw(|frame| page.view(&model, frame)).unwrap();
            assert!(!terminal.backend().cursor_visible());
        }
    }

    #[test]
    fn renders_top_tabs_and_bottom_status_without_editor_box() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        let page = Editor::default();

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
    fn shows_centered_theme_picker_and_hides_editor_cursor() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        model.theme_picker.open(crate::theme::ThemeName::TokyoNight);
        let page = Editor::default();

        terminal.draw(|frame| page.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Theme"));
        assert!(content.contains("> tokyonight"));
        assert!(content.contains("catppuccin_mocha"));
        assert!(!terminal.backend().cursor_visible());
    }

    #[test]
    fn shows_file_picker_and_places_cursor_after_query() {
        let backend = TestBackend::new(100, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut model = Model::default();
        create_buffer(&mut model, "hello");
        model.file_picker.visibility = Visibility::Displayed;
        model.file_picker.query = "hello".to_string();
        let page = Editor::default();

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
        assert!(terminal.backend().cursor_visible());
        terminal.backend_mut().assert_cursor_position((8, 3));
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
        let page = Editor::default();

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
    }
}
