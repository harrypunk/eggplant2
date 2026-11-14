use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Position, Rect},
    text::Line,
    widgets::{Block, Paragraph, Widget},
};

use crate::{
    component::{
        picker,
        popup::{self, Popup},
        text_input::TextInput,
        text_view::TextView,
    },
    data_object::{Model, Visibility},
    theme::{Theme, UiTheme},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

use super::{BufferGrepState, SearchStatus};

#[derive(Debug, Clone, Copy)]
pub struct BufferGrep<'a> {
    state: &'a BufferGrepState,
    theme: &'a Theme,
}

impl<'a> BufferGrep<'a> {
    pub fn new(state: &'a BufferGrepState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        let input = picker::content_areas(popup::content_area(area)).input;
        TextInput::new(&self.state.query, self.theme.ui()).cursor_position(input)
    }
}

impl Widget for BufferGrep<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(
            "Search current file",
            BufferGrepContent {
                state: self.state,
                theme: self.theme,
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

struct BufferGrepContent<'a> {
    state: &'a BufferGrepState,
    theme: &'a Theme,
}

pub(crate) fn overlay(model: &Model) -> Option<OverlayContribution> {
    match model.buffer_grep.visibility {
        Visibility::Displayed => Some(
            OverlayContribution::new(
                "buffer_grep",
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
    frame.render_widget(BufferGrep::new(&model.buffer_grep, &model.theme), area);
}

fn cursor_overlay(model: &Model, area: Rect) -> Option<Position> {
    BufferGrep::new(&model.buffer_grep, &model.theme).cursor_position(area)
}

impl Widget for BufferGrepContent<'_> {
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
        match &self.state.preview {
            Some(snapshot) => TextView::new(snapshot, self.theme).render(preview_inner, buf),
            None => Paragraph::new("No match selected")
                .style(surface)
                .render(preview_inner, buf),
        }
    }
}

fn result_lines(state: &BufferGrepState, styles: &UiTheme) -> Vec<Line<'static>> {
    if state.results.is_empty() {
        let text = match &state.status {
            SearchStatus::Idle => "Type a regular expression".to_string(),
            SearchStatus::Searching => "Searching…".to_string(),
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
            Line::styled(format!("{:>4}  {}", result.line + 1, result.text), style)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use editor::{BufferTextSnapshot, DocumentFactory, EditorSession};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::buffer_grep::GrepMatch;

    #[test]
    fn renders_query_results_and_context_preview() {
        let mut editor = EditorSession::new();
        let buffer_id = editor.insert_document(DocumentFactory::new().create(""));
        let source = BufferTextSnapshot {
            buffer_id,
            revision: 0,
            text: "hello\nworld".to_string(),
        };
        let mut state = BufferGrepState::default();
        state.open(source);
        state.query = "ell".to_string();
        state.status = SearchStatus::Ready;
        state.results = vec![GrepMatch {
            line: 0,
            text: "hello".to_string(),
        }];
        state.selected = Some(0);
        state.update_preview();

        let backend = TestBackend::new(60, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                frame.render_widget(BufferGrep::new(&state, &Theme::default()), frame.area())
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Search current file"));
        assert!(content.contains("ell"));
        assert!(content.contains("1  hello"));
        assert!(content.contains("world"));
    }
}
