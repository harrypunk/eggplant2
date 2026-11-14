use ratatui::layout::{Constraint, Flex, Layout, Rect};

pub struct Areas {
    pub input: Rect,
    pub results: Rect,
    pub preview: Rect,
}

/// Shared query/results/preview geometry for picker-style extensions.
pub fn content_areas(area: Rect) -> Areas {
    let [left, preview] = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Fill(1)])
        .flex(Flex::Start)
        .areas(area);
    let [input, results] = Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
        .flex(Flex::Start)
        .areas(left);
    Areas {
        input,
        results,
        preview,
    }
}
