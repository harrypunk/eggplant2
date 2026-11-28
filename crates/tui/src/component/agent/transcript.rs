use eggplant_agent::{
    AgentSession, ChatMessage, MessagePart, MessageRole, ToolApproval, ToolEvent, ToolResultStatus,
};
use ratatui::{
    buffer::Buffer,
    layout::{Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget, Wrap},
};
use unicode_width::UnicodeWidthStr;

use super::navigation::{PageScrollState, PanelNavigation, ViewportMetrics};
use crate::{agent::AgentInputMode, component::markdown::PreparedMarkdown, theme::UiTheme};

pub(super) fn render_transcript(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
    navigation: PanelNavigation,
    mode: AgentInputMode,
    buffer: &mut Buffer,
) -> Option<Position> {
    if area.is_empty() {
        return None;
    }
    let blocks = transcript_blocks(session, area.width, styles);
    let metrics = block_metrics(&blocks, area);
    let (offset, cursor) = navigation.viewport(metrics, mode);
    render_blocks(&blocks, area, offset, cursor, styles, buffer);
    cursor.and_then(|row| {
        (row >= offset && row < offset + usize::from(area.height)).then_some(Position::new(
            area.x,
            area.y + row.saturating_sub(offset) as u16,
        ))
    })
}

fn transcript_blocks(session: &AgentSession, width: u16, styles: &UiTheme) -> Vec<PreparedBlock> {
    let messages = message_blocks(session.messages(), width, styles);
    let tools = session
        .tool_events()
        .iter()
        .map(|event| tool_event_line(event, styles))
        .collect::<Vec<_>>();
    let tools = spaced_plain_section(tools, styles);
    let approval = session
        .pending_approval()
        .and_then(|approval| spaced_plain_section(approval_lines(approval, styles), styles));

    messages
        .into_iter()
        .chain(tools.into_iter().flatten())
        .chain(approval.into_iter().flatten())
        .collect()
}

fn message_blocks(messages: &[ChatMessage], width: u16, styles: &UiTheme) -> Vec<PreparedBlock> {
    match messages {
        [] => vec![PreparedBlock::plain(
            vec![Line::styled("No conversation yet", styles.muted)],
            styles,
        )],
        messages => messages
            .iter()
            .enumerate()
            .flat_map(|(index, message)| {
                (index > 0)
                    .then(|| PreparedBlock::spacer(styles))
                    .into_iter()
                    .chain(PreparedBlock::message(
                        message,
                        width,
                        RolePrefix::for_role(message.role, styles),
                        styles,
                    ))
            })
            .collect(),
    }
}

fn spaced_plain_section(lines: Vec<Line<'static>>, styles: &UiTheme) -> Option<[PreparedBlock; 2]> {
    if lines.is_empty() {
        return None;
    }
    Some([
        PreparedBlock::spacer(styles),
        PreparedBlock::plain(lines, styles),
    ])
}

pub(super) fn render_latest_assistant(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
    scroll: PageScrollState,
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
        .map(|message| PreparedBlock::message(message, area.width, None, styles))
        .unwrap_or_default();
    let offset = scroll.offset(block_metrics(&blocks, area));
    render_blocks(&blocks, area, offset, None, styles, buffer);
}

pub(super) fn conversation_metrics(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
) -> ViewportMetrics {
    block_metrics(&transcript_blocks(session, area.width, styles), area)
}

pub(super) fn latest_assistant_metrics(
    session: &AgentSession,
    area: Rect,
    styles: &UiTheme,
) -> ViewportMetrics {
    let blocks = session
        .messages()
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::Assistant)
        .map(|message| PreparedBlock::message(message, area.width, None, styles))
        .unwrap_or_default();
    block_metrics(&blocks, area)
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
    fn for_role(role: MessageRole, styles: &UiTheme) -> Option<Self> {
        match role {
            MessageRole::User => None,
            MessageRole::Assistant => Some(Self {
                text: "Agent: ",
                style: styles.action_hint,
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
    ) -> Vec<Self> {
        match message.role {
            MessageRole::User => vec![Self::plain(user_lines(&message.text, styles), styles)],
            MessageRole::Assistant if message.parts().is_empty() => {
                vec![Self::answer(&message.text, width, prefix, styles)]
            }
            MessageRole::Assistant => {
                let mut prefix = prefix;
                message
                    .parts()
                    .iter()
                    .flat_map(|part| {
                        let part_prefix = prefix.take();
                        match part {
                            MessagePart::Thinking(text) => {
                                Self::thinking(text, width, part_prefix, styles)
                            }
                            MessagePart::Answer(text) => {
                                vec![Self::answer(text, width, part_prefix, styles)]
                            }
                        }
                    })
                    .collect()
            }
        }
    }

    fn answer(text: &str, width: u16, prefix: Option<RolePrefix>, styles: &UiTheme) -> Self {
        let prefix = prefix.filter(|prefix| width > prefix.width());
        let markdown_width = prefix
            .as_ref()
            .map_or(width, |prefix| width.saturating_sub(prefix.width()));
        let fallback_prefix = prefix.as_ref().map_or("", |_| "Agent: ");
        match PreparedMarkdown::parse(text, markdown_width, styles) {
            Ok(markdown) if markdown.line_count() > 0 => Self::Markdown { markdown, prefix },
            _ => Self::plain(
                plain_message_lines(text, fallback_prefix, styles.action_hint),
                styles,
            ),
        }
    }

    fn thinking(text: &str, width: u16, prefix: Option<RolePrefix>, styles: &UiTheme) -> Vec<Self> {
        let thinking_style = styles.muted.add_modifier(Modifier::ITALIC);
        let mut header_spans = Vec::new();
        if let Some(prefix) = prefix {
            header_spans.push(Span::styled(prefix.text, prefix.style));
        }
        header_spans.push(Span::styled("▾ Thinking", thinking_style));
        let header = Self::plain(vec![Line::from(header_spans)], styles);

        let prefix = (width > 2).then_some(RolePrefix {
            text: "  ",
            style: styles.muted,
        });
        let body = match PreparedMarkdown::parse_thinking(text, width.saturating_sub(2), styles) {
            Ok(markdown) if markdown.line_count() > 0 => Self::Markdown { markdown, prefix },
            _ => Self::plain(
                text.split('\n')
                    .map(|line| Line::styled(format!("  {line}"), thinking_style))
                    .collect(),
                styles,
            ),
        };
        vec![header, body]
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

fn block_metrics(blocks: &[PreparedBlock], area: Rect) -> ViewportMetrics {
    ViewportMetrics {
        content_rows: blocks.iter().map(|block| block.height(area.width)).sum(),
        viewport_rows: usize::from(area.height),
    }
}

fn render_blocks(
    blocks: &[PreparedBlock],
    area: Rect,
    offset: usize,
    cursor: Option<usize>,
    styles: &UiTheme,
    buffer: &mut Buffer,
) {
    let viewport_end = offset.saturating_add(usize::from(area.height));
    let mut start = 0usize;

    for block in blocks {
        let end = start.saturating_add(block.height(area.width));
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

    if let Some(row) = cursor.filter(|row| *row >= offset && *row < viewport_end) {
        buffer.set_style(
            Rect {
                y: area.y + row.saturating_sub(offset) as u16,
                height: 1,
                ..area
            },
            styles.cursor_line,
        );
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
