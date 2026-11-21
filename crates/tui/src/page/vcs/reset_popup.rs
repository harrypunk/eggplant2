use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    component::{picker, text_input::TextInput},
    theme::{Theme, UiTheme},
};

pub(super) struct ResetPopupProps<'a> {
    pub input: &'a TextInput,
    pub targets: Vec<&'a str>,
    pub selected: Option<usize>,
    pub loading: bool,
    pub resetting: bool,
    pub selecting: bool,
    pub target: Option<&'a str>,
    pub revision: Option<&'a str>,
    pub error: Option<&'a str>,
    pub theme: &'a Theme,
}

pub(super) struct ResetPopup<'a> {
    props: ResetPopupProps<'a>,
}

impl<'a> ResetPopup<'a> {
    pub(super) fn new(props: ResetPopupProps<'a>) -> Self {
        Self { props }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let styles = self.props.theme.ui();
        let popup = popup_area(area);
        let block = Block::new()
            .title("Soft reset")
            .borders(Borders::ALL)
            .border_style(styles.base.patch(styles.focused_border));
        let [query, targets, status] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(block.inner(popup));
        frame.render_widget(block.style(styles.base), popup);

        let input = self.props.input.view(styles);
        let cursor = self
            .props
            .selecting
            .then(|| input.cursor_position(query))
            .flatten();
        frame.render_widget(input, query);
        let lines = picker::visible_lines(
            target_lines(&self.props, styles),
            self.props.selected,
            targets.height,
        );
        frame.render_widget(Paragraph::new(lines).style(styles.base), targets);
        let (content, style) = status_content(self.props.error, styles);
        frame.render_widget(Paragraph::new(content).style(style), status);
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
    }
}

fn popup_area(area: Rect) -> Rect {
    let width = area.width.min(64);
    let height = area.height.min(16);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    )
}

fn target_lines(props: &ResetPopupProps<'_>, styles: &UiTheme) -> Vec<Line<'static>> {
    if props.loading {
        return vec![Line::styled("Loading refs and commits…", styles.muted)];
    }
    if props.resetting {
        return vec![Line::styled(
            format!(
                "Resetting to {} (index and worktree unchanged)…",
                props.target.unwrap_or("target")
            ),
            styles.muted,
        )];
    }
    if let Some(revision) = props.revision {
        return vec![Line::styled(
            format!("Use commit {revision}"),
            styles.key_hint,
        )];
    }
    if props.targets.is_empty() {
        return vec![Line::styled("No matching refs or commits", styles.muted)];
    }
    props
        .targets
        .iter()
        .enumerate()
        .map(|(index, target)| target_line(target, props.selected == Some(index), styles))
        .collect()
}

fn target_line(target: &str, selected: bool, styles: &UiTheme) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let style = if selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    Line::styled(format!("{marker}{target}"), style)
}

fn status_content(error: Option<&str>, styles: &UiTheme) -> (String, Style) {
    error.map_or(
        (
            "Filter or enter commit hash  Up/Down select  Enter reset  Esc close".to_string(),
            styles.base.patch(styles.muted),
        ),
        |error| (error.to_string(), styles.base.patch(styles.error)),
    )
}
