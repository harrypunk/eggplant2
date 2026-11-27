use eggplant_agent::RunStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    text::Line,
    widgets::{Block, BorderType, Paragraph, Widget},
};

use super::{
    footer::{AgentFooter, footer_layout},
    prompt::AgentPromptView,
    props::AgentViewProps,
    transcript::render_transcript as render_agent_transcript,
};
use crate::{
    agent::AgentInputMode,
    component::{auto_height::AutoHeight, text_layout::TextFlow},
    data_object::FocusState,
};

const AUTO_HEIGHT: AutoHeight = AutoHeight::new(3, 8, 2, 2, 4);

#[derive(Clone)]
pub struct AgentPanelProps<'a> {
    pub shared: AgentViewProps<'a>,
    pub mode: AgentInputMode,
    pub focus: FocusState,
}

pub struct AgentPanel;

impl AgentPanel {
    pub fn render(props: &AgentPanelProps<'_>, frame: &mut Frame, area: Rect) {
        let inner = render_chrome(props, frame, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        let [content, footer] = footer_layout(inner);
        let input_height = input_height(props, content);
        let completion_height = AgentPromptView::completion_height(&props.shared.prompt).min(
            content
                .height
                .saturating_sub(input_height.saturating_add(1)),
        );
        let prompt_height = completion_height + input_height;
        let [status, transcript, prompt] = panel_layout(content, prompt_height);
        render_status(props, status, frame);
        render_transcript(props, transcript, frame);
        render_prompt(props, prompt, input_height, frame);
        AgentFooter::render(&props.shared, footer, frame.buffer_mut());
    }
}

fn render_chrome(props: &AgentPanelProps<'_>, frame: &mut Frame, area: Rect) -> Rect {
    let focused = props.focus == FocusState::Focused;
    let (border_style, border_type, title_style) = if focused {
        (
            props.shared.styles.focused_border,
            BorderType::Double,
            props.shared.styles.action_hint,
        )
    } else {
        (
            props.shared.styles.border,
            BorderType::Plain,
            props.shared.styles.muted,
        )
    };
    let title = if focused {
        format!(" Agent ● {} ", props.mode.label())
    } else {
        " Agent ".to_string()
    };
    let block = Block::bordered()
        .title(Line::styled(
            title,
            props.shared.styles.base.patch(title_style),
        ))
        .border_type(border_type)
        .border_style(props.shared.styles.base.patch(border_style))
        .style(props.shared.styles.base);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn input_height(props: &AgentPanelProps<'_>, area: Rect) -> u16 {
    match props.shared.session.pending_approval() {
        Some(_) => 0,
        None => AUTO_HEIGHT.input_height(props.shared.prompt.input(), area, TextFlow::Wrapped),
    }
}

fn panel_layout(area: Rect, prompt: u16) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(prompt),
    ])
    .areas(area)
}

fn render_status(props: &AgentPanelProps<'_>, area: Rect, frame: &mut Frame) {
    Paragraph::new(status_text(props))
        .style(props.shared.styles.muted)
        .render(area, frame.buffer_mut());
}

fn status_text(props: &AgentPanelProps<'_>) -> String {
    let status = props
        .shared
        .model_notice
        .map(|notice| notice.text().to_string())
        .or_else(|| {
            props
                .shared
                .storage_error
                .map(|error| format!("Storage warning: {error}"))
        })
        .or_else(|| {
            props.shared.session.run().map(|run| match &run.status {
                RunStatus::Failed(error) => format!("Failed: {error}"),
                status => status.label().to_string(),
            })
        })
        .unwrap_or_else(|| "No active run".to_string());
    format!("{status} · {}", props.mode.label())
}

fn render_transcript(props: &AgentPanelProps<'_>, area: Rect, frame: &mut Frame) {
    render_agent_transcript(
        props.shared.session,
        area,
        props.shared.styles,
        frame.buffer_mut(),
    );
}

fn render_prompt(props: &AgentPanelProps<'_>, area: Rect, input_height: u16, frame: &mut Frame) {
    if area.height == 0 {
        return;
    }
    let input = AgentPromptView::render(
        &props.shared.prompt,
        props.shared.styles,
        area,
        input_height,
        frame.buffer_mut(),
    );
    if let Some(cursor) = prompt_cursor(props, input) {
        frame.set_cursor_position(cursor);
    }
}

fn prompt_cursor(props: &AgentPanelProps<'_>, area: Rect) -> Option<Position> {
    if props.focus != FocusState::Focused || props.mode != AgentInputMode::Insert {
        return None;
    }
    AgentPromptView::cursor(&props.shared.prompt, props.shared.styles, area)
}

#[cfg(test)]
mod tests {
    use eggplant_agent::ToolApproval;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::component::{
        agent::{AgentModelNotice, AgentPromptProps},
        text_input::TextInput,
    };
    use crate::{
        agent::PromptCommand,
        theme::{Theme, UiTheme},
    };
    use eggplant_agent::AgentSession;

    fn panel_props<'a>(
        session: &'a AgentSession,
        prompt: &'a TextInput,
        commands: Option<Vec<PromptCommand>>,
        mode: AgentInputMode,
        focus: FocusState,
        styles: &'a UiTheme,
    ) -> AgentPanelProps<'a> {
        AgentPanelProps {
            shared: AgentViewProps {
                session,
                prompt: AgentPromptProps::new(prompt).with_commands(commands, None),
                storage_error: None,
                model_notice: None,
                footer: Default::default(),
                styles,
            },
            mode,
            focus,
        }
    }

    fn render(props: &AgentPanelProps<'_>, width: u16, height: u16) -> Vec<String> {
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
        let props = panel_props(
            &session,
            &prompt,
            None,
            AgentInputMode::Insert,
            FocusState::Focused,
            theme.ui(),
        );
        let rows = render(&props, 50, 12);
        let content = rows.join("\n");
        assert!(content.contains("You: question"), "{content}");
        assert!(content.contains("Agent: answer"));
        let answer_row = rows
            .iter()
            .position(|row| row.contains("Agent: answer"))
            .unwrap();
        let prompt_row = rows.iter().rposition(|row| row.contains('x')).unwrap();
        assert!(answer_row < prompt_row);
        let footer_row = rows
            .iter()
            .rposition(|row| row.contains("unknown · no model"))
            .unwrap();
        assert_eq!(footer_row, rows.len() - 2);
    }

    #[test]
    fn assistant_markdown_is_rendered_without_parsing_user_prompt_markup() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("show **raw**".to_string()).unwrap();
        session.append_delta(run, "# Result\n\n**bold** and `code`\n\n- one\n- two");
        let prompt = TextInput::default();
        let props = panel_props(
            &session,
            &prompt,
            None,
            AgentInputMode::Normal,
            FocusState::Focused,
            theme.ui(),
        );

        let content = render(&props, 60, 22).join("\n");
        assert!(content.contains("You: show **raw**"));
        assert!(content.contains("Agent: Result"));
        assert!(content.contains("bold"));
        assert!(content.contains("code"));
        assert!(content.contains("• one"));
        assert!(!content.contains("**bold**"));
        assert!(!content.contains("# Result"));
    }

    #[test]
    fn wrapped_markdown_transcript_stays_bottom_anchored() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.append_delta(
            run,
            &format!(
                "start-marker\n\n{}\n\nfinal-marker",
                "long wrapped text ".repeat(30)
            ),
        );
        let prompt = TextInput::default();
        let props = panel_props(
            &session,
            &prompt,
            None,
            AgentInputMode::Normal,
            FocusState::Focused,
            theme.ui(),
        );

        let content = render(&props, 44, 12).join("\n");
        assert!(content.contains("final-marker"), "{content}");
        assert!(!content.contains("start-marker"), "{content}");
    }

    #[test]
    fn no_model_hint_is_rendered_with_login_guidance() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::default();
        let mut props = panel_props(
            &session,
            &prompt,
            None,
            AgentInputMode::Normal,
            FocusState::Focused,
            theme.ui(),
        );
        props.shared.model_notice = Some(AgentModelNotice::Unavailable);

        let content = render(&props, 70, 12).join("\n");

        assert!(content.contains("No model available"));
        assert!(content.contains("/provider"));
        assert!(content.contains("Space a i"));
    }

    #[test]
    fn command_menu_sits_directly_above_the_input() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::new("/p");
        let props = panel_props(
            &session,
            &prompt,
            Some(vec![PromptCommand::ProviderLogin]),
            AgentInputMode::Insert,
            FocusState::Focused,
            theme.ui(),
        );
        let rows = render(&props, 50, 12);
        let command_row = rows
            .iter()
            .position(|row| row.contains("/provider"))
            .unwrap();
        let prompt_row = rows.iter().rposition(|row| row.contains("/p")).unwrap();
        assert!(command_row < prompt_row);
        let [command_area, input_area] =
            AgentPromptView::layout(&props.shared.prompt, Rect::new(0, 0, 50, 12), 3);
        assert_eq!(command_area.y + command_area.height, input_area.y);
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
            &panel_props(
                &session,
                &prompt,
                None,
                AgentInputMode::Normal,
                FocusState::Unfocused,
                theme.ui(),
            ),
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
        let props = |focus| {
            panel_props(
                &session,
                &prompt,
                None,
                AgentInputMode::Normal,
                focus,
                theme.ui(),
            )
        };
        let focused = render(&props(FocusState::Focused), 50, 10).join("\n");
        assert!(focused.contains("╔"));
        assert!(focused.contains("Agent ● Normal"));

        let unfocused = render(&props(FocusState::Unfocused), 50, 10).join("\n");
        assert!(unfocused.contains("┌"));
        assert!(!unfocused.contains("Agent ● Normal"));
    }
}
