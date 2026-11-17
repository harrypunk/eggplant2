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

pub(super) struct BranchPopupProps<'a> {
    pub query: &'a str,
    pub targets: Vec<&'a str>,
    pub selected: Option<usize>,
    pub creating: bool,
    pub loading: bool,
    pub switching: bool,
    pub naming: bool,
    pub selecting: bool,
    pub start: Option<String>,
    pub revision: Option<&'a str>,
    pub error: Option<&'a str>,
    pub theme: &'a Theme,
}

pub(super) struct BranchPopup<'a> {
    props: BranchPopupProps<'a>,
}

struct BranchPopupLayout {
    query: Rect,
    targets: Rect,
    status: Rect,
}

impl BranchPopupLayout {
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

impl<'a> BranchPopup<'a> {
    pub(super) fn new(props: BranchPopupProps<'a>) -> Self {
        Self { props }
    }

    pub(super) fn render(&self, frame: &mut Frame, area: Rect) {
        let styles = self.props.theme.ui();
        let popup = popup_area(area);
        let title = match (self.props.creating, self.props.naming) {
            (true, true) => "New branch: name",
            (true, false) => "New branch: start point",
            (false, _) => "Switch branch",
        };
        let block = Block::new()
            .title(title)
            .borders(Borders::ALL)
            .border_style(styles.base.patch(styles.focused_border));
        let layout = BranchPopupLayout::new(block.inner(popup));
        frame.render_widget(block.style(styles.base), popup);

        let cursor = render_query(&self.props, styles, frame, layout.query);
        render_targets(&self.props, styles, frame, layout.targets);
        render_status(&self.props, styles, frame, layout.status);
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

fn render_query(
    props: &BranchPopupProps<'_>,
    styles: &UiTheme,
    frame: &mut Frame,
    area: Rect,
) -> Option<Position> {
    let input = TextInput::new(props.query, styles);
    let cursor = (props.selecting || props.naming)
        .then(|| input.cursor_position(area))
        .flatten();
    frame.render_widget(input, area);
    cursor
}

fn render_targets(props: &BranchPopupProps<'_>, styles: &UiTheme, frame: &mut Frame, area: Rect) {
    let lines = picker::visible_lines(target_lines(props, styles), props.selected, area.height);
    frame.render_widget(Paragraph::new(lines).style(styles.base), area);
}

fn target_lines(props: &BranchPopupProps<'_>, styles: &UiTheme) -> Vec<Line<'static>> {
    if props.loading {
        return vec![Line::styled("Loading branches…", styles.muted)];
    }
    if props.switching {
        return vec![Line::styled("Switching…", styles.muted)];
    }
    if props.naming {
        return vec![Line::styled(
            props.start.clone().unwrap_or_default(),
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

fn render_status(props: &BranchPopupProps<'_>, styles: &UiTheme, frame: &mut Frame, area: Rect) {
    let (content, style) = status_content(props, styles);
    frame.render_widget(Paragraph::new(content).style(style), area);
}

fn status_content(props: &BranchPopupProps<'_>, styles: &UiTheme) -> (String, Style) {
    if let Some(error) = props.error {
        return (error.to_string(), styles.base.patch(styles.error));
    }
    let hint = if props.naming {
        "Enter create and switch  Esc close"
    } else if props.creating {
        "Filter or enter commit hash  Up/Down select  Enter next  Esc close"
    } else {
        "Type to filter  Up/Down select  Enter switch  Esc close"
    };
    (hint.to_string(), styles.base.patch(styles.muted))
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    fn props<'a>(theme: &'a Theme) -> BranchPopupProps<'a> {
        BranchPopupProps {
            query: "",
            targets: vec!["HEAD", "main"],
            selected: Some(0),
            creating: true,
            loading: false,
            switching: false,
            naming: false,
            selecting: true,
            start: None,
            revision: None,
            error: None,
            theme,
        }
    }

    #[test]
    fn renders_head_before_branches() {
        let theme = Theme::default();
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| {
                BranchPopup::new(props(&theme)).render(frame, frame.area());
            })
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("New branch: start point"));
        assert!(content.find("> HEAD").unwrap() < content.find("main").unwrap());
        assert!(content.contains("commit hash"));
    }

    #[test]
    fn target_lines_show_explicit_revision_and_naming_start() {
        let theme = Theme::default();
        let mut props = props(&theme);
        props.revision = Some("abc123");
        props.targets.clear();
        assert_eq!(
            target_lines(&props, theme.ui())[0].to_string(),
            "Use commit abc123"
        );

        props.revision = None;
        props.naming = true;
        props.start = Some("From commit abc123".to_string());
        assert_eq!(
            target_lines(&props, theme.ui())[0].to_string(),
            "From commit abc123"
        );
        assert!(
            status_content(&props, theme.ui())
                .0
                .contains("create and switch")
        );
    }
}
