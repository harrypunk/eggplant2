mod commit_detail;
mod history_list;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    text::Line,
    widgets::Paragraph,
};

use crate::{data_object::Model, vcs::HistoryMoreState};

use self::{
    commit_detail::{CommitDetail, CommitDetailProps},
    history_list::{HistoryList, HistoryListProps},
};

#[derive(Debug, Default)]
pub struct VcsHistory;

impl VcsHistory {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let detail_open = model.vcs.history_pane == crate::vcs::HistoryPane::Detail;
        let areas = VcsHistoryLayout::new(frame.area(), detail_open);
        let styles = model.theme.ui();
        let header_lines =
            std::iter::once(Line::styled("History", styles.base.patch(styles.key_hint)))
                .chain(history_more_line(model))
                .collect::<Vec<_>>();
        frame.render_widget(
            Paragraph::new(header_lines).style(styles.base),
            areas.header,
        );
        HistoryList::new(HistoryListProps {
            history: model.vcs.history(),
            selected: model.vcs.history_selected,
            expanded: model.vcs.history_expanded.as_deref(),
            files: &model.vcs.history_files,
            theme: &model.theme,
        })
        .render(areas.body, frame);
        if let Some(detail) = areas.detail {
            CommitDetail::new(CommitDetailProps {
                detail: &model.vcs.history_detail,
                selected_file: model.vcs.history_detail_selected_file,
                expanded_file: model.vcs.history_detail_expanded_file.as_deref(),
                diff: &model.vcs.history_detail_diff,
                theme: &model.theme,
            })
            .render(detail, frame);
        }
    }
}

struct VcsHistoryLayout {
    header: Rect,
    body: Rect,
    detail: Option<Rect>,
}

impl VcsHistoryLayout {
    fn new(area: Rect, detail_open: bool) -> Self {
        let [history_area, detail_area] = match detail_open {
            true => Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)])
                .flex(Flex::Start)
                .areas(area),
            false => [area, Rect::default()],
        };
        let [header, body] = Layout::vertical([Constraint::Length(2), Constraint::Fill(1)])
            .flex(Flex::Start)
            .areas(history_area);
        Self {
            header,
            body,
            detail: detail_open.then_some(detail_area),
        }
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
    use crate::{
        component::relative_time,
        vcs::{CommitDetailsState, HistoryState},
    };
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
        model.vcs.history_pane = crate::vcs::HistoryPane::Detail;
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
}
