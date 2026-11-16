use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    component::{diff, picker},
    data_object::Model,
    vcs::{CommitDetailsState, DiffState, HistoryState},
};

#[derive(Debug, Default)]
pub struct VcsHistory;

impl VcsHistory {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let detail_open = model.vcs.history_detail != CommitDetailsState::Idle;
        let [history_area, detail_area] = match detail_open {
            true => Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(frame.area()),
            false => [frame.area(), Rect::default()],
        };
        let [title, body] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .flex(Flex::Start)
            .areas(history_area);
        let styles = model.theme.ui();
        frame.render_widget(
            Paragraph::new("History").style(styles.base.patch(styles.key_hint)),
            title,
        );
        let (lines, selected) = history_lines(model);
        let lines = picker::visible_lines(lines, selected, body.height);
        frame.render_widget(Paragraph::new(lines).style(styles.base), body);
        if detail_open {
            frame.render_widget(
                Paragraph::new(detail_lines(model)).style(styles.base),
                detail_area,
            );
        }
    }
}

fn history_lines(model: &Model) -> (Vec<Line<'static>>, Option<usize>) {
    let styles = model.theme.ui();
    match model.vcs.history() {
        HistoryState::Idle | HistoryState::Loading => {
            (vec![Line::styled("Loading history…", styles.muted)], None)
        }
        HistoryState::Failed(error) => (vec![Line::styled(error.clone(), styles.error)], None),
        HistoryState::Ready(commits) if commits.is_empty() => {
            (vec![Line::styled("No commits", styles.muted)], None)
        }
        HistoryState::Ready(commits) => commits.iter().enumerate().fold(
            (Vec::new(), None),
            |(mut lines, mut selected), (index, commit)| {
                if model.vcs.history_selected == Some(index) {
                    selected = Some(lines.len());
                }
                lines.push(commit_line(model, index, commit));
                if model.vcs.history_expanded.as_ref() == Some(&commit.id) {
                    lines.extend(history_file_lines(model));
                }
                (lines, selected)
            },
        ),
    }
}

fn commit_line(model: &Model, index: usize, commit: &::vcs::CommitSummary) -> Line<'static> {
    let styles = model.theme.ui();
    let selected = model.vcs.history_selected == Some(index);
    let marker = if selected { ">" } else { " " };
    let style = if selected {
        styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD)
    } else {
        styles.base
    };
    Line::from(vec![
        Span::styled(format!("{marker} {}  ", commit.id), styles.key_hint),
        Span::styled(commit.subject.clone(), style),
        Span::styled(
            format!("  {}  {}", commit.author, commit.date),
            styles.base.patch(styles.muted),
        ),
    ])
}

fn history_file_lines(model: &Model) -> Vec<Line<'static>> {
    let styles = model.theme.ui();
    match &model.vcs.history_files {
        CommitDetailsState::Idle => Vec::new(),
        CommitDetailsState::Loading => vec![Line::styled("  Loading files…", styles.muted)],
        CommitDetailsState::Failed(error) => vec![Line::styled(error.clone(), styles.error)],
        CommitDetailsState::Ready(details) => details
            .files
            .iter()
            .map(|file| {
                Line::styled(
                    format!("  {} {}", change_marker(file.kind), file.path),
                    styles.base,
                )
            })
            .collect(),
    }
}

fn detail_lines(model: &Model) -> Vec<Line<'static>> {
    let styles = model.theme.ui();
    match &model.vcs.history_detail {
        CommitDetailsState::Idle => Vec::new(),
        CommitDetailsState::Loading => vec![Line::styled("Loading commit…", styles.muted)],
        CommitDetailsState::Failed(error) => vec![Line::styled(error.clone(), styles.error)],
        CommitDetailsState::Ready(details) => {
            let mut lines = vec![
                Line::styled(details.full_id.clone(), styles.key_hint),
                Line::styled(
                    format!("{}  {}", details.author, details.date),
                    styles.base.patch(styles.muted),
                ),
                Line::default(),
            ];
            lines.extend(
                details
                    .message
                    .lines()
                    .map(|line| Line::raw(line.to_string())),
            );
            lines.push(Line::default());
            lines.push(Line::styled("Changed files", styles.key_hint));
            for (index, file) in details.files.iter().enumerate() {
                let selected = model.vcs.history_detail_selected_file == Some(index);
                let marker = if selected { ">" } else { " " };
                lines.push(Line::styled(
                    format!("{marker} {} {}", change_marker(file.kind), file.path),
                    if selected {
                        styles.key_hint
                    } else {
                        styles.base
                    },
                ));
                if model.vcs.history_detail_expanded_file.as_ref() == Some(&file.path) {
                    lines.extend(file_diff_lines(model));
                }
            }
            lines
        }
    }
}

fn file_diff_lines(model: &Model) -> Vec<Line<'static>> {
    let styles = model.theme.ui();
    match &model.vcs.history_detail_diff {
        DiffState::Idle => Vec::new(),
        DiffState::Loading => vec![Line::styled("  Loading diff…", styles.muted)],
        DiffState::Failed(error) => vec![Line::styled(error.clone(), styles.error)],
        DiffState::Ready { lines, truncated } => {
            let mut output = lines
                .iter()
                .take(6)
                .map(|line| Line::styled(format!("  {line}"), diff::line_style(styles, line)))
                .collect::<Vec<_>>();
            if *truncated || lines.len() > 6 {
                output.push(Line::styled("  …", styles.base.patch(styles.muted)));
            }
            output
        }
    }
}

fn change_marker(kind: ::vcs::ChangeKind) -> &'static str {
    match kind {
        ::vcs::ChangeKind::Added => "A",
        ::vcs::ChangeKind::Untracked => "?",
        ::vcs::ChangeKind::Modified => "M",
        ::vcs::ChangeKind::Conflict => "C",
        ::vcs::ChangeKind::Deleted => "D",
        ::vcs::ChangeKind::Renamed => "R",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use ::vcs::{ChangeKind, CommitDetails, CommitFile};

    #[test]
    fn renders_history_files_and_right_commit_detail() {
        let mut model = Model::default();
        model.vcs.history = HistoryState::Ready(vec![::vcs::CommitSummary {
            id: "abc1234".to_string(),
            subject: "current commit".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
        }]);
        model.vcs.history_selected = Some(0);
        model.vcs.history_expanded = Some("abc1234".to_string());
        model.vcs.history_files = CommitDetailsState::Ready(CommitDetails {
            full_id: "abc1234".to_string(),
            message: "complete message\nsecond line".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16".to_string(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        });
        model.vcs.history_detail = model.vcs.history_files.clone();
        model.vcs.history_detail_selected_file = Some(0);
        let backend = TestBackend::new(100, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| VcsHistory.view(&model, frame))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("History"));
        assert!(content.contains("> abc1234"));
        assert!(content.contains("complete message"));
        assert!(content.contains("second line"));
        assert!(content.contains("Changed files"));
        assert!(content.contains("M note.txt"));
    }
}
