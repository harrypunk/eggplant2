use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::{
    component::{diff, picker},
    theme::{Theme, UiTheme},
    vcs::{CommitDetailsState, DiffState},
};

use super::change_marker;

#[derive(Debug, Clone, Copy)]
pub(super) struct CommitDetailProps<'a> {
    pub detail: &'a CommitDetailsState,
    pub selected_file: Option<usize>,
    pub expanded_file: Option<&'a str>,
    pub diff: &'a DiffState,
    pub theme: &'a Theme,
}

pub(super) struct CommitDetail<'a> {
    props: CommitDetailProps<'a>,
}

impl<'a> CommitDetail<'a> {
    pub(super) fn new(props: CommitDetailProps<'a>) -> Self {
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
        match self.props.detail {
            CommitDetailsState::Idle => (Vec::new(), None),
            CommitDetailsState::Loading => {
                (vec![Line::styled("Loading commit…", styles.muted)], None)
            }
            CommitDetailsState::Failed(error) => {
                (vec![Line::styled(error.clone(), styles.error)], None)
            }
            CommitDetailsState::Ready(details) => {
                let header = [
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
                .collect::<Vec<_>>();
                details.files.iter().enumerate().fold(
                    (header, None),
                    |(mut lines, mut selected), (index, file)| {
                        let is_selected = self.props.selected_file == Some(index);
                        if is_selected {
                            selected = Some(lines.len());
                        }
                        lines.push(detail_file_line(file, is_selected, styles));
                        if self.props.expanded_file == Some(file.path.as_str()) {
                            lines.extend(diff::lines(self.props.diff, styles));
                        }
                        (lines, selected)
                    },
                )
            }
        }
    }
}

fn detail_file_line(file: &::vcs::CommitFile, selected: bool, styles: &UiTheme) -> Line<'static> {
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

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::vcs::VcsState;
    use ::vcs::{ChangeKind, CommitDetails, CommitFile};

    #[test]
    fn selected_file_stays_visible_after_long_message_and_expanded_diff() {
        let file = |path: &str| CommitFile {
            kind: ChangeKind::Modified,
            path: path.to_string(),
            previous_path: None,
        };
        let mut state = VcsState::default();
        state.history_detail = CommitDetailsState::Ready(CommitDetails {
            full_id: "abc1234".to_string(),
            message: (0..20)
                .map(|index| format!("message-{index}"))
                .collect::<Vec<_>>()
                .join("\n"),
            author: "Eggplant".to_string(),
            date: "2026-11-16 10:30:00 +0000".to_string(),
            files: vec![file("first.txt"), file("second.txt")],
        });
        state.history_detail_selected_file = Some(1);
        state.history_detail_expanded_file = Some("first.txt".to_string());
        state.history_detail_diff = DiffState::Ready {
            lines: vec![
                "-old".to_string(),
                "+new".to_string(),
                " context".to_string(),
            ],
            truncated: false,
        };
        let theme = Theme::default();
        let props = CommitDetailProps {
            detail: &state.history_detail,
            selected_file: state.history_detail_selected_file,
            expanded_file: state.history_detail_expanded_file.as_deref(),
            diff: &state.history_detail_diff,
            theme: &theme,
        };

        let (lines, selected) = CommitDetail::new(props).lines();
        assert_eq!(lines.len(), 30);
        assert_eq!(selected, Some(29));

        let backend = TestBackend::new(80, 6);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| CommitDetail::new(props).render(frame.area(), frame))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("> M second.txt"));
    }
}
