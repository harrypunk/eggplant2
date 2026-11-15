use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

use crate::{
    component::{
        picker,
        popup::{self, Popup},
        search_highlight::{highlighted_spans, match_highlight, style_preview},
        text_input::TextInput,
        text_view::TextView,
    },
    data_object::{Model, Visibility},
    theme::{Theme, UiTheme},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{ProjectGrepState, SearchStatus};

#[derive(Debug, Clone, Copy)]
pub struct ProjectGrep<'a> {
    state: &'a ProjectGrepState,
    theme: &'a Theme,
}

impl<'a> ProjectGrep<'a> {
    pub fn new(state: &'a ProjectGrepState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        let input = picker::content_areas(popup::content_area(area)).input;
        TextInput::new(&self.state.query, self.theme.ui()).cursor_position(input)
    }
}

impl Widget for ProjectGrep<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(
            "Search project",
            ProjectGrepContent {
                state: self.state,
                theme: self.theme,
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

struct ProjectGrepContent<'a> {
    state: &'a ProjectGrepState,
    theme: &'a Theme,
}

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    match model.project_grep.visibility {
        Visibility::Displayed => Some(
            OverlayContribution::new(
                "project_grep",
                OverlayKind::Modal,
                OverlayPlacement::MainPanel {
                    max_width: 160,
                    max_height: 36,
                    margin: 2,
                },
                render_overlay,
            )
            .with_cursor(cursor_overlay),
        ),
        Visibility::Hidden => None,
    }
}

fn render_overlay(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(ProjectGrep::new(&model.project_grep, &model.theme), area);
}

fn cursor_overlay(model: &Model, area: Rect) -> Option<Position> {
    ProjectGrep::new(&model.project_grep, &model.theme).cursor_position(area)
}

impl Widget for ProjectGrepContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let areas = picker::content_areas(area);
        let surface = self.theme.ui().popup_surface();
        let border = surface.patch(self.theme.ui().border);
        TextInput::new(&self.state.query, self.theme.ui()).render(areas.input, buf);
        Paragraph::new(result_lines(self.state, self.theme.ui()))
            .style(surface)
            .render(areas.results, buf);

        let preview_block = Block::bordered().style(surface).border_style(border);
        let preview_inner = preview_block.inner(areas.preview);
        preview_block.render(areas.preview, buf);
        match (&self.state.preview, &self.state.preview_error) {
            (Some(snapshot), _) => {
                TextView::new(snapshot, self.theme).render(preview_inner, buf);
                let selected_path = self.state.selected_result().map(|result| &result.path);
                style_preview(
                    snapshot,
                    self.state.selected_result().map(|result| result.line),
                    self.state
                        .results
                        .iter()
                        .filter(move |result| selected_path == Some(&result.path))
                        .map(|result| (result.line, result.match_ranges.as_slice())),
                    preview_inner,
                    buf,
                    self.theme.ui(),
                );
            }
            (None, Some(error)) => Paragraph::new(error.as_str())
                .style(surface)
                .render(preview_inner, buf),
            (None, None) if self.state.preview_loading => Paragraph::new("Loading preview…")
                .style(surface)
                .render(preview_inner, buf),
            (None, None) => Paragraph::new("No match selected")
                .style(surface)
                .render(preview_inner, buf),
        }
    }
}

fn result_lines(state: &ProjectGrepState, styles: &UiTheme) -> Vec<Line<'static>> {
    if state.results.is_empty() {
        let text = match &state.status {
            SearchStatus::Idle => "Type a regular expression".to_string(),
            SearchStatus::Searching => "Searching project…".to_string(),
            SearchStatus::Ready => "No matches".to_string(),
            SearchStatus::Failed(error) => error.clone(),
        };
        return vec![Line::from(text)];
    }

    state
        .results
        .iter()
        .enumerate()
        .map(|(index, result)| {
            let base = styles.popup_surface();
            let style = match state.selected == Some(index) {
                true => base.patch(styles.selection),
                false => base,
            };
            let mut spans = vec![Span::styled(
                format!("{}:{}  ", result.display, result.line + 1),
                style.patch(styles.muted),
            )];
            spans.extend(highlighted_spans(
                &result.text,
                &result.match_ranges,
                style,
                match_highlight(),
            ));
            Line::from(spans)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, preview_document_at};
    use ratatui::{
        Terminal,
        backend::TestBackend,
        style::{Color, Modifier},
    };

    use super::*;
    use crate::project_grep::ProjectGrepResult;

    #[test]
    fn renders_path_results_and_syntax_preview_with_match_blocks() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\nprintln!(\"hello\");").unwrap();
        let mut state = ProjectGrepState::default();
        state.query = "main".to_string();
        state.status = SearchStatus::Ready;
        state.results = vec![ProjectGrepResult {
            path: path.clone(),
            display: "main.rs".to_string(),
            line: 0,
            text: "fn main() {}".to_string(),
            match_ranges: vec![std::ops::Range { start: 3, end: 7 }],
        }];
        state.selected = Some(0);
        state.preview = Some(preview_document_at(&DocumentFactory::new(), &path, 0, 34).unwrap());

        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(ProjectGrep::new(&state, &Theme::default()), frame.area())
            })
            .unwrap();

        let cells = terminal.backend().buffer().content();
        let content = cells.iter().map(|cell| cell.symbol()).collect::<String>();
        assert!(content.contains("Search project"));
        assert!(content.contains("main.rs:1"));
        assert!(content.contains("println"));
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "f" && cell.fg == Color::Magenta)
        );
        assert!(
            cells
                .iter()
                .filter(|cell| {
                    cell.bg == Color::Yellow
                        && cell.fg == Color::Black
                        && cell.modifier.contains(Modifier::BOLD)
                })
                .count()
                >= 8
        );
    }
}
