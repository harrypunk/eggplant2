use ratatui::{Frame, layout::Size, text::Line, widgets::Paragraph};

use super::CodeNavigationState;
use crate::{
    component::{picker, popup::Popup},
    data_object::Model,
    theme::UiTheme,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

pub fn overlay(model: &Model) -> Option<OverlayContribution> {
    let state = &model.code_navigation;
    if state.locations().is_empty() {
        return None;
    }
    let height = u16::try_from(state.locations().len() + 2)
        .unwrap_or(u16::MAX)
        .min(16);
    Some(OverlayContribution::new(
        "code_navigation",
        OverlayKind::Modal,
        OverlayPlacement::Centered(Size::new(64, height)),
        render,
    ))
}

fn render(model: &Model, frame: &mut Frame, area: ratatui::layout::Rect) {
    let state = &model.code_navigation;
    let styles = model.theme.ui();
    let surface = styles.popup_surface();
    let lines = location_lines(
        state,
        model.workspace.as_ref().map(|workspace| workspace.root()),
        styles,
    );
    let lines = picker::visible_lines(lines, state.selected_index(), area.height);
    let content = Paragraph::new(lines).style(surface);
    frame.render_widget(Popup::new("Locations", content, styles), area);
}

fn location_lines(
    state: &CodeNavigationState,
    root: Option<&std::path::Path>,
    styles: &UiTheme,
) -> Vec<Line<'static>> {
    let surface = styles.popup_surface();
    state
        .locations()
        .iter()
        .enumerate()
        .map(|(index, location)| {
            let selected = state.selected_index() == Some(index);
            let marker = if selected { "> " } else { "  " };
            let path = root
                .and_then(|root| location.path.strip_prefix(root).ok())
                .unwrap_or(&location.path);
            let text = format!(
                "{marker}{}:{}:{}",
                path.display(),
                location.start.line + 1,
                location.start.character + 1
            );
            let style = if selected {
                surface.patch(styles.selection)
            } else {
                surface
            };
            Line::styled(text, style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use app::language::{NavigationKind, SourceLocation};
    use editor::{DocumentPosition, EditorSession};
    use ratatui::{Terminal, backend::TestBackend};

    use super::overlay;
    use crate::{code_navigation::NavigationOutcome, data_object::Model};

    #[test]
    fn renders_workspace_relative_locations_and_selection() {
        let mut model = Model {
            workspace: Some(crate::data_object::Workspace::new(
                std::path::PathBuf::from("/workspace"),
            )),
            ..Model::default()
        };
        let buffer =
            EditorSession::new().insert_document(editor::DocumentFactory::new().create(""));
        let request = model.code_navigation.begin(
            NavigationKind::Definition,
            buffer,
            std::path::PathBuf::from("/workspace/main.rs"),
            1,
            DocumentPosition {
                line: 0,
                character: 0,
            },
        );
        let location = SourceLocation {
            path: std::path::PathBuf::from("/workspace/main.rs"),
            start: DocumentPosition {
                line: 1,
                character: 2,
            },
            end: DocumentPosition {
                line: 1,
                character: 4,
            },
        };
        assert_eq!(
            model
                .code_navigation
                .complete(request.id, buffer, 1, Ok(vec![location]),),
            NavigationOutcome::Locations
        );

        let contribution = overlay(&model).unwrap();
        let backend = TestBackend::new(50, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| (contribution.render)(&model, frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Locations"));
        assert!(content.contains("> main.rs:2:3"));
    }
}
