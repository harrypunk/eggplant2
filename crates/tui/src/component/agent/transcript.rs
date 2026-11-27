use eggplant_agent::{
    AgentSession, ChatMessage, MessageRole, ToolApproval, ToolEvent, ToolResultStatus,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{component::markdown::PreparedMarkdown, theme::UiTheme};

pub(super) fn render_transcript(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
    buffer: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    let mut blocks = Vec::new();
    if session.messages().is_empty() {
        blocks.push(PreparedBlock::plain(
            vec![Line::styled("No conversation yet", styles.muted)],
            styles,
        ));
    } else {
        for (index, message) in session.messages().iter().enumerate() {
            if index > 0 {
                blocks.push(PreparedBlock::spacer(styles));
            }
            blocks.push(PreparedBlock::message(
                message,
                area.width,
                RolePrefix::for_role(message.role),
                styles,
            ));
        }
    }
    let tools = session
        .tool_events()
        .iter()
        .map(|event| tool_event_line(event, styles))
        .collect::<Vec<_>>();
    if !tools.is_empty() {
        blocks.push(PreparedBlock::spacer(styles));
        blocks.push(PreparedBlock::plain(tools, styles));
    }
    if let Some(approval) = session.pending_approval() {
        blocks.push(PreparedBlock::spacer(styles));
        blocks.push(PreparedBlock::plain(
            approval_lines(approval, styles),
            styles,
        ));
    }
    render_bottom_anchored(&blocks, area, buffer);
}

pub(super) fn render_latest_assistant(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
    buffer: &mut Buffer,
) {
    if area.is_empty() {
        return;
    }
    let blocks = session
        .messages()
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| vec![PreparedBlock::message(message, area.width, None, styles)])
        .unwrap_or_default();
    render_bottom_anchored(&blocks, area, buffer);
}

#[derive(Debug)]
enum PreparedBlock {
    Plain {
        lines: Vec<Line<'static>>,
        base: Style,
    },
    Markdown {
        markdown: PreparedMarkdown,
        prefix: Option<RolePrefix>,
    },
}

#[derive(Debug, Clone)]
struct RolePrefix {
    text: &'static str,
    style: Style,
}

impl RolePrefix {
    fn for_role(role: MessageRole) -> Option<Self> {
        match role {
            MessageRole::User => None,
            MessageRole::Assistant => Some(Self {
                text: "Agent: ",
                style: Style::default(),
            }),
        }
    }

    fn width(&self) -> u16 {
        UnicodeWidthStr::width(self.text).min(usize::from(u16::MAX)) as u16
    }
}

impl PreparedBlock {
    fn message(
        message: &ChatMessage,
        width: u16,
        prefix: Option<RolePrefix>,
        styles: &UiTheme,
    ) -> Self {
        match message.role {
            MessageRole::User => Self::plain(user_lines(&message.text, styles), styles),
            MessageRole::Assistant => {
                let prefix = prefix.filter(|prefix| width > prefix.width());
                let markdown_width = prefix
                    .as_ref()
                    .map_or(width, |prefix| width.saturating_sub(prefix.width()));
                let fallback_prefix = prefix.as_ref().map_or("", |_| "Agent: ");
                match PreparedMarkdown::parse(&message.text, markdown_width, styles) {
                    Ok(markdown) if markdown.line_count() > 0 => Self::Markdown {
                        markdown,
                        prefix: prefix.map(|mut prefix| {
                            prefix.style = styles.action_hint;
                            prefix
                        }),
                    },
                    _ => Self::plain(
                        plain_message_lines(&message.text, fallback_prefix, styles.action_hint),
                        styles,
                    ),
                }
            }
        }
    }

    fn plain(lines: Vec<Line<'static>>, styles: &UiTheme) -> Self {
        Self::Plain {
            lines,
            base: styles.base,
        }
    }

    fn spacer(styles: &UiTheme) -> Self {
        Self::plain(vec![Line::default()], styles)
    }

    fn height(&self, width: u16) -> usize {
        match self {
            Self::Plain { .. } => self.paragraph().line_count(width),
            Self::Markdown { markdown, .. } => markdown.line_count(),
        }
    }

    fn render(&self, area: Rect, top_clip: usize, buffer: &mut Buffer) {
        match self {
            Self::Plain { .. } => {
                self.paragraph()
                    .scroll((top_clip.min(usize::from(u16::MAX)) as u16, 0))
                    .render(area, buffer);
            }
            Self::Markdown { markdown, prefix } => {
                let prefix_width = prefix.as_ref().map_or(0, RolePrefix::width);
                let content = Rect {
                    x: area.x.saturating_add(prefix_width),
                    width: area.width.saturating_sub(prefix_width),
                    ..area
                };
                markdown.render(content, top_clip, buffer);
                if top_clip == 0
                    && area.height > 0
                    && let Some(prefix) = prefix
                {
                    Paragraph::new(Line::from(Span::styled(prefix.text, prefix.style))).render(
                        Rect {
                            width: prefix_width,
                            height: 1,
                            ..area
                        },
                        buffer,
                    );
                }
            }
        }
    }

    fn paragraph(&self) -> Paragraph<'static> {
        let Self::Plain { lines, base } = self else {
            unreachable!("only plain blocks have paragraphs")
        };
        Paragraph::new(lines.clone())
            .wrap(Wrap { trim: false })
            .style(*base)
    }
}

fn render_bottom_anchored(blocks: &[PreparedBlock], area: Rect, buffer: &mut Buffer) {
    let heights = blocks
        .iter()
        .map(|block| block.height(area.width))
        .collect::<Vec<_>>();
    let total = heights.iter().sum::<usize>();
    let offset = total.saturating_sub(usize::from(area.height));
    let viewport_end = offset.saturating_add(usize::from(area.height));
    let mut start = 0usize;

    for (block, height) in blocks.iter().zip(heights) {
        let end = start.saturating_add(height);
        if end <= offset {
            start = end;
            continue;
        }
        if start >= viewport_end {
            break;
        }
        let top_clip = offset.saturating_sub(start);
        let visible_start = start.saturating_add(top_clip);
        let visible_end = end.min(viewport_end);
        let y = area
            .y
            .saturating_add(visible_start.saturating_sub(offset) as u16);
        let height = visible_end.saturating_sub(visible_start) as u16;
        block.render(Rect { y, height, ..area }, top_clip, buffer);
        start = end;
    }
}

fn user_lines(text: &str, styles: &UiTheme) -> Vec<Line<'static>> {
    plain_message_lines(text, "You: ", styles.muted)
}

fn plain_message_lines(text: &str, prefix: &str, prefix_style: Style) -> Vec<Line<'static>> {
    let continuation = " ".repeat(UnicodeWidthStr::width(prefix));
    text.split('\n')
        .enumerate()
        .map(|(index, line)| {
            let prefix = if index == 0 { prefix } else { &continuation };
            Line::from(vec![
                Span::styled(prefix.to_string(), prefix_style),
                Span::raw(line.to_string()),
            ])
        })
        .collect()
}

fn tool_event_line(event: &ToolEvent, styles: &UiTheme) -> Line<'static> {
    let summary = match &event.result {
        Some(result) => {
            let status = match result.status {
                ToolResultStatus::Success => "ok",
                ToolResultStatus::Error => "error",
                ToolResultStatus::Rejected => "rejected",
                ToolResultStatus::Cancelled => "cancelled",
            };
            let preview = tool_preview(&result.content);
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
}

fn tool_preview(content: &str) -> String {
    content
        .lines()
        .take(2)
        .collect::<Vec<_>>()
        .join(" · ")
        .chars()
        .take(240)
        .collect()
}

fn approval_lines(approval: &ToolApproval, styles: &UiTheme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::styled(
        format!("Approval: {}", approval.summary),
        styles.action_hint,
    )];
    lines.extend(diff_lines(&approval.diff, styles));
    lines.push(Line::styled(
        "Resolve in the Agent popup: y approve · n reject",
        styles.muted,
    ));
    lines
}

pub(super) fn diff_lines(diff: &str, styles: &UiTheme) -> Vec<Line<'static>> {
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
