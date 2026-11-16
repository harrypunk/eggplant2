use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    component::{commit_message, diff, picker},
    data_object::Model,
    vcs::{CommitDetailsState, DiffState, HistoryMoreState, HistoryState},
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
        let [header, body] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .flex(Flex::Start)
            .areas(history_area);
        let styles = model.theme.ui();
        let header_lines =
            std::iter::once(Line::styled("History", styles.base.patch(styles.key_hint)))
                .chain(history_more_line(model))
                .collect::<Vec<_>>();
        frame.render_widget(Paragraph::new(header_lines).style(styles.base), header);
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

fn history_more_line(model: &Model) -> Option<Line<'static>> {
    let styles = model.theme.ui();
    match &model.vcs.history_more {
        HistoryMoreState::Idle if model.vcs.history_has_more => Some(Line::styled(
            "n load more",
            styles.base.patch(styles.action_hint),
        )),
        HistoryMoreState::Idle => None,
        HistoryMoreState::Loading => Some(Line::styled(
            "Loading more…",
            styles.base.patch(styles.muted),
        )),
        HistoryMoreState::Failed(error) => Some(Line::styled(
            format!("{error} — press n to retry"),
            styles.base.patch(styles.error),
        )),
    }
}

fn commit_line(model: &Model, index: usize, commit: &::vcs::CommitSummary) -> Line<'static> {
    let selected = model.vcs.history_selected == Some(index);
    let prefix = if selected { "> " } else { "  " };
    commit_message::line(commit, prefix.to_string(), selected, model.theme.ui())
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
        CommitDetailsState::Ready(details) => [
            Line::styled(details.full_id.clone(), styles.key_hint),
            Line::styled(
                format!("{}  {}", details.author, details.date),
                styles.base.patch(styles.muted),
            ),
            Line::default(),
        ]
        .into_iter()
        .chain(
            details
                .message
                .lines()
                .map(|line| Line::raw(line.to_string())),
        )
        .chain(std::iter::once(Line::default()))
        .chain(std::iter::once(Line::styled(
            "Changed files",
            styles.key_hint,
        )))
        .chain(details.files.iter().enumerate().flat_map(|(index, file)| {
            let row = detail_file_line(model, index, file);
            let diff = if model.vcs.history_detail_expanded_file.as_ref() == Some(&file.path) {
                file_diff_lines(model)
            } else {
                Vec::new()
            };
            std::iter::once(row).chain(diff)
        }))
        .collect(),
    }
}

fn detail_file_line(model: &Model, index: usize, file: &::vcs::CommitFile) -> Line<'static> {
    let styles = model.theme.ui();
    let selected = model.vcs.history_detail_selected_file == Some(index);
    let marker = if selected { ">" } else { " " };
    Line::styled(
        format!("{marker} {} {}", change_marker(file.kind), file.path),
        if selected {
            styles.key_hint
        } else {
            styles.base
        },
    )
}

fn file_diff_lines(model: &Model) -> Vec<Line<'static>> {
    let styles = model.theme.ui();
    match &model.vcs.history_detail_diff {
        DiffState::Idle => Vec::new(),
        DiffState::Loading => vec![Line::styled("  Loading diff…", styles.muted)],
        DiffState::Failed(error) => vec![Line::styled(error.clone(), styles.error)],
        DiffState::Ready { lines, truncated } => lines
            .iter()
            .map(|line| Line::styled(format!("  {line}"), diff::line_style(styles, line)))
            .chain((*truncated).then(|| Line::styled("  …", styles.base.patch(styles.muted))))
            .collect(),
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
    use ratatui::{Terminal, backend::TestBackend, style::Modifier};

    use super::*;
    use crate::component::relative_time;
    use ::vcs::{ChangeKind, CommitDetails, CommitFile};

    #[test]
    fn renders_history_files_and_right_commit_detail() {
        let mut model = Model::default();
        model.vcs.history = HistoryState::Ready(vec![::vcs::CommitSummary {
            id: "abc1234".to_string(),
            subject: "current commit".to_string(),
            author: "Eggplant".to_string(),
            timestamp: relative_time::now(),
            branches: vec!["main".to_string()],
        }]);
        model.vcs.history_selected = Some(0);
        model.vcs.history_has_more = true;
        model.vcs.history_expanded = Some("abc1234".to_string());
        model.vcs.history_files = CommitDetailsState::Ready(CommitDetails {
            full_id: "abc1234".to_string(),
            message: "complete message\nsecond line".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:30:00 +0000".to_string(),
            files: vec![CommitFile {
                kind: ChangeKind::Modified,
                path: "note.txt".to_string(),
                previous_path: None,
            }],
        });
        model.vcs.history_detail = model.vcs.history_files.clone();
        model.vcs.history_detail_selected_file = Some(0);
        let backend = TestBackend::new(120, 12);
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
        assert!(content.contains("just now"));
        assert!(content.contains("[main]"), "{content}");
        assert!(content.contains("2026-11-16 10:30:00 +0000"));
        assert!(content.contains("n load more"));
        let load_more = &terminal.backend().buffer()[(0, 1)];
        assert_eq!(load_more.symbol(), "n");
        assert_eq!(
            load_more.fg,
            model.theme.ui().action_hint.fg.unwrap(),
            "load-more help should use the accent color"
        );
        assert!(load_more.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn expanded_file_diff_renders_every_loaded_line() {
        let mut model = Model::default();
        model.vcs.history_detail_diff = DiffState::Ready {
            lines: (0..8).map(|index| format!("+line-{index}")).collect(),
            truncated: false,
        };

        let lines = file_diff_lines(&model);

        assert_eq!(lines.len(), 8);
        assert_eq!(lines[7].to_string(), "  +line-7");
    }
}
