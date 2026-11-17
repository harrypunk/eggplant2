use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    component::{picker, text_input::TextInput},
    theme::{Theme, UiTheme},
};

pub(super) struct MergePopupProps<'a> {
    pub query: &'a str,
    pub targets: Vec<&'a str>,
    pub selected: Option<usize>,
    pub loading: bool,
    pub merging: bool,
    pub error: Option<&'a str>,
    pub theme: &'a Theme,
}

pub(super) struct MergePopup<'a> {
    props: MergePopupProps<'a>,
}

struct MergePopupLayout {
    query: Rect,
    targets: Rect,
    status: Rect,
}

impl MergePopupLayout {
    fn new(area: Rect) -> Self {
        let [query, targets, status] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        Self {
            query,
            targets,
            status,
        }
    }
}

impl<'a> MergePopup<'a> {
    pub(super) fn new(props: MergePopupProps<'a>) -> Self {
        Self { props }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let styles = self.props.theme.ui();
        let popup = popup_area(area);
        let block = Block::new()
            .title("Switch & fast-forward")
            .borders(Borders::ALL)
            .border_style(styles.base.patch(styles.focused_border));
        let layout = MergePopupLayout::new(block.inner(popup));
        frame.render_widget(block.style(styles.base), popup);

        let cursor = render_query(self.props.query, styles, frame, layout.query);
        render_targets(&self.props, styles, frame, layout.targets);
        render_status(self.props.error, styles, frame, layout.status);
        if let Some(cursor) = cursor {
            frame.set_cursor_position(cursor);
        }
    }
}

fn popup_area(area: Rect) -> Rect {
    let width = area.width.min(64);
    let height = area.height.min(12);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 3,
        width,
        height,
    )
}

fn render_query(query: &str, styles: &UiTheme, frame: &mut Frame, area: Rect) -> Option<Position> {
    let input = TextInput::new(query, styles);
    let cursor = input.cursor_position(area);
    frame.render_widget(input, area);
    cursor
}

fn render_targets(props: &MergePopupProps<'_>, styles: &UiTheme, frame: &mut Frame, area: Rect) {
    let lines = picker::visible_lines(target_lines(props, styles), props.selected, area.height);
    frame.render_widget(Paragraph::new(lines).style(styles.base), area);
}

fn target_lines(props: &MergePopupProps<'_>, styles: &UiTheme) -> Vec<Line<'static>> {
    if props.loading {
        return vec![Line::styled("Loading branches…", styles.muted)];
    }
    if props.merging {
        return vec![Line::styled("Fast-forwarding…", styles.muted)];
    }
    if props.targets.is_empty() {
        return vec![Line::styled("No matching branches", styles.muted)];
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

fn render_status(error: Option<&str>, styles: &UiTheme, frame: &mut Frame, area: Rect) {
    let (content, style) = status_content(error, styles);
    frame.render_widget(Paragraph::new(content).style(style), area);
}

fn status_content(error: Option<&str>, styles: &UiTheme) -> (String, Style) {
    error.map_or(
        (
            "Type to filter  Up/Down select  Enter switch  Esc close".to_string(),
            styles.base.patch(styles.muted),
        ),
        |error| (error.to_string(), styles.base.patch(styles.error)),
    )
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_filtered_targets_selection_and_error() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                MergePopup::new(MergePopupProps {
                    query: "ori",
                    targets: vec!["origin/main"],
                    selected: Some(0),
                    loading: false,
                    merging: false,
                    error: Some("not a fast-forward"),
                    theme: &theme,
                })
                .render(frame, frame.area())
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Switch & fast-forward"));
        assert!(content.contains("> origin/main"));
        assert!(!content.contains("> main"));
        assert!(content.contains("not a fast-forward"));
    }

    #[test]
    fn target_lines_reflect_loading_and_merging_phases() {
        let theme = Theme::default();
        let mut props = MergePopupProps {
            query: "",
            targets: Vec::new(),
            selected: None,
            loading: true,
            merging: false,
            error: None,
            theme: &theme,
        };
        assert_eq!(
            target_lines(&props, theme.ui())[0].to_string(),
            "Loading branches…"
        );

        props.loading = false;
        props.merging = true;
        assert_eq!(
            target_lines(&props, theme.ui())[0].to_string(),
            "Fast-forwarding…"
        );
    }
}
