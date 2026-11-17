use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::{
    component::{commit_message, picker},
    theme::{Theme, UiTheme},
    vcs::{CommitDetailsState, HistoryState},
};

use super::change_marker;

pub(super) struct HistoryListProps<'a> {
    pub history: &'a HistoryState,
    pub selected: Option<usize>,
    pub expanded: Option<&'a str>,
    pub files: &'a CommitDetailsState,
    pub theme: &'a Theme,
}

pub(super) struct HistoryList<'a> {
    props: HistoryListProps<'a>,
}

impl<'a> HistoryList<'a> {
    pub(super) fn new(props: HistoryListProps<'a>) -> Self {
        Self { props }
    }

    pub(super) fn render(&self, area: Rect, frame: &mut Frame) {
        let (lines, selected) = self.lines();
        let lines = picker::visible_lines(lines, selected, area.height);
        frame.render_widget(
            Paragraph::new(lines).style(self.props.theme.ui().base),
            area,
        );
    }

    fn lines(&self) -> (Vec<Line<'static>>, Option<usize>) {
        let styles = self.props.theme.ui();
        match self.props.history {
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
                    let is_selected = self.props.selected == Some(index);
                    if is_selected {
                        selected = Some(lines.len());
                    }
                    lines.push(commit_line(commit, is_selected, styles));
                    if self.props.expanded == Some(commit.id.as_str()) {
                        lines.extend(history_file_lines(self.props.files, styles));
                    }
                    (lines, selected)
                },
            ),
        }
    }
}

fn commit_line(commit: &::vcs::CommitSummary, selected: bool, styles: &UiTheme) -> Line<'static> {
    let prefix = if selected { "> " } else { "  " };
    commit_message::line(commit, prefix.to_string(), selected, styles)
}

fn history_file_lines(files: &CommitDetailsState, styles: &UiTheme) -> Vec<Line<'static>> {
    match files {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vcs::VcsState;
    use ::vcs::{ChangeKind, CommitDetails, CommitFile, CommitSummary};

    #[test]
    fn selection_accounts_for_expanded_commit_files() {
        let commit = |id: &str| CommitSummary {
            id: id.to_string(),
            subject: id.to_string(),
            author: "Eggplant".to_string(),
            timestamp: 0,
            branches: Vec::new(),
        };
        let mut state = VcsState::default();
        state.history = HistoryState::Ready(vec![commit("first"), commit("second")]);
        state.history_selected = Some(1);
        state.history_expanded = Some("first".to_string());
        state.history_files = CommitDetailsState::Ready(CommitDetails {
            full_id: "first".to_string(),
            message: "message".to_string(),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:30:00 +0000".to_string(),
            files: vec![
                CommitFile {
                    kind: ChangeKind::Modified,
                    path: "first.txt".to_string(),
                    previous_path: None,
                },
                CommitFile {
                    kind: ChangeKind::Added,
                    path: "second.txt".to_string(),
                    previous_path: None,
                },
            ],
        });
        let theme = Theme::default();

        let (lines, selected) = HistoryList::new(HistoryListProps {
            history: state.history(),
            selected: state.history_selected,
            expanded: state.history_expanded.as_deref(),
            files: &state.history_files,
            theme: &theme,
        })
        .lines();

        assert_eq!(lines.len(), 4);
        assert_eq!(selected, Some(3));
    }
}
