use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect},
    widgets::{Paragraph, Widget},
};

use super::{
    footer::{AgentFooter, footer_layout},
    navigation::ViewportMetrics,
    prompt::AgentPromptView,
    props::AgentViewProps,
    status::{RunStatusPresentation, status_text},
    transcript::{latest_assistant_metrics, render_latest_assistant},
};
use crate::component::{
    popup::{Popup, content_area},
    text_input::TextInput,
    text_layout::TextFlow,
};

const STATUS_HEIGHT: u16 = 1;
const MIN_ASSISTANT_HEIGHT: u16 = 3;

pub type AgentDialogProps<'a> = AgentViewProps<'a>;

pub struct AgentDialog;

impl AgentDialog {
    pub fn render(props: &AgentDialogProps<'_>, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Popup::new("AI agent", AgentDialogContent { props }, props.styles),
            area,
        );
    }

    pub fn cursor(props: &AgentDialogProps<'_>, area: Rect) -> Option<Position> {
        AgentPromptView::cursor(&props.prompt, props.styles, input_area(props, area))
    }

    pub fn transcript_metrics(props: &AgentDialogProps<'_>, area: Rect) -> ViewportMetrics {
        latest_assistant_metrics(props.session, assistant_area(props, area), props.styles)
    }
}

struct AgentDialogContent<'a> {
    props: &'a AgentDialogProps<'a>,
}

impl AgentDialogContent<'_> {
    fn render_prompt(&self, area: Rect, buffer: &mut Buffer) {
        let (input_height, prompt_height) = prompt_heights(self.props, area);
        let [status, assistant, prompt] = dialog_layout(area, prompt_height);
        Paragraph::new(status_text(
            self.props,
            RunStatusPresentation::Summary,
            "Ready",
        ))
        .style(self.props.styles.muted)
        .render(status, buffer);
        render_latest_assistant(
            self.props.session,
            assistant,
            self.props.styles,
            self.props.navigation.dialog(),
            buffer,
        );
        AgentPromptView::render(
            &self.props.prompt,
            self.props.styles,
            prompt,
            input_height,
            buffer,
        );
    }
}

impl Widget for AgentDialogContent<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        let [content, footer] = footer_layout(area);
        self.render_prompt(content, buffer);
        AgentFooter::render(self.props, footer, buffer);
    }
}

fn completion_height(props: &AgentDialogProps<'_>) -> u16 {
    AgentPromptView::completion_height(&props.prompt)
}

fn dialog_layout(area: Rect, prompt: u16) -> [Rect; 3] {
    Layout::vertical([
        Constraint::Length(STATUS_HEIGHT),
        Constraint::Fill(1),
        Constraint::Length(prompt),
    ])
    .areas(area)
}

fn prompt_input_height(prompt: &TextInput, area: Rect) -> u16 {
    prompt.measure(area.width, TextFlow::Wrapped).height.min(
        area.height
            .saturating_sub(STATUS_HEIGHT + MIN_ASSISTANT_HEIGHT),
    )
}

/// Measure the combined menu + input prompt and bound the menu to the actual dialog viewport.
fn prompt_heights(props: &AgentDialogProps<'_>, area: Rect) -> (u16, u16) {
    let input = prompt_input_height(props.prompt.input(), area);
    let completion =
        completion_height(props).min(area.height.saturating_sub(input + STATUS_HEIGHT));
    (input, completion.saturating_add(input))
}

fn prompt_area(props: &AgentDialogProps<'_>, area: Rect) -> Rect {
    let (_, prompt) = prompt_heights(props, area);
    dialog_layout(area, prompt)[2]
}

fn assistant_area(props: &AgentDialogProps<'_>, area: Rect) -> Rect {
    let [content, _] = footer_layout(content_area(area));
    let (_, prompt) = prompt_heights(props, content);
    dialog_layout(content, prompt)[1]
}

fn input_area(props: &AgentDialogProps<'_>, area: Rect) -> Rect {
    let [content, _] = footer_layout(content_area(area));
    let (input, _) = prompt_heights(props, content);
    AgentPromptView::layout(&props.prompt, prompt_area(props, content), input)[1]
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        component::agent::{AgentModelNotice, AgentPromptProps, SurfaceNavigation},
        theme::Theme,
    };
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
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: Default::default(),
            styles: theme.ui(),
        };
        let content = render(&props, 70, 12);
        assert!(content.contains("main.rs:1"));
        assert!(content.contains("Streaming"));
        assert!(content.contains("answer"));
        assert!(content.contains("context ?"));
    }

    #[test]
    fn renders_the_latest_assistant_message_as_markdown() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.append_delta(run, "## Summary\n\n```rust\nfn main() {}\n```");
        let prompt = TextInput::default();
        let props = AgentDialogProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: Default::default(),
            styles: theme.ui(),
        };

        let content = render(&props, 70, 20);
        assert!(content.contains("Summary"));
        assert!(content.contains("fn main()"));
        assert!(!content.contains("## Summary"));
        assert!(!content.contains("```rust"));
    }

    #[test]
    fn renders_thinking_before_the_latest_answer() {
        let theme = Theme::default();
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.append_thinking_delta(run, "plan");
        session.append_delta(run, "answer");
        let prompt = TextInput::default();
        let props = AgentDialogProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: Default::default(),
            styles: theme.ui(),
        };

        let content = render(&props, 70, 20);
        assert!(content.contains("Thinking"));
        assert!(content.contains("plan"));
        assert!(content.contains("answer"));
    }

    #[test]
    fn no_model_hint_is_rendered_with_login_guidance() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::default();
        let props = AgentDialogProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: Some(AgentModelNotice::Unavailable),
            footer: Default::default(),
            styles: theme.ui(),
        };

        let content = render(&props, 70, 12);

        assert!(content.contains("No model available"));
        assert!(content.contains("/provider"));
        assert!(content.contains("Space a i"));
    }

    #[test]
    fn command_menu_has_real_rows_and_cursor_stays_in_the_input() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::new("/");
        let props = AgentDialogProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt).with_commands(
                Some(crate::agent::catalog()),
                Some(crate::agent::resolve("/logout").unwrap()),
            ),
            storage_error: None,
            model_notice: None,
            footer: Default::default(),
            styles: theme.ui(),
        };
        let area = Rect::new(0, 0, 90, 24);
        let input = input_area(&props, area);
        let [content, _] = footer_layout(content_area(area));
        let [commands, laid_out_input] =
            AgentPromptView::layout(&props.prompt, prompt_area(&props, content), input.height);

        assert_eq!(commands.height, crate::agent::catalog().len() as u16);
        assert_eq!(commands.y + commands.height, laid_out_input.y);
        assert_eq!(laid_out_input, input);
        let cursor = AgentDialog::cursor(&props, area).unwrap();
        assert!(cursor.y >= input.y && cursor.y < input.y + input.height);
        render(&props, area.width, area.height);
    }

    #[test]
    fn assistant_area_flexes_with_the_available_dialog_height() {
        let compact = dialog_layout(Rect::new(0, 0, 90, 16), 5);
        let expanded = dialog_layout(Rect::new(0, 0, 90, 28), 5);

        assert_eq!(compact[0].height, STATUS_HEIGHT);
        assert_eq!(compact[1].height, 10);
        assert_eq!(expanded[1].height, 22);
        assert_eq!(compact[1].y + compact[1].height, compact[2].y);
        assert_eq!(expanded[1].y + expanded[1].height, expanded[2].y);
    }

    #[test]
    fn wrapped_prompt_keeps_cursor_visible_in_a_flexible_dialog() {
        let theme = Theme::default();
        let session = AgentSession::default();
        let prompt = TextInput::new("ab".repeat(100));
        let props = AgentDialogProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: Default::default(),
            styles: theme.ui(),
        };
        let area = Rect::new(0, 0, 90, 24);
        let cursor = AgentDialog::cursor(&props, area).unwrap();
        assert!(cursor.x < area.width && cursor.y < area.height);
        assert!(
            prompt_input_height(&prompt, content_area(area))
                <= content_area(area).height - (STATUS_HEIGHT + MIN_ASSISTANT_HEIGHT)
        );
        render(&props, area.width, area.height);
    }
}
