use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use ::vcs::{ChangeKind, FileChange};

use crate::{
    component::{diff, picker},
    data_object::Model,
    theme::UiTheme,
    vcs::{ChangesState, DiffState, HistoryState},
};

const DIFF_DISPLAY_LIMIT: usize = 6;

#[derive(Debug, Default)]
pub struct Vcs;

impl Vcs {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let [hints_area, head_area, body] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Fill(1),
        ])
        .flex(Flex::Start)
        .areas(frame.area());
        let styles = model.theme.ui();
        let heading = styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD);

        let mut hints = vec![Line::styled(
            "Tab toggle  s stage  u unstage  l history  x discard  c commit  ? help",
            heading,
        )];
        if let Some(notice) = &model.vcs.notice {
            hints.push(Line::styled(notice.clone(), styles.muted));
        }
        frame.render_widget(Paragraph::new(hints).style(styles.base), hints_area);
        frame.render_widget(
            Paragraph::new(head_line(model, styles)).style(styles.base),
            head_area,
        );

        match model.vcs.changes() {
            ChangesState::Idle | ChangesState::Loading => {
                frame.render_widget(
                    Paragraph::new("Loading changes…").style(styles.base.patch(styles.muted)),
                    body,
                );
            }
            ChangesState::Failed(error) => {
                frame.render_widget(
                    Paragraph::new(error.as_str()).style(styles.base.patch(styles.error)),
                    body,
                );
            }
            ChangesState::Ready(changes) if changes.is_empty() => {
                frame.render_widget(
                    Paragraph::new("No changes").style(styles.base.patch(styles.muted)),
                    body,
                );
            }
            ChangesState::Ready(_) => {
                let staged = render_section(model, styles, body, true, frame);
                let remaining = Rect {
                    y: body.y.saturating_add(staged),
                    height: body.height.saturating_sub(staged),
                    ..body
                };
                render_section(model, styles, remaining, false, frame);
            }
        }
    }
}

fn head_line(model: &Model, styles: &UiTheme) -> Line<'static> {
    let head = model.vcs.head().unwrap_or("--");
    match model.vcs.history() {
        HistoryState::Ready(commits) if !commits.is_empty() => {
            let commit = &commits[0];
            Line::from(vec![
                Span::styled(format!("HEAD {head}  "), styles.key_hint),
                Span::styled(format!("{}  ", commit.id), styles.key_hint),
                Span::raw(commit.subject.clone()),
            ])
        }
        HistoryState::Idle | HistoryState::Loading => {
            Line::styled(format!("HEAD {head}  Loading…"), styles.muted)
        }
        HistoryState::Ready(_) | HistoryState::Failed(_) => {
            Line::styled(format!("HEAD {head}"), styles.key_hint)
        }
    }
}

fn render_section(
    model: &Model,
    styles: &UiTheme,
    area: Rect,
    staged: bool,
    frame: &mut Frame,
) -> u16 {
    let (lines, selected) = change_lines(model, styles, staged);
    if lines.is_empty() || area.height == 0 {
        return 0;
    }
    let title = if staged {
        "Staged files"
    } else {
        "Unstaged files"
    };
    frame.render_widget(
        Paragraph::new(title).style(styles.base.patch(styles.key_hint)),
        Rect { height: 1, ..area },
    );
    let content = Rect {
        y: area.y.saturating_add(1),
        height: area.height.saturating_sub(1),
        ..area
    };
    let lines = picker::visible_lines(lines, selected, content.height);
    let rendered = lines.len() as u16;
    frame.render_widget(Paragraph::new(lines).style(styles.base), content);
    rendered.saturating_add(1)
}

/// Build one staged/unstaged section and its picker selection.
///
/// Each file contributes one row, followed by its diff when expanded. The
/// returned selection is the selected file's rendered row, while the model's
/// selection indexes the combined change list.
fn change_lines(
    model: &Model,
    styles: &UiTheme,
    staged: bool,
) -> (Vec<Line<'static>>, Option<usize>) {
    let ChangesState::Ready(changes) = model.vcs.changes() else {
        return (Vec::new(), None);
    };
    changes
        .iter()
        .enumerate()
        .filter(|(_, change)| change.staged == staged)
        .fold(
            (Vec::new(), None),
            |(mut lines, mut selected), (index, change)| {
                if model.vcs.selected_change == Some(index) {
                    selected = Some(lines.len());
                }
                lines.push(change_line(model, styles, index, change));
                let expanded = model
                    .vcs
                    .expanded_change
                    .as_ref()
                    .is_some_and(|(path, staged)| path == &change.path && *staged == change.staged);
                if expanded {
                    lines.extend(diff_lines(model, styles));
                }
                (lines, selected)
            },
        )
}

fn change_line(
    model: &Model,
    styles: &UiTheme,
    index: usize,
    change: &FileChange,
) -> Line<'static> {
    let is_selected = model.vcs.selected_change == Some(index);
    let style = if is_selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    let marker = if is_selected { "> " } else { "  " };
    Line::styled(format!("{marker}{}", change_text(model, change)), style)
}

fn diff_lines(model: &Model, styles: &UiTheme) -> Vec<Line<'static>> {
    match &model.vcs.diff {
        DiffState::Idle => Vec::new(),
        DiffState::Loading => vec![Line::styled(
            "  Loading diff…",
            styles.base.patch(styles.muted),
        )],
        DiffState::Failed(error) => vec![Line::styled(
            format!("  {error}"),
            styles.base.patch(styles.error),
        )],
        DiffState::Ready { lines, truncated } => {
            let mut output = lines
                .iter()
                .take(DIFF_DISPLAY_LIMIT)
                .map(|line| Line::styled(format!("  {line}"), diff::line_style(styles, line)))
                .collect::<Vec<_>>();
            if *truncated || lines.len() > DIFF_DISPLAY_LIMIT {
                output.push(Line::styled("  …", styles.base.patch(styles.muted)));
            }
            output
        }
    }
}

fn change_text(model: &Model, change: &FileChange) -> String {
    let marker = match change.kind {
        ChangeKind::Added => "A",
        ChangeKind::Untracked => "?",
        ChangeKind::Modified => "M",
        ChangeKind::Conflict => "C",
        ChangeKind::Deleted => "D",
        ChangeKind::Renamed => "R",
    };
    let path = display_path(model, &change.path);
    match &change.previous_path {
        Some(previous) => format!("{marker} {} -> {path}", display_path(model, previous)),
        None => format!("{marker} {path}"),
    }
}

fn display_path(model: &Model, path: &std::path::Path) -> String {
    model
        .workspace
        .as_ref()
        .and_then(|workspace| path.strip_prefix(workspace.root()).ok())
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::*;
    use crate::data_object::Workspace;

    fn change(path: &str, kind: ChangeKind, staged: bool) -> FileChange {
        FileChange {
            kind,
            staged,
            path: std::path::PathBuf::from(format!("/workspace/{path}")),
            previous_path: None,
        }
    }

    #[test]
    fn renders_dynamic_sections_head_and_inline_diff_without_git_log() {
        let mut model = Model {
            workspace: Some(Workspace::new(std::path::PathBuf::from("/workspace"))),
            ..Model::default()
        };
        model.vcs.history = HistoryState::Ready(vec![::vcs::CommitSummary {
            id: "abc1234".to_string(),
            subject: "current commit".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
        }]);
        model.vcs.changes =
            ChangesState::Ready(vec![change("note.rs", ChangeKind::Modified, false)]);
        model.vcs.selected_change = Some(0);
        model.vcs.expanded_change = Some((std::path::PathBuf::from("/workspace/note.rs"), false));
        model.vcs.diff = DiffState::Ready {
            lines: vec![
                "@@ -1 +1 @@".to_string(),
                "-old".to_string(),
                "+changed".to_string(),
            ],
            truncated: false,
        };
        let backend = TestBackend::new(80, 14);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| Vcs.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Tab toggle"));
        assert!(content.contains("abc1234  current commit"));
        assert!(!content.contains("Staged files"));
        assert!(content.contains("Unstaged files"));
        assert!(content.contains("> M note.rs"));
        assert!(content.contains("+changed"));
        assert!(!content.contains("Recent commits"));
        assert!(!content.contains("NORMAL"));
        let cells = terminal.backend().buffer().content();
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "+" && cell.fg == Color::Green)
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "-" && cell.fg == Color::Red)
        );
        assert!(
            cells
                .iter()
                .any(|cell| cell.symbol() == "@" && cell.fg == Color::LightBlue)
        );
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.bg != Color::White)
        );
    }

    #[test]
    fn change_lines_tracks_selection_after_an_expanded_diff() {
        let mut model = Model::default();
        model.vcs.changes = ChangesState::Ready(vec![
            change("first.rs", ChangeKind::Modified, false),
            change("second.rs", ChangeKind::Modified, false),
        ]);
        model.vcs.selected_change = Some(1);
        model.vcs.expanded_change = Some((std::path::PathBuf::from("/workspace/first.rs"), false));
        model.vcs.diff = DiffState::Ready {
            lines: vec!["+first".to_string()],
            truncated: false,
        };

        let (lines, selected) = change_lines(&model, model.theme.ui(), false);

        assert_eq!(lines.len(), 3);
        assert_eq!(selected, Some(2));
    }

    #[test]
    fn empty_state_uses_the_dark_theme_background() {
        let mut model = Model::default();
        model.vcs.changes = ChangesState::Ready(Vec::new());
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| Vcs.view(&model, frame)).unwrap();

        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.bg == Color::Black)
        );
    }
}
