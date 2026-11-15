use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout},
    style::Modifier,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::{
    data_object::Model,
    ui::{StatusSlot, UiRegistry},
    vcs::HistoryState,
};

#[derive(Debug)]
pub struct Vcs {
    ui: UiRegistry,
}

impl Default for Vcs {
    fn default() -> Self {
        Self::new(UiRegistry::builtin())
    }
}

impl Vcs {
    pub fn new(ui: UiRegistry) -> Self {
        Self { ui }
    }

    pub fn view(&self, model: &Model, frame: &mut Frame) {
        let [main, status] = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)])
            .flex(Flex::Start)
            .areas(frame.area());
        let styles = model.theme.ui();
        let head = model.vcs.head().unwrap_or("--");
        let heading = styles
            .base
            .patch(styles.key_hint)
            .add_modifier(Modifier::BOLD);
        let mut lines = vec![
            Line::styled(format!("Git  {head}"), heading),
            Line::default(),
            Line::styled("Recent commits", heading),
        ];
        match model.vcs.history() {
            HistoryState::Idle | HistoryState::Loading => {
                lines.push(Line::styled("Loading history…", styles.muted));
            }
            HistoryState::Ready(commits) if commits.is_empty() => {
                lines.push(Line::styled("No commits", styles.muted));
            }
            HistoryState::Ready(commits) => {
                lines.extend(commits.iter().map(|commit| {
                    Line::from(vec![
                        Span::styled(format!("{}  ", commit.id), styles.key_hint),
                        Span::raw(commit.subject.clone()),
                    ])
                }));
            }
            HistoryState::Failed(error) => {
                lines.push(Line::styled(error.clone(), styles.error));
            }
        }
        frame.render_widget(Paragraph::new(lines).style(styles.base), main);
        self.ui
            .render_status(StatusSlot::Message, model, frame, status);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_oneline_history_on_the_full_page() {
        let mut model = Model::default();
        model.vcs.history = HistoryState::Ready(vec![
            ::vcs::CommitSummary {
                id: "abc1234".to_string(),
                subject: "second commit".to_string(),
            },
            ::vcs::CommitSummary {
                id: "def5678".to_string(),
                subject: "initial commit".to_string(),
            },
        ]);
        let backend = TestBackend::new(70, 10);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| Vcs::default().view(&model, frame))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Git"));
        assert!(content.contains("Recent commits"));
        assert!(content.contains("abc1234  second commit"));
        assert!(content.contains("def5678  initial commit"));
        assert!(!content.contains("NORMAL |"));
    }
}
