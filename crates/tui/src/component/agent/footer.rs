use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use super::props::AgentViewProps;
use crate::theme::UiTheme;

/// Reserve one bottom row without making prompt layouts aware of footer content.
pub(super) fn footer_layout(area: Rect) -> [Rect; 2] {
    Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).areas(area)
}

pub struct AgentFooter;

impl AgentFooter {
    pub fn render(props: &AgentViewProps<'_>, area: Rect, buffer: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let identity = identity(props);
        let identity_width = u16::try_from(Line::from(identity.as_str()).width())
            .unwrap_or(u16::MAX)
            .min(area.width);
        let [stats, identity_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Length(identity_width)])
                .areas(area);
        Paragraph::new(stats_line(props)).render(stats, buffer);
        Paragraph::new(identity)
            .alignment(Alignment::Right)
            .style(props.styles.muted)
            .render(identity_area, buffer);
    }
}

fn identity(props: &AgentViewProps<'_>) -> String {
    let provider = match props.footer.provider {
        "" => "unknown",
        provider => provider,
    };
    format!("{provider} · {}", props.footer.model.unwrap_or("no model"))
}

fn stats_line(props: &AgentViewProps<'_>) -> Line<'static> {
    let usage = props.session.usage();
    let usage = format!(
        "↑{} ↓{} · ",
        format_tokens(usage.input),
        format_tokens(usage.output)
    );
    Line::from(vec![
        Span::styled(usage, props.styles.muted),
        context_span(props),
    ])
}

fn context_span(props: &AgentViewProps<'_>) -> Span<'static> {
    if let Some(projection) = props.footer.projection {
        return projection_span(projection, props.styles);
    }
    let (text, style) = match (props.session.latest_usage(), props.footer.context_window) {
        (Some(usage), Some(window)) if window > 0 => {
            let percent = usage.total as f64 * 100.0 / window as f64;
            (
                format!("{percent:.1}%/{}", format_tokens(window)),
                context_style(percent, props.styles),
            )
        }
        (_, Some(window)) => (format!("?/{}", format_tokens(window)), props.styles.muted),
        (_, None) => ("context ?".to_string(), props.styles.muted),
    };
    Span::styled(text, style)
}

fn projection_span(
    projection: &eggplant_agent::ContextProjection,
    styles: &UiTheme,
) -> Span<'static> {
    let measurement = projection.measurement;
    let mut text = match (
        measurement.estimated_input_tokens,
        measurement.capacity_tokens,
    ) {
        (Some(estimate), Some(capacity)) => {
            format!("~{}/{}", format_tokens(estimate), format_tokens(capacity))
        }
        (Some(estimate), None) => format!("~{} tokens", format_tokens(estimate)),
        (None, Some(capacity)) => format!("?/{}", format_tokens(capacity)),
        (None, None) => "context ?".to_string(),
    };
    if projection.omitted_turns > 0 {
        text.push_str(&format!(" · omit {}", projection.omitted_turns));
    }
    let style = match measurement.fits_capacity {
        Some(false) => styles.error,
        _ if projection.omitted_turns > 0 => styles.warning,
        _ => styles.muted,
    };
    Span::styled(text, style)
}

fn context_style(percent: f64, styles: &UiTheme) -> Style {
    if percent > 90.0 {
        styles.error
    } else if percent > 70.0 {
        styles.warning
    } else {
        styles.muted
    }
}

fn format_tokens(count: u64) -> String {
    if count < 1_000 {
        count.to_string()
    } else if count < 10_000 {
        format!("{:.1}k", count as f64 / 1_000.0)
    } else if count < 1_000_000 {
        format!("{}k", (count as f64 / 1_000.0).round() as u64)
    } else if count < 10_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else {
        format!("{}M", (count as f64 / 1_000_000.0).round() as u64)
    }
}

#[cfg(test)]
mod tests {
    use eggplant_agent::{AgentSession, TokenUsage};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        component::agent::{AgentFooterProps, AgentPromptProps, SurfaceNavigation},
        component::text_input::TextInput,
        theme::Theme,
    };

    #[test]
    fn renders_usage_context_and_provider_model_identity() {
        let mut session = AgentSession::default();
        let run = session.begin("question".to_string()).unwrap();
        session.record_usage(
            run,
            TokenUsage {
                input: 1_200,
                output: 300,
                total: 1_500,
            },
        );
        session.record_usage(
            run,
            TokenUsage {
                input: 800,
                output: 200,
                total: 1_000,
            },
        );
        let prompt = TextInput::default();
        let theme = Theme::default();
        let props = AgentViewProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: AgentFooterProps::new("openai", Some("gpt-test"), Some(10_000)),
            styles: theme.ui(),
        };
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| AgentFooter::render(&props, frame.area(), frame.buffer_mut()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("↑2.0k ↓500"));
        assert!(content.contains("10.0%/10k"));
        assert!(content.contains("openai · gpt-test"));
    }

    #[test]
    fn missing_usage_or_context_capacity_is_explicitly_unknown() {
        let session = AgentSession::default();
        let prompt = TextInput::default();
        let theme = Theme::default();
        let props = AgentViewProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: AgentFooterProps::new("fake", None, None),
            styles: theme.ui(),
        };
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| AgentFooter::render(&props, frame.area(), frame.buffer_mut()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("↑0 ↓0 · context ?"));
        assert!(content.contains("fake · no model"));
    }

    #[test]
    fn renders_projection_estimate_and_omitted_turns() {
        let session = AgentSession::default();
        let projection = eggplant_agent::ContextProjection {
            history: Vec::new(),
            omitted_turns: 2,
            measurement: eggplant_agent::ContextMeasurement {
                input_bytes: 0,
                estimated_input_tokens: Some(80),
                measured_input_tokens: None,
                capacity_tokens: Some(100),
                fits_capacity: Some(true),
            },
        };
        let prompt = TextInput::default();
        let theme = Theme::default();
        let props = AgentViewProps {
            session: &session,
            navigation: SurfaceNavigation::default(),
            prompt: AgentPromptProps::new(&prompt),
            storage_error: None,
            model_notice: None,
            footer: AgentFooterProps::new("openai", Some("gpt-test"), Some(100))
                .with_projection(Some(&projection)),
            styles: theme.ui(),
        };
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| AgentFooter::render(&props, frame.area(), frame.buffer_mut()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("~80/100"));
        assert!(content.contains("omit 2"));
    }
}
