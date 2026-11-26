use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    component::{picker, popup::Popup},
    provider_logout::{ProviderLogoutPage, ProviderLogoutState},
    theme::UiTheme,
};

const MAX_PROVIDER_ROWS: u16 = 8;

pub struct ProviderLogout;

impl ProviderLogout {
    pub fn size(state: &ProviderLogoutState) -> Size {
        let height = match state.page() {
            ProviderLogoutPage::Closed => 0,
            ProviderLogoutPage::Loading | ProviderLogoutPage::Deleting => 8,
            ProviderLogoutPage::Ready => u16::try_from(state.providers().len())
                .unwrap_or(MAX_PROVIDER_ROWS)
                .clamp(1, MAX_PROVIDER_ROWS)
                .saturating_add(6)
                .saturating_add(u16::from(state.error().is_some())),
        };
        Size::new(56, height)
    }

    pub fn render(state: &ProviderLogoutState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let content = match state.page() {
            ProviderLogoutPage::Loading => {
                Paragraph::new(operation_lines("Loading logged-in providers", styles))
            }
            ProviderLogoutPage::Deleting => {
                Paragraph::new(operation_lines("Removing stored credential", styles))
            }
            ProviderLogoutPage::Closed => Paragraph::new(""),
            ProviderLogoutPage::Ready => Paragraph::new(provider_lines(state, styles, area.height)),
        };
        frame.render_widget(
            Popup::new(
                "Provider logout",
                content.style(styles.popup_surface()),
                styles,
            ),
            area,
        );
    }
}

fn provider_lines(
    state: &ProviderLogoutState,
    styles: &UiTheme,
    height: u16,
) -> Vec<Line<'static>> {
    let error_height = u16::from(state.error().is_some());
    let viewport = height.saturating_sub(4 + error_height);
    let options = if state.providers().is_empty() {
        vec![Line::styled("No stored credentials", styles.muted)]
    } else {
        state
            .providers()
            .iter()
            .enumerate()
            .map(|(index, provider)| {
                picker::selectable_line(
                    provider.provider_id.clone(),
                    state.selected() == Some(index),
                    styles,
                )
            })
            .collect()
    };
    let options = picker::visible_lines(options, state.selected(), viewport);
    let mut lines = vec![
        Line::styled("Select provider to log out:", styles.action_hint),
        Line::raw(""),
    ];
    lines.extend(options);
    if let Some(error) = state.error() {
        lines.push(Line::styled(error.to_string(), styles.error));
    }
    lines.push(Line::raw(""));
    lines.push(Line::styled("Enter log out · Esc close", styles.muted));
    lines
}

fn operation_lines(label: &'static str, styles: &UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::styled(label, styles.action_hint),
        Line::raw(""),
        Line::styled("Please wait", styles.muted),
        Line::raw(""),
        Line::styled("Esc close", styles.muted),
    ]
}

#[cfg(test)]
mod tests {
    use eggplant_agent_storage::{ProviderAuthType, ProviderCredentialInfo};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_a_simple_provider_list() {
        let theme = Theme::default();
        let mut state = ProviderLogoutState::default();
        state.open(None);
        let generation = state.generation();
        state.complete_load(
            generation,
            Ok(["openai", "qwen-tokenplan-individual-cn"]
                .into_iter()
                .map(|provider_id| ProviderCredentialInfo {
                    provider_id: provider_id.to_string(),
                    auth_type: ProviderAuthType::ApiKey,
                })
                .collect()),
        );
        let size = ProviderLogout::size(&state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| ProviderLogout::render(&state, theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(content.contains("Select provider to log out"));
        assert!(content.contains("> openai"));
        assert!(content.contains("qwen-tokenplan-individual-cn"));
    }
}
