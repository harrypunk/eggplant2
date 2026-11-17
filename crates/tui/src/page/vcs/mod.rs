mod change_section;
mod merge_popup;

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::Modifier,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    component::commit_message,
    data_object::Model,
    theme::UiTheme,
    vcs::{ChangesState, HistoryState},
};

use self::{
    change_section::{ChangeSection, ChangeSectionProps},
    merge_popup::{MergePopup, MergePopupProps},
};

#[derive(Debug, Default)]
pub struct Vcs;

impl Vcs {
    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let areas = VcsLayout::new(frame.area(), model.vcs.help_visible);
        let styles = model.theme.ui();
        let heading = styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD);

        let mut hints = vec![Line::styled(
            "Tab toggle  s stage  u unstage  c commit  x discard  ? help",
            heading,
        )];
        if let Some(notice) = &model.vcs.notice {
            hints.push(Line::styled(notice.clone(), styles.muted));
        }
        frame.render_widget(Paragraph::new(hints).style(styles.base), areas.hints);
        frame.render_widget(
            Paragraph::new(head_line(model, styles)).style(styles.base),
            areas.head,
        );

        match model.vcs.changes() {
            ChangesState::Idle | ChangesState::Loading => {
                frame.render_widget(
                    Paragraph::new("Loading changes…").style(styles.base.patch(styles.muted)),
                    areas.changes,
                );
            }
            ChangesState::Failed(error) => {
                frame.render_widget(
                    Paragraph::new(error.as_str()).style(styles.base.patch(styles.error)),
                    areas.changes,
                );
            }
            ChangesState::Ready(changes) if changes.is_empty() => {
                frame.render_widget(
                    Paragraph::new("No changes").style(styles.base.patch(styles.muted)),
                    areas.changes,
                );
            }
            ChangesState::Ready(_) => {
                let props = ChangeSectionProps {
                    changes: model.vcs.changes(),
                    selected: model.vcs.changes.selected,
                    expanded: model.vcs.changes.expanded(),
                    diff: model.vcs.changes.diff(),
                    workspace_root: model.workspace.as_ref().map(|workspace| workspace.root()),
                    theme: &model.theme,
                };
                let consumed = ChangeSection::staged(props).render(areas.changes, frame);
                ChangeSection::unstaged(props).render(areas.after_staged(consumed), frame);
            }
        }
        if model.vcs.help_visible {
            render_help(model, frame, areas.bottom);
        }
        if model.vcs.merge.is_active() {
            MergePopup::new(MergePopupProps {
                query: model.vcs.merge.query(),
                targets: model.vcs.merge.filtered_targets(),
                selected: model.vcs.merge.selected(),
                loading: model.vcs.merge.is_loading(),
                merging: model.vcs.merge.is_merging(),
                error: model.vcs.merge.error(),
                theme: &model.theme,
            })
            .render(frame, frame.area());
        }
    }
}

struct VcsLayout {
    hints: Rect,
    head: Rect,
    changes: Rect,
    bottom: Rect,
}

impl VcsLayout {
    fn new(area: Rect, help_visible: bool) -> Self {
        let bottom_height = if help_visible { 5 } else { 0 };
        let [hints, head, changes, bottom] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(bottom_height),
        ])
        .flex(Flex::Start)
        .areas(area);
        Self {
            hints,
            head,
            changes,
            bottom,
        }
    }

    fn after_staged(&self, consumed: u16) -> Rect {
        Rect {
            y: self.changes.y.saturating_add(consumed),
            height: self.changes.height.saturating_sub(consumed),
            ..self.changes
        }
    }
}

fn render_help(model: &Model, frame: &mut Frame, area: Rect) {
    let styles = model.theme.ui();
    let lines = vec![
        Line::raw("s stage   u unstage   Ctrl-s stage all   U unstage all"),
        Line::raw("Tab toggle  Enter open  l history  c commit  m merge"),
        Line::raw("? / Esc close help    x discard (placeholder)"),
    ];
    let help = Paragraph::new(lines).block(
        Block::new()
            .title("Help")
            .borders(Borders::ALL)
            .border_style(styles.base.patch(styles.focused_border)),
    );
    frame.render_widget(help.style(styles.base), area);
}

fn head_line(model: &Model, styles: &UiTheme) -> Line<'static> {
    let head = model.vcs.head().unwrap_or("--");
    match model.vcs.history() {
        HistoryState::Ready(commits) if !commits.is_empty() => {
            let commit = &commits[0];
            commit_message::line(commit, format!("HEAD {head}  "), false, styles)
        }
        HistoryState::Idle | HistoryState::Loading => {
            Line::styled(format!("HEAD {head}  Loading…"), styles.muted)
        }
        HistoryState::Ready(_) | HistoryState::Failed(_) => {
            Line::styled(format!("HEAD {head}"), styles.key_hint)
        }
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend, style::Color};

    use super::*;
    use crate::{component::relative_time, data_object::Workspace, vcs::DiffState};
    use ::vcs::{ChangeKind, FileChange};

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
            timestamp: relative_time::now(),
            branches: Vec::new(),
        }]);
        model.vcs.changes.state =
            ChangesState::Ready(vec![change("note.rs", ChangeKind::Modified, false)]);
        model.vcs.changes.selected = Some(0);
        model.vcs.changes.expanded = Some((std::path::PathBuf::from("/workspace/note.rs"), false));
        model.vcs.changes.diff = DiffState::Ready {
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
        assert!(content.contains("abc1234  current commit  Eggplant  just now"));
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
    fn help_uses_a_bordered_bottom_panel() {
        let mut model = Model::default();
        model.vcs.help_visible = true;
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| Vcs.view(&model, frame)).unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Help"));
        assert!(content.contains("Ctrl-s stage all"));
        assert!(content.contains("U unstage all"));
        assert!(content.contains("? / Esc close help"));
        assert!(content.contains("└"));
    }

    #[test]
    fn empty_state_uses_the_dark_theme_background() {
        let mut model = Model::default();
        model.vcs.changes.state = ChangesState::Ready(Vec::new());
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
