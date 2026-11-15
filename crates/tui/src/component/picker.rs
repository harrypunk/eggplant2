use ratatui::{
    layout::{Constraint, Flex, Layout, Rect},
    text::Line,
};

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

/// Limit rendered rows to a viewport window that contains the selected row.
pub fn visible_lines<'a>(
    lines: Vec<Line<'a>>,
    selected: Option<usize>,
    height: u16,
) -> Vec<Line<'a>> {
    let height = usize::from(height);
    if height == 0 {
        return Vec::new();
    }
    if lines.len() <= height {
        return lines;
    }
    let start = selected.map_or(0, |selected| {
        selected
            .saturating_add(1)
            .saturating_sub(height)
            .min(lines.len() - height)
    });
    lines.into_iter().skip(start).take(height).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visible_window_follows_selection_and_wraps_to_the_top() {
        let lines = || {
            (0..10)
                .map(|index| Line::from(format!("row-{index}")))
                .collect::<Vec<_>>()
        };
        let text = |lines: Vec<Line>| {
            lines
                .into_iter()
                .map(|line| line.to_string())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            text(visible_lines(lines(), Some(0), 3)),
            ["row-0", "row-1", "row-2"]
        );
        assert_eq!(
            text(visible_lines(lines(), Some(4), 3)),
            ["row-2", "row-3", "row-4"]
        );
        assert_eq!(
            text(visible_lines(lines(), Some(9), 3)),
            ["row-7", "row-8", "row-9"]
        );
        assert_eq!(
            text(visible_lines(lines(), Some(0), 3)),
            ["row-0", "row-1", "row-2"]
        );
        assert!(visible_lines(lines(), Some(9), 0).is_empty());
        assert_eq!(visible_lines(lines(), Some(9), 20).len(), 10);
    }
}
