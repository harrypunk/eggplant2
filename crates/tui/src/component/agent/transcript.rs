use eggplant_agent::{
    AgentSession, ChatMessage, MessageRole, ToolApproval, ToolEvent, ToolResultStatus,
};
use ratatui::text::{Line, Span};

use crate::theme::UiTheme;

pub(super) fn transcript_lines(
    session: &AgentSession,
    viewport_height: u16,
    styles: &UiTheme,
) -> Vec<Line<'static>> {
    let entries = session.messages();
    let start = entries
        .len()
        .saturating_sub(viewport_height.max(1) as usize);
    let mut lines = if entries.is_empty() {
        vec![Line::styled("No conversation yet", styles.muted)]
    } else {
        entries[start..]
            .iter()
            .map(|message| message_line(message, styles))
            .collect()
    };
    lines.extend(
        session
            .tool_events()
            .iter()
            .map(|event| tool_event_line(event, styles)),
    );
    if let Some(approval) = session.pending_approval() {
        lines.extend(approval_lines(approval, styles));
    }
    lines
}

fn message_line(message: &ChatMessage, styles: &UiTheme) -> Line<'static> {
    let (prefix, style) = match message.role {
        MessageRole::User => ("You: ", styles.muted),
        MessageRole::Assistant => ("Agent: ", styles.action_hint),
    };
    Line::from(vec![
        Span::styled(prefix, style),
        Span::raw(message.text.clone()),
    ])
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
