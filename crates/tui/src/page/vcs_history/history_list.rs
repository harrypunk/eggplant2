use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::{
    component::{commit_message, picker},
    data_object::Model,
    vcs::{CommitDetailsState, HistoryState},
};

use super::change_marker;

pub(super) struct HistoryList<'a> {
    model: &'a Model,
}

impl<'a> HistoryList<'a> {
    pub(super) fn new(model: &'a Model) -> Self {
        Self { model }
    }

    pub(super) fn render(&self, area: Rect, frame: &mut Frame) {
        let (lines, selected) = self.lines();
        let lines = picker::visible_lines(lines, selected, area.height);
        frame.render_widget(
            Paragraph::new(lines).style(self.model.theme.ui().base),
            area,
        );
    }

    fn lines(&self) -> (Vec<Line<'static>>, Option<usize>) {
        let styles = self.model.theme.ui();
        match self.model.vcs.history() {
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
                    if self.model.vcs.history_selected == Some(index) {
                        selected = Some(lines.len());
                    }
                    lines.push(commit_line(self.model, index, commit));
                    if self.model.vcs.history_expanded.as_ref() == Some(&commit.id) {
                        lines.extend(history_file_lines(self.model));
                    }
                    (lines, selected)
                },
            ),
        }
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let mut model = Model::default();
        model.vcs.history = HistoryState::Ready(vec![commit("first"), commit("second")]);
        model.vcs.history_selected = Some(1);
        model.vcs.history_expanded = Some("first".to_string());
        model.vcs.history_files = CommitDetailsState::Ready(CommitDetails {
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

        let (lines, selected) = HistoryList::new(&model).lines();

        assert_eq!(lines.len(), 4);
        assert_eq!(selected, Some(3));
    }
}
