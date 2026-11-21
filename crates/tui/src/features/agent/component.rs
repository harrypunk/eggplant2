use eggplant_agent::{AgentSession, MessageRole, RunStatus};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget, Wrap},
};

use super::AgentState;
use crate::{
    component::{popup::Popup, popup::content_area, text_input::TextInput},
    data_object::{FocusTarget, Model},
    theme::UiTheme,
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

pub struct AgentDialog;

impl AgentDialog {
    pub fn overlay(model: &Model) -> Option<OverlayContribution> {
        model.agent.dialog_open().then(|| {
            OverlayContribution::new(
                "agent",
                OverlayKind::Modal,
                OverlayPlacement::Centered(ratatui::layout::Size::new(70, 12)),
                Self::render,
            )
            .with_cursor(Self::cursor)
        })
    }

    pub fn render(model: &Model, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Popup::new(
                "AI agent",
                AgentDialogContent::new(&model.agent, model.theme.ui()),
                model.theme.ui(),
            ),
            area,
        );
    }

    pub fn cursor(model: &Model, area: Rect) -> Option<Position> {
        TextInput::new(model.agent.prompt(), model.theme.ui()).cursor_position(input_area(area))
    }
}

struct AgentDialogContent<'a> {
    state: &'a AgentState,
    styles: &'a UiTheme,
}

impl<'a> AgentDialogContent<'a> {
    fn new(state: &'a AgentState, styles: &'a UiTheme) -> Self {
        Self { state, styles }
    }
}

impl Widget for AgentDialogContent<'_> {
    fn render(self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let [status, input, assistant] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(area);
        let status_text = self
            .state
            .session()
            .run()
            .map(|run| run.status.label().to_string())
            .unwrap_or_else(|| "Ready".to_string());
        Paragraph::new(status_text)
            .style(self.styles.muted)
            .render(status, buffer);
        TextInput::new(self.state.prompt(), self.styles).render(input, buffer);
        let text = self
            .state
            .session()
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant)
            .map(|message| message.text.as_str())
            .unwrap_or("Ask about this buffer.");
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .style(self.styles.base)
            .render(assistant, buffer);
    }
}

fn input_area(area: Rect) -> Rect {
    let [_, input, _] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(content_area(area));
    input
}

pub struct AgentPanel;

impl AgentPanel {
    pub fn render(
        session: &AgentSession,
        focus: FocusTarget,
        styles: &UiTheme,
        frame: &mut Frame,
        area: Rect,
    ) {
        let border_style = if focus == FocusTarget::ToolsPanel {
            styles.focused_border
        } else {
            styles.border
        };
        let block = Block::bordered()
            .title("Agent")
            .border_style(styles.base.patch(border_style))
            .style(styles.base);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let [status, messages] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
        let status_text = session
            .run()
            .map(|run| match &run.status {
                RunStatus::Failed(error) => format!("Failed: {error}"),
                status => status.label().to_string(),
            })
            .unwrap_or_else(|| "No active run".to_string());
        Paragraph::new(status_text)
            .style(styles.muted)
            .render(status, frame.buffer_mut());

        let entries = session.messages();
        let start = entries
            .len()
            .saturating_sub(messages.height.max(1) as usize);
        let lines = if entries.is_empty() {
            vec![Line::styled("No conversation yet", styles.muted)]
        } else {
            entries[start..]
                .iter()
                .map(|message| {
                    let (prefix, style) = match message.role {
                        MessageRole::User => ("You: ", styles.muted),
                        MessageRole::Assistant => ("Agent: ", styles.action_hint),
                    };
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::raw(message.text.clone()),
                    ])
                })
                .collect::<Vec<_>>()
        };
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .style(styles.base)
            .render(messages, frame.buffer_mut());
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::test_utils::create_buffer;

    #[test]
    fn dialog_and_panel_share_the_same_conversation() {
        let mut model = Model::default();
        let buffer = create_buffer(&mut model, "fn main() {}\n");
        model.agent.open_dialog(
            crate::agent::EditorContext {
                buffer,
                path: std::path::PathBuf::from("/workspace/main.rs"),
                revision: 1,
                line: 1,
            },
            "main.rs:1".to_string(),
        );
        let run = model
            .agent
            .session_mut()
            .begin("question".to_string())
            .unwrap();
        model.agent.session_mut().append_delta(run, "answer");

        let dialog_backend = TestBackend::new(70, 12);
        let mut dialog = Terminal::new(dialog_backend).unwrap();
        dialog
            .draw(|frame| AgentDialog::render(&model, frame, frame.area()))
            .unwrap();
        let dialog_content = dialog
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(dialog_content.contains("main.rs:1"));
        assert!(dialog_content.contains("Streaming"));

        let panel_backend = TestBackend::new(50, 10);
        let mut panel = Terminal::new(panel_backend).unwrap();
        panel
            .draw(|frame| {
                AgentPanel::render(
                    model.agent.session(),
                    model.current_focus,
                    model.theme.ui(),
                    frame,
                    frame.area(),
                );
            })
            .unwrap();
        let panel_content = panel
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(panel_content.contains("You: question"));
        assert!(panel_content.contains("Agent: answer"));
    }
}
