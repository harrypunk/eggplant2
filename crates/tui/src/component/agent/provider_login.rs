use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::{
    component::{
        popup::{Popup, content_area},
        text_input::{TextInputStyleInput, TextInputStyleOutput, TextInputViewStyle},
    },
    provider_login::{
        ProviderAuthMethod, ProviderLoginOption, ProviderLoginPage, ProviderLoginState,
        ProviderSpec,
    },
    theme::UiTheme,
};

/// Pure presentation for provider-login menus and API-key drafts.
pub struct ProviderLogin;

impl ProviderLogin {
    pub fn size(state: &ProviderLoginState) -> Size {
        let height = match state.page() {
            ProviderLoginPage::Placeholder(_) => 8,
            ProviderLoginPage::ApiKeyInput(_) => 11,
            _ => u16::try_from(state.options().len())
                .unwrap_or(u16::MAX)
                .saturating_add(6),
        };
        Size::new(64, height)
    }

    pub fn render(state: &ProviderLoginState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Popup::new(
                "Provider login",
                ProviderLoginContent { state, styles },
                styles,
            ),
            area,
        );
    }

    pub fn cursor(state: &ProviderLoginState, styles: &UiTheme, area: Rect) -> Option<Position> {
        if !matches!(state.page(), ProviderLoginPage::ApiKeyInput(_)) || state.is_saving() {
            return None;
        }
        let areas = api_key_areas(content_area(area));
        state
            .api_key_input()
            .view(styles)
            .with_style(&API_KEY_MASK_STYLE)
            .cursor_position(areas.input)
    }
}

struct ProviderLoginContent<'a> {
    state: &'a ProviderLoginState,
    styles: &'a UiTheme,
}

impl Widget for ProviderLoginContent<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        match self.state.page() {
            ProviderLoginPage::ApiKeyInput(provider) => {
                render_api_key(self.state, provider, self.styles, area, buffer)
            }
            ProviderLoginPage::Placeholder(provider) => {
                Paragraph::new(placeholder_lines(provider, self.styles))
                    .style(self.styles.popup_surface())
                    .render(area, buffer)
            }
            _ => Paragraph::new(menu_lines(self.state, self.styles))
                .style(self.styles.popup_surface())
                .render(area, buffer),
        }
    }
}

fn menu_lines(state: &ProviderLoginState, styles: &UiTheme) -> Vec<Line<'static>> {
    let (heading, hint) = match state.page() {
        ProviderLoginPage::AuthenticationMethod => {
            ("Select authentication method:", "Enter select · Esc close")
        }
        ProviderLoginPage::Providers(method) => (method_label(method), "Enter select · Esc back"),
        ProviderLoginPage::Placeholder(_) | ProviderLoginPage::ApiKeyInput(_) => {
            unreachable!("provider pages use separate content")
        }
    };
    let mut lines = vec![Line::styled(heading, styles.action_hint), Line::raw("")];
    lines.extend(state.options().iter().enumerate().map(|(index, option)| {
        let selected = state.selected() == Some(index);
        let label = match option {
            ProviderLoginOption::Method(method) => method_label(*method),
            ProviderLoginOption::Provider(provider) => provider.id(),
        };
        let prefix = if selected { "> " } else { "  " };
        Line::styled(
            format!("{prefix}{label}"),
            if selected {
                styles.selection
            } else {
                styles.base
            },
        )
    }));
    lines.push(Line::raw(""));
    lines.push(Line::styled(hint, styles.muted));
    lines
}

fn placeholder_lines(provider: ProviderSpec, styles: &UiTheme) -> Vec<Line<'static>> {
    vec![
        Line::styled("Account sign-in", styles.action_hint),
        Line::raw(""),
        Line::styled(format!("Provider: {}", provider.id()), styles.base),
        Line::styled("Sign-in is not implemented yet.", styles.muted),
        Line::raw(""),
        Line::styled("Esc back", styles.muted),
    ]
}

struct ApiKeyAreas {
    heading: Rect,
    provider: Rect,
    input: Rect,
    status: Rect,
    hints: Rect,
}

fn api_key_areas(area: Rect) -> ApiKeyAreas {
    let [heading, _, provider, input, status, _, hints] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    ApiKeyAreas {
        heading,
        provider,
        input,
        status,
        hints,
    }
}

fn render_api_key(
    state: &ProviderLoginState,
    provider: ProviderSpec,
    styles: &UiTheme,
    area: Rect,
    buffer: &mut Buffer,
) {
    let areas = api_key_areas(area);
    Paragraph::new("API key sign-in")
        .style(styles.action_hint)
        .render(areas.heading, buffer);
    Paragraph::new(format!("Provider: {}", provider.id()))
        .style(styles.base)
        .render(areas.provider, buffer);
    state
        .api_key_input()
        .view(styles)
        .with_style(&API_KEY_MASK_STYLE)
        .render(areas.input, buffer);
    if let Some(error) = state.error() {
        Paragraph::new(error)
            .style(styles.error)
            .render(areas.status, buffer);
    } else if state.is_saving() {
        Paragraph::new("Saving...")
            .style(styles.muted)
            .render(areas.status, buffer);
    }
    Paragraph::new(if state.is_saving() {
        "Saving..."
    } else {
        "Enter save · Esc back"
    })
    .style(styles.muted)
    .render(areas.hints, buffer);
}

#[derive(Debug)]
struct ApiKeyMaskStyle;

static API_KEY_MASK_STYLE: ApiKeyMaskStyle = ApiKeyMaskStyle;

impl TextInputViewStyle for ApiKeyMaskStyle {
    fn style(&self, input: TextInputStyleInput<'_>) -> TextInputStyleOutput {
        let text = "•".repeat(input.value.chars().count());
        let cursor_byte = text
            .char_indices()
            .nth(input.cursor_offset)
            .map(|(index, _)| index)
            .unwrap_or(text.len());
        TextInputStyleOutput { text, cursor_byte }
    }
}

fn method_label(method: ProviderAuthMethod) -> &'static str {
    match method {
        ProviderAuthMethod::Account => "Sign in with an account",
        ProviderAuthMethod::ApiKey => "Sign in with an API key",
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{component::text_input::TextInputEvent, theme::Theme};

    fn render(state: &ProviderLoginState) -> String {
        let theme = Theme::default();
        let size = ProviderLogin::size(state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| ProviderLogin::render(state, theme.ui(), frame, frame.area()))
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
    fn renders_authentication_methods() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        let content = render(&state);
        assert!(content.contains("Select authentication method"));
        assert!(content.contains("Sign in with an account"));
        assert!(content.contains("Sign in with an API key"));
        assert!(content.contains("Esc close"));
    }

    #[test]
    fn renders_account_providers_and_their_placeholder() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        state.activate();
        let content = render(&state);
        for provider in ["openai", "qwen.com", "kimi.com"] {
            assert!(content.contains(provider));
        }
        assert!(!content.contains("deepseek"));

        state.activate();
        let content = render(&state);
        assert!(content.contains("Account sign-in"));
        assert!(content.contains("Provider: openai"));
        assert!(content.contains("not implemented yet"));
        assert!(content.contains("Esc back"));
    }

    #[test]
    fn renders_every_api_key_provider_without_clipping() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        state.select_next();
        state.activate();
        let content = render(&state);
        for provider in [
            "openai",
            "kimi-for-coding",
            "qwen-tokenplan-individual-cn",
            "deepseek",
        ] {
            assert!(content.contains(provider), "missing {provider}");
        }
        assert!(!content.contains("qwen.com"));
    }

    #[test]
    fn api_key_input_uses_the_injected_mask_and_preserves_the_cursor() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        state.select_next();
        state.activate();
        state.activate();
        for character in "key-secret".chars() {
            state.handle_input_event(TextInputEvent::Insert(character));
        }
        let content = render(&state);
        assert!(content.contains("••••••••••"), "{content:?}");
        assert!(!content.contains("key-secret"));
        assert!(content.contains("Enter save"));

        let theme = Theme::default();
        let area = Rect::new(0, 0, 64, 11);
        assert!(ProviderLogin::cursor(&state, theme.ui(), area).is_some());
        let ProviderLoginPage::ApiKeyInput(provider) = state.page() else {
            unreachable!();
        };
        assert!(state.begin_save(provider));
        assert!(ProviderLogin::cursor(&state, theme.ui(), area).is_none());
        assert!(render(&state).contains("Saving..."));
    }
}
