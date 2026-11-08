use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use super::popup::Popup;
use crate::file_picker::{FileCollectionState, FilePickerState, PreviewState};

/// Two-pane file picker: query/results on the left and text preview on the right.
#[derive(Debug, Clone, Copy)]
pub struct FilePicker<'a> {
    state: &'a FilePickerState,
}

impl<'a> FilePicker<'a> {
    pub fn new(state: &'a FilePickerState) -> Self {
        Self { state }
    }
}

impl Widget for FilePicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new("Files", FilePickerContent { state: self.state }).render(area, buf);
    }
}

struct FilePickerContent<'a> {
    state: &'a FilePickerState,
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

        Paragraph::new(self.state.query.as_str())
            .block(Block::bordered().title("Search"))
            .render(input_area, buf);
        Paragraph::new(result_lines(self.state)).render(results_area, buf);
        Paragraph::new(preview_lines(&self.state.preview))
            .block(Block::bordered().title("Preview"))
            .render(preview_area, buf);
    }
}

fn result_lines(state: &FilePickerState) -> Vec<Line<'static>> {
    if state.results.is_empty() {
        let text = match state.collection {
            FileCollectionState::Loading => "Loading files…",
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
            let style = match state.selected == Some(index) {
                true => Style::new().add_modifier(Modifier::REVERSED),
                false => Style::new(),
            };
            Line::styled(entry.display.clone(), style)
        })
        .collect()
}

fn preview_lines(preview: &PreviewState) -> Vec<Line<'static>> {
    match preview {
        PreviewState::Empty => vec![Line::from("No file selected")],
        PreviewState::Loading => vec![Line::from("Loading preview…")],
        PreviewState::Text(lines) => lines.iter().cloned().map(Line::from).collect(),
        PreviewState::Unavailable(error) => vec![Line::from(error.clone())],
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::file_picker::FileEntry;

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
        state.preview = PreviewState::Text(vec!["fn main() {}".to_string()]);
        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(FilePicker::new(&state), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Files"));
        assert!(content.contains("Search"));
        assert!(content.contains("main"));
        assert!(content.contains("src/main.rs"));
        assert!(content.contains("Preview"));
        assert!(content.contains("fn main() {}"));
    }
}
