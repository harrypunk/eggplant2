use eggplant_agent::{MessageRole, ToolApproval};
use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    widgets::{Paragraph, Widget, Wrap},
};

use super::{prompt::AgentPromptView, props::AgentViewProps, transcript::diff_lines};
use crate::component::{
    auto_height::AutoHeight,
    popup::{Popup, content_area},
    text_input::TextInput,
    text_layout::TextFlow,
};

const AUTO_HEIGHT: AutoHeight = AutoHeight::new(12, 24, 2, 9, 4);

pub type AgentDialogProps<'a> = AgentViewProps<'a>;

pub struct AgentDialog;

impl AgentDialog {
    pub fn size(props: &AgentDialogProps<'_>) -> Size {
        if props.session.pending_approval().is_some() {
            return Size::new(90, 24);
        }
        let mut size = AUTO_HEIGHT.dialog_size(props.prompt.input(), 68, TextFlow::Wrapped);
        size.height = size.height.saturating_add(command_height(props)).min(24);
        size
    }

    pub fn render(props: &AgentDialogProps<'_>, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Popup::new("AI agent", AgentDialogContent { props }, props.styles),
            area,
        );
    }

    pub fn cursor(props: &AgentDialogProps<'_>, area: Rect) -> Option<Position> {
        if props.session.pending_approval().is_some() {
            return None;
        }
        AgentPromptView::cursor(&props.prompt, props.styles, input_area(props, area))
    }
}

struct AgentDialogContent<'a> {
    props: &'a AgentDialogProps<'a>,
}

impl AgentDialogContent<'_> {
    fn render_prompt(&self, area: Rect, buffer: &mut Buffer) {
        let input_height = prompt_input_height(self.props.prompt.input(), area);
        let [status, assistant, prompt] = dialog_layout(area, input_height);
        Paragraph::new(self.status_text())
            .style(self.props.styles.muted)
            .render(status, buffer);
        Paragraph::new(self.latest_assistant_text())
            .wrap(Wrap { trim: false })
            .style(self.props.styles.base)
            .render(assistant, buffer);
        AgentPromptView::render(
            &self.props.prompt,
            self.props.styles,
            prompt,
            input_height,
            buffer,
        );
    }

    fn status_text(&self) -> String {
        self.props
            .storage_error
            .map(|error| format!("Storage warning: {error}"))
            .or_else(|| {
                self.props
                    .session
                    .run()
                    .map(|run| run.status.label().to_string())
            })
            .unwrap_or_else(|| "Ready".to_string())
    }

    fn latest_assistant_text(&self) -> &str {
        self.props
            .session
            .messages()
            .iter()
            .rev()
            .find(|message| message.role == MessageRole::Assistant)
            .map(|message| message.text.as_str())
            .unwrap_or("")
    }

    fn render_approval(&self, approval: &ToolApproval, area: Rect, buffer: &mut Buffer) {
        let [status, summary, diff, hints] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        Paragraph::new("Waiting for write approval")
            .style(self.props.styles.action_hint)
            .render(status, buffer);
        Paragraph::new(approval.summary.clone())
            .wrap(Wrap { trim: false })
            .style(self.props.styles.base)
            .render(summary, buffer);
        Paragraph::new(diff_lines(&approval.diff, self.props.styles))
            .wrap(Wrap { trim: false })
            .style(self.props.styles.base)
            .render(diff, buffer);
        Paragraph::new("y approve once · n reject · Esc hide · Ctrl-F7 cancel")
            .style(self.props.styles.muted)
            .render(hints, buffer);
    }
}

impl Widget for AgentDialogContent<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        match self.props.session.pending_approval() {
            Some(approval) => self.render_approval(approval, area, buffer),
            None => self.render_prompt(area, buffer),
        }
    }
}

fn command_height(props: &AgentDialogProps<'_>) -> u16 {
    AgentPromptView::command_height(&props.prompt)
}

fn dialog_layout(area: Rect, input: u16) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(input),
    ])
    .areas(area)
}

fn prompt_input_height(prompt: &TextInput, area: Rect) -> u16 {
    AUTO_HEIGHT.input_height(prompt, area, TextFlow::Wrapped)
}

fn prompt_area(props: &AgentDialogProps<'_>, area: Rect) -> Rect {
    dialog_layout(area, prompt_input_height(props.prompt.input(), area))[2]
}

fn input_area(props: &AgentDialogProps<'_>, area: Rect) -> Rect {
    prompt_area(props, content_area(area))
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{component::agent::AgentPromptProps, theme::Theme};
    use eggplant_agent::AgentSession;

    fn render(props: &AgentDialogProps<'_>, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| AgentDialog::render(props, frame, frame.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn renders_conversation_above_the_prompt() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.append_delta(run, "answer");
        let prompt = TextInput::new("main.rs:1");
        let props = AgentDialogProps {
            session: &session,
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            styles: theme.ui(),
        };
        let content = render(&props, 70, 12);
        assert!(content.contains("main.rs:1"));
        assert!(content.contains("Streaming"));
        assert!(content.contains("answer"));
    }

    #[test]
    fn approval_replaces_prompt_and_hides_cursor() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("change".to_string()).unwrap();
        let call = eggplant_agent::ToolCall {
            id: "write-1".to_string(),
            name: "write".to_string(),
            arguments: "{}".to_string(),
        };
        session.record_tool_call(run, &call);
        session.request_approval(
            run,
            ToolApproval {
                call,
                summary: "Replace text in main.rs".to_string(),
                diff: "--- main.rs\n+++ main.rs\n@@ exact replacement @@\n-before\n+after\n"
                    .to_string(),
            },
        );
        let prompt = TextInput::default();
        let props = AgentDialogProps {
            session: &session,
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            styles: theme.ui(),
        };
        let area = Rect::new(0, 0, 90, 24);
        let content = render(&props, area.width, area.height);
        assert!(content.contains("Waiting for write approval"));
        assert!(content.contains("before"));
        assert!(content.contains("after"));
        assert!(AgentDialog::cursor(&props, area).is_none());
    }

    #[test]
    fn wrapped_prompt_grows_dialog_and_keeps_cursor_visible() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::new("ab".repeat(100));
        let props = AgentDialogProps {
            session: &session,
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            styles: theme.ui(),
        };
        let size = AgentDialog::size(&props);
        assert_eq!(size, Size::new(70, 15));
        let area = Rect::new(0, 0, size.width, size.height);
        let cursor = AgentDialog::cursor(&props, area).unwrap();
        assert!(cursor.x < area.width && cursor.y < area.height);
        assert!(prompt_input_height(&prompt, content_area(area)) <= content_area(area).height - 4);
        render(&props, size.width, size.height);
    }
}
