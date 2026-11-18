use ratatui::{style::Style, text::Line};

use crate::theme::UiTheme;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub enum DiffState {
    #[default]
    Idle,
    Loading,
    Ready {
        lines: Vec<String>,
        truncated: bool,
    },
    Failed(String),
}

pub fn lines(state: &DiffState, styles: &UiTheme) -> Vec<Line<'static>> {
    match state {
        DiffState::Idle => Vec::new(),
        DiffState::Loading => vec![Line::styled(
            "  Loading diff…",
            styles.base.patch(styles.muted),
        )],
        DiffState::Failed(error) => vec![Line::styled(
            format!("  {error}"),
            styles.base.patch(styles.error),
        )],
        DiffState::Ready { lines, truncated } => lines
            .iter()
            .map(|line| Line::styled(format!("  {line}"), line_style(styles, line)))
            .chain((*truncated).then(|| Line::styled("  …", styles.base.patch(styles.muted))))
            .collect(),
    }
}

pub fn line_style(styles: &UiTheme, line: &str) -> Style {
    match line.as_bytes().first() {
        Some(b'+') => styles.base.patch(styles.diff_add),
        Some(b'-') => styles.base.patch(styles.diff_remove),
        Some(b'@') if line.starts_with("@@") => styles.base.patch(styles.diff_hunk),
        _ => styles.base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_every_loaded_line_and_only_marks_backend_truncation() {
        let theme = Theme::default();
        let state = DiffState::Ready {
            lines: (0..8).map(|index| format!("+line-{index}")).collect(),
            truncated: false,
        };
        let rendered = lines(&state, theme.ui());
        assert_eq!(rendered.len(), 8);
        assert_eq!(rendered[7].to_string(), "  +line-7");

        let state = DiffState::Ready {
            lines: vec!["+changed".to_string()],
            truncated: true,
        };
        assert_eq!(lines(&state, theme.ui()).last().unwrap().to_string(), "  …");
    }
}
