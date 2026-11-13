use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use super::{popup::Popup, text_view::TextView};
use crate::{
    data_object::{Model, Visibility},
    file_picker::{FileCollectionState, FilePickerState, PreviewState},
    theme::{Theme, UiTheme},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

/// Two-pane file picker: query/results on the left and text preview on the right.
#[derive(Debug, Clone, Copy)]
pub struct FilePicker<'a> {
    state: &'a FilePickerState,
    theme: &'a Theme,
}

impl<'a> FilePicker<'a> {
    pub fn new(state: &'a FilePickerState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }
}

impl Widget for FilePicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(
            "Files",
            FilePickerContent {
                state: self.state,
                theme: self.theme,
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

struct FilePickerContent<'a> {
    state: &'a FilePickerState,
    theme: &'a Theme,
}

pub fn overlay(model: &Model) -> Option<OverlayContribution> {
    match model.file_picker.visibility {
        Visibility::Displayed => Some(OverlayContribution::new(
            "file_picker",
            OverlayKind::Modal,
            OverlayPlacement::MainPanel {
                max_width: 160,
                max_height: 36,
                margin: 2,
            },
            render_overlay,
        )),
        Visibility::Hidden => None,
    }
}

fn render_overlay(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(FilePicker::new(&model.file_picker, &model.theme), area);
}

impl Widget for FilePickerContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let [left, preview_area] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(area);
        let [input_area, results_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(left);

        let surface = self.theme.ui().popup_surface();
        let border = surface.patch(self.theme.ui().border);
        Paragraph::new(self.state.query.as_str())
            .style(surface)
            .block(Block::bordered().style(surface).border_style(border))
            .render(input_area, buf);
        Paragraph::new(result_lines(self.state, self.theme.ui()))
            .style(surface)
            .render(results_area, buf);
        let preview_block = Block::bordered().style(surface).border_style(border);
        let preview_inner = preview_block.inner(preview_area);
        preview_block.render(preview_area, buf);
        match &self.state.preview {
            PreviewState::Text(snapshot) => {
                TextView::new(snapshot, self.theme).render(preview_inner, buf)
            }
            preview => Paragraph::new(preview_lines(preview))
                .style(surface)
                .render(preview_inner, buf),
        }
    }
}

fn result_lines(state: &FilePickerState, styles: &UiTheme) -> Vec<Line<'static>> {
    if state.results.is_empty() {
        let text = match state.collection {
            FileCollectionState::Loading => "Loading files…",
            FileCollectionState::Filtering => "Filtering files…",
            FileCollectionState::Ready => "No matches",
            FileCollectionState::Failed => "File scan failed",
        };
        return vec![Line::from(text)];
    }

    state
        .results
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let base = styles.popup_surface();
            let style = match state.selected == Some(index) {
                true => base.patch(styles.selection),
                false => base,
            };
            Line::styled(entry.display.clone(), style)
        })
        .collect()
}

fn preview_lines(preview: &PreviewState) -> Vec<Line<'static>> {
    match preview {
        PreviewState::Empty => vec![Line::from("No file selected")],
        PreviewState::Loading => vec![Line::from("Loading preview…")],
        PreviewState::Text(_) => Vec::new(),
        PreviewState::Unavailable(error) => vec![Line::from(error.clone())],
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use editor::TextViewSnapshot;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::file_picker::FileEntry;

    #[test]
    fn empty_results_show_filtering_instead_of_a_no_match_flash() {
        let mut state = FilePickerState::default();
        state.collection = FileCollectionState::Filtering;

        let theme = Theme::default();
        let lines = result_lines(&state, theme.ui());
        assert_eq!(lines[0].spans[0].content.as_ref(), "Filtering files…");
    }

    #[test]
    fn renders_query_results_and_text_preview() {
        let mut state = FilePickerState::default();
        state.query = "main".to_string();
        state.collection = FileCollectionState::Ready;
        state.results = vec![FileEntry {
            path: PathBuf::from("src/main.rs"),
            display: "src/main.rs".to_string(),
        }];
        state.selected = Some(0);
        state.preview =
            PreviewState::Text(TextViewSnapshot::plain(vec!["fn main() {}".to_string()]));
        let theme = Theme::default();
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(FilePicker::new(&state, &theme), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Files"));
        assert!(!content.contains("Search"));
        assert!(content.contains("main"));
        assert!(content.contains("src/main.rs"));
        assert!(!content.contains("Preview"));
        assert!(content.contains("fn main() {}"));
    }
}
