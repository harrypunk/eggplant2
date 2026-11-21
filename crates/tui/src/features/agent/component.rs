use eggplant_agent::{MessageRole, RunStatus};
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
            let size = match model.agent.session().pending_approval() {
                Some(_) => ratatui::layout::Size::new(90, 24),
                None => ratatui::layout::Size::new(70, 12),
            };
            OverlayContribution::new(
                "agent",
                OverlayKind::Modal,
                OverlayPlacement::Centered(size),
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
        model
            .agent
            .session()
            .pending_approval()
            .is_none()
            .then(|| {
                TextInput::new(model.agent.prompt(), model.theme.ui())
                    .cursor_position(input_area(area))
            })
            .flatten()
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

impl AgentDialogContent<'_> {
    fn render_prompt(self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        let [status, input, assistant] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .areas(area);
        let status_text = self
            .state
            .storage_error()
            .map(|error| format!("Storage warning: {error}"))
            .or_else(|| {
                self.state
                    .session()
                    .run()
                    .map(|run| run.status.label().to_string())
            })
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

    fn render_approval(
        self,
        approval: &eggplant_agent::ToolApproval,
        area: Rect,
        buffer: &mut ratatui::buffer::Buffer,
    ) {
        let [status, summary, diff, hints] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        Paragraph::new("Waiting for write approval")
            .style(self.styles.action_hint)
            .render(status, buffer);
        Paragraph::new(approval.summary.clone())
            .wrap(Wrap { trim: false })
            .style(self.styles.base)
            .render(summary, buffer);
        Paragraph::new(diff_lines(&approval.diff, self.styles))
            .wrap(Wrap { trim: false })
            .style(self.styles.base)
            .render(diff, buffer);
        Paragraph::new("y approve once · n reject · Esc hide · Ctrl-F7 cancel")
            .style(self.styles.muted)
            .render(hints, buffer);
    }
}

impl Widget for AgentDialogContent<'_> {
    fn render(self, area: Rect, buffer: &mut ratatui::buffer::Buffer) {
        match self.state.session().pending_approval() {
            Some(approval) => self.render_approval(approval, area, buffer),
            None => self.render_prompt(area, buffer),
        }
    }
}

fn diff_lines(diff: &str, styles: &UiTheme) -> Vec<Line<'static>> {
    diff.lines()
        .map(|line| {
            let style = match line {
                line if line.starts_with('+') => styles.diff_add,
                line if line.starts_with('-') => styles.diff_remove,
                line if line.starts_with("@@") => styles.diff_hunk,
                _ => styles.muted,
            };
            Line::styled(line.to_string(), style)
        })
        .collect()
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
        state: &AgentState,
        focus: FocusTarget,
        styles: &UiTheme,
        frame: &mut Frame,
        area: Rect,
    ) {
        let session = state.session();
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
        let status_text = state
            .storage_error()
            .map(|error| format!("Storage warning: {error}"))
            .or_else(|| {
                session.run().map(|run| match &run.status {
                    RunStatus::Failed(error) => format!("Failed: {error}"),
                    status => status.label().to_string(),
                })
            })
            .unwrap_or_else(|| "No active run".to_string());
        Paragraph::new(status_text)
            .style(styles.muted)
            .render(status, frame.buffer_mut());

        let entries = session.messages();
        let start = entries
            .len()
            .saturating_sub(messages.height.max(1) as usize);
        let mut lines = if entries.is_empty() {
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
        lines.extend(session.tool_events().iter().map(|event| {
            let summary = match &event.result {
                Some(result) => {
                    let status = match result.status {
                        eggplant_agent::ToolResultStatus::Success => "ok",
                        eggplant_agent::ToolResultStatus::Error => "error",
                        eggplant_agent::ToolResultStatus::Rejected => "rejected",
                        eggplant_agent::ToolResultStatus::Cancelled => "cancelled",
                    };
                    let preview = result
                        .content
                        .lines()
                        .take(2)
                        .collect::<Vec<_>>()
                        .join(" · ")
                        .chars()
                        .take(240)
                        .collect::<String>();
                    let truncated = if result.truncated {
                        " · truncated"
                    } else {
                        ""
                    };
                    format!("tool {}: {status} · {preview}{truncated}", event.name)
                }
                None => format!("tool {}: pending", event.name),
            };
            Line::styled(summary, styles.muted)
        }));
        lines.extend(session.pending_approval().into_iter().flat_map(|approval| {
            std::iter::once(Line::styled(
                format!("Approval: {}", approval.summary),
                styles.action_hint,
            ))
            .chain(diff_lines(&approval.diff, styles))
            .chain(std::iter::once(Line::styled(
                "Resolve in the Agent popup: y approve · n reject",
                styles.muted,
            )))
        }));
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
                    &model.agent,
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

    #[test]
    fn dialog_and_panel_render_the_same_bounded_write_approval() {
        let mut model = Model::default();
        let buffer = create_buffer(&mut model, "before");
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
            .begin("change".to_string())
            .unwrap();
        let call = eggplant_agent::ToolCall {
            id: "write-1".to_string(),
            name: "write".to_string(),
            arguments: "{}".to_string(),
        };
        model.agent.session_mut().record_tool_call(run, &call);
        model.agent.session_mut().request_approval(
            run,
            eggplant_agent::ToolApproval {
                call,
                summary: "Replace text in main.rs".to_string(),
                diff: "--- main.rs\n+++ main.rs\n@@ exact replacement @@\n-before\n+after\n"
                    .to_string(),
            },
        );

        let mut dialog = Terminal::new(TestBackend::new(90, 24)).unwrap();
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
        assert!(dialog_content.contains("Waiting for write approval"));
        assert!(dialog_content.contains("before"));
        assert!(dialog_content.contains("after"));
        assert!(AgentDialog::cursor(&model, ratatui::layout::Rect::new(0, 0, 90, 24)).is_none());

        let mut panel = Terminal::new(TestBackend::new(80, 20)).unwrap();
        panel
            .draw(|frame| {
                AgentPanel::render(
                    &model.agent,
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
        assert!(panel_content.contains("Approval: Replace text in main.rs"));
        assert!(panel_content.contains("before"));
        assert!(panel_content.contains("after"));
    }
}
