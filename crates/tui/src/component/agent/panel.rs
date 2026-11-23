use eggplant_agent::{AgentSession, RunStatus};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget, Wrap},
};

use super::transcript::transcript_lines;
use crate::{
    agent::AgentInputMode,
    component::{auto_height::AutoHeight, text_input::TextInput, text_layout::TextFlow},
    data_object::FocusState,
    theme::UiTheme,
};

const AUTO_HEIGHT: AutoHeight = AutoHeight::new(3, 8, 2, 2, 4);

#[derive(Clone, Copy)]
pub struct AgentPanelProps<'a> {
    pub session: &'a AgentSession,
    pub prompt: &'a TextInput,
    pub mode: AgentInputMode,
    pub storage_error: Option<&'a str>,
    pub focus: FocusState,
    pub styles: &'a UiTheme,
}

pub struct AgentPanel;

impl AgentPanel {
    pub fn render(props: AgentPanelProps<'_>, frame: &mut Frame, area: Rect) {
        let inner = render_chrome(props, frame, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let input_height = input_height(props, inner);
        let [status, transcript, input] = panel_layout(inner, input_height);
        render_status(props, status, frame);
        render_transcript(props, transcript, frame);
        render_prompt(props, input, frame);
    }
}

fn render_chrome(props: AgentPanelProps<'_>, frame: &mut Frame, area: Rect) -> Rect {
    let focused = props.focus == FocusState::Focused;
    let (border_style, border_type, title_style) = if focused {
        (
            props.styles.focused_border,
            BorderType::Double,
            props.styles.action_hint,
        )
    } else {
        (props.styles.border, BorderType::Plain, props.styles.muted)
    };
    let title = if focused {
        format!(" Agent ● {} ", props.mode.label())
    } else {
        " Agent ".to_string()
    };
    let block = Block::bordered()
        .title(Line::styled(title, props.styles.base.patch(title_style)))
        .border_type(border_type)
        .border_style(props.styles.base.patch(border_style))
        .style(props.styles.base);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn input_height(props: AgentPanelProps<'_>, area: Rect) -> u16 {
    match props.session.pending_approval() {
        Some(_) => 0,
        None => AUTO_HEIGHT.input_height(props.prompt, area, TextFlow::Wrapped),
    }
}

fn panel_layout(area: Rect, input_height: u16) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(input_height),
    ])
    .areas(area)
}

fn render_status(props: AgentPanelProps<'_>, area: Rect, frame: &mut Frame) {
    Paragraph::new(status_text(props))
        .style(props.styles.muted)
        .render(area, frame.buffer_mut());
}

fn status_text(props: AgentPanelProps<'_>) -> String {
    let status = props
        .storage_error
        .map(|error| format!("Storage warning: {error}"))
        .or_else(|| {
            props.session.run().map(|run| match &run.status {
                RunStatus::Failed(error) => format!("Failed: {error}"),
                status => status.label().to_string(),
            })
        })
        .unwrap_or_else(|| "No active run".to_string());
    format!("{status} · {}", props.mode.label())
}

fn render_transcript(props: AgentPanelProps<'_>, area: Rect, frame: &mut Frame) {
    Paragraph::new(transcript_lines(props.session, area.height, props.styles))
        .wrap(Wrap { trim: false })
        .style(props.styles.base)
        .render(area, frame.buffer_mut());
}

fn render_prompt(props: AgentPanelProps<'_>, area: Rect, frame: &mut Frame) {
    if area.height == 0 {
        return;
    }
    props
        .prompt
        .view_with_flow(props.styles, TextFlow::Wrapped)
        .render(area, frame.buffer_mut());
    if let Some(cursor) = prompt_cursor(props, area) {
        frame.set_cursor_position(cursor);
    }
}

fn prompt_cursor(props: AgentPanelProps<'_>, area: Rect) -> Option<Position> {
    if props.focus != FocusState::Focused || props.mode != AgentInputMode::Insert {
        return None;
    }
    props
        .prompt
        .cursor_position_with_flow(area, props.styles, TextFlow::Wrapped)
}

#[cfg(test)]
mod tests {
    use eggplant_agent::ToolApproval;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    fn render(props: AgentPanelProps<'_>, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| AgentPanel::render(props, frame, frame.area()))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .chunks(width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect())
            .collect()
    }

    #[test]
    fn conversation_and_tools_precede_the_bottom_input() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.append_delta(run, "answer");
        let prompt = TextInput::new("x".repeat(100));
        let props = AgentPanelProps {
            session: &session,
            prompt: &prompt,
            mode: AgentInputMode::Insert,
            storage_error: None,
            focus: FocusState::Focused,
            styles: theme.ui(),
        };
        let rows = render(props, 50, 12);
        let content = rows.join("\n");
        assert!(content.contains("You: question"));
        assert!(content.contains("Agent: answer"));
        let answer_row = rows
            .iter()
            .position(|row| row.contains("Agent: answer"))
            .unwrap();
        let prompt_row = rows.iter().rposition(|row| row.contains('x')).unwrap();
        assert!(answer_row < prompt_row);
    }

    #[test]
    fn renders_bounded_write_approval_without_an_input() {
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
        let rows = render(
            AgentPanelProps {
                session: &session,
                prompt: &prompt,
                mode: AgentInputMode::Normal,
                storage_error: None,
                focus: FocusState::Unfocused,
                styles: theme.ui(),
            },
            80,
            20,
        );
        let content = rows.join("\n");
        assert!(content.contains("Approval: Replace text in main.rs"));
        assert!(content.contains("before"));
        assert!(content.contains("after"));
    }

    #[test]
    fn focus_changes_the_border_and_title() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::default();
        let props = |focus| AgentPanelProps {
            session: &session,
            prompt: &prompt,
            mode: AgentInputMode::Normal,
            storage_error: None,
            focus,
            styles: theme.ui(),
        };
        let focused = render(props(FocusState::Focused), 50, 10).join("\n");
        assert!(focused.contains("╔"));
        assert!(focused.contains("Agent ● Normal"));

        let unfocused = render(props(FocusState::Unfocused), 50, 10).join("\n");
        assert!(unfocused.contains("┌"));
        assert!(!unfocused.contains("Agent ● Normal"));
    }
}
