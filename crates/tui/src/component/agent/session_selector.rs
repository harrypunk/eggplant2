use eggplant_agent_storage::SessionRecord;
use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    agent::{SessionSelectorNotice, SessionSelectorPage, SessionSelectorState},
    component::{picker, popup::Popup},
    theme::UiTheme,
};

const MAX_SESSION_ROWS: u16 = 10;

pub struct AgentSessionSelector;

impl AgentSessionSelector {
    pub fn size(state: &SessionSelectorState) -> Size {
        let height = match state.page() {
            SessionSelectorPage::Closed => 0,
            SessionSelectorPage::Loading => 8,
            SessionSelectorPage::Ready | SessionSelectorPage::Restoring => {
                u16::try_from(state.sessions().len())
                    .unwrap_or(MAX_SESSION_ROWS)
                    .min(MAX_SESSION_ROWS)
                    .saturating_add(7)
                    .saturating_add(u16::from(state.notice().is_some()))
            }
        };
        Size::new(72, height)
    }

    pub fn render(state: &SessionSelectorState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let content = match state.page() {
            SessionSelectorPage::Closed => Paragraph::new(""),
            SessionSelectorPage::Loading => {
                Paragraph::new(operation_lines("Loading recent sessions", styles))
            }
            SessionSelectorPage::Ready | SessionSelectorPage::Restoring => {
                Paragraph::new(session_lines(state, styles, area.height))
            }
        };
        frame.render_widget(
            Popup::new("Sessions", content.style(styles.popup_surface()), styles),
            area,
        );
    }
}

fn session_lines(
    state: &SessionSelectorState,
    styles: &UiTheme,
    height: u16,
) -> Vec<Line<'static>> {
    let notice_height = u16::from(state.notice().is_some());
    let viewport = height.saturating_sub(5 + notice_height);
    let options = state
        .sessions()
        .iter()
        .enumerate()
        .map(|(index, session)| session_line(state, session, index, styles))
        .collect();
    let options = picker::visible_lines(options, state.selected(), viewport);
    let status = match state.page() {
        SessionSelectorPage::Restoring => {
            Line::styled("Restoring selected session...", styles.muted)
        }
        SessionSelectorPage::Closed | SessionSelectorPage::Loading | SessionSelectorPage::Ready => {
            Line::styled(
                format!(
                    "Current: {}",
                    state
                        .active_session()
                        .map(|id| id.as_str())
                        .unwrap_or("none")
                ),
                styles.muted,
            )
        }
    };
    let notice = state.notice().map(|notice| match notice {
        SessionSelectorNotice::Info(message) => Line::styled(message.clone(), styles.muted),
        SessionSelectorNotice::Error(message) => Line::styled(message.clone(), styles.error),
    });
    let hints = match state.page() {
        SessionSelectorPage::Restoring => "Esc close",
        SessionSelectorPage::Closed | SessionSelectorPage::Loading | SessionSelectorPage::Ready => {
            "Enter restore · Esc close"
        }
    };
    [
        Line::styled("Recent sessions in this workspace", styles.action_hint),
        Line::raw(""),
    ]
    .into_iter()
    .chain(options)
    .chain([status])
    .chain(notice)
    .chain([Line::raw(""), Line::styled(hints, styles.muted)])
    .collect()
}

fn session_line(
    state: &SessionSelectorState,
    session: &SessionRecord,
    index: usize,
    styles: &UiTheme,
) -> Line<'static> {
    let title = session
        .title
        .as_deref()
        .unwrap_or("Untitled session")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let title = truncate(&title, 42);
    let id = session.id.as_str().chars().take(8).collect::<String>();
    let marker = match state.active_session() == Some(&session.id) {
        true => "●",
        false => " ",
    };
    picker::selectable_line(
        format!("{marker} {title:<42} {id}"),
        state.selected() == Some(index),
        styles,
    )
}

fn truncate(text: &str, limit: usize) -> String {
    let preview = text.chars().take(limit).collect::<String>();
    match text.chars().count() > limit {
        true => format!("{preview}…"),
        false => preview,
    }
}

fn operation_lines(label: &'static str, styles: &UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::styled(label, styles.action_hint),
        Line::raw(""),
        Line::styled("Reading the local session store", styles.base),
        Line::styled("Please wait", styles.muted),
        Line::raw(""),
        Line::styled("Esc close", styles.muted),
    ]
}

#[cfg(test)]
mod tests {
    use eggplant_agent::SessionId;
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    fn record(index: usize) -> SessionRecord {
        SessionRecord {
            id: SessionId::from(format!("session-{index:02}")),
            created_at: 1,
            updated_at: 2,
            workspace: Some("/workspace".to_string()),
            title: Some(format!("session-{index:02}")),
        }
    }

    #[test]
    fn bounded_rows_follow_the_active_session() {
        let mut state = SessionSelectorState::default();
        let generation = state.open(None, SessionId::from("session-19"));
        state.complete_recent(generation, Ok((0..20).map(record).collect::<Vec<_>>()));
        let theme = Theme::default();
        let size = AgentSessionSelector::size(&state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| AgentSessionSelector::render(&state, theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("session-19"));
        assert!(!content.contains("session-00"));
        assert!(content.contains("●"));
        assert!(content.contains("Current: session-19"));
    }
}
