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
        text_layout::TextFlow,
    },
    provider_login::{
        OpenAiField, ProviderAuthMethod, ProviderLoginOption, ProviderLoginPage,
        ProviderLoginState, ProviderSpec,
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
            ProviderLoginPage::OpenAiInput(_) => openai_height(state, 62),
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
        if state.is_saving() {
            return None;
        }
        match state.page() {
            ProviderLoginPage::ApiKeyInput(_) => {
                let areas = api_key_areas(content_area(area));
                state
                    .api_key_input()
                    .view(styles)
                    .with_style(&API_KEY_MASK_STYLE)
                    .cursor_position(areas.input)
            }
            ProviderLoginPage::OpenAiInput(_) => {
                let areas = openai_areas(state, content_area(area));
                let field = state.openai_focused_field();
                let input = state.openai_input(field);
                let area = match field {
                    OpenAiField::Endpoint => areas.endpoint,
                    OpenAiField::ApiKey => areas.api_key,
                };
                let view = input.view_with_flow(styles, TextFlow::Wrapped);
                match field {
                    OpenAiField::Endpoint => view.cursor_position(area),
                    OpenAiField::ApiKey => {
                        view.with_style(&API_KEY_MASK_STYLE).cursor_position(area)
                    }
                }
            }
            _ => None,
        }
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
            ProviderLoginPage::OpenAiInput(provider) => {
                render_openai(self.state, provider, self.styles, area, buffer)
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
        ProviderLoginPage::Placeholder(_)
        | ProviderLoginPage::ApiKeyInput(_)
        | ProviderLoginPage::OpenAiInput(_) => {
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

struct OpenAiAreas {
    heading: Rect,
    provider: Rect,
    endpoint_label: Rect,
    endpoint: Rect,
    api_key_label: Rect,
    api_key: Rect,
    status: Rect,
    hints: Rect,
}

fn openai_input_heights(state: &ProviderLoginState, width: u16) -> (u16, u16) {
    let endpoint = state
        .openai_input(OpenAiField::Endpoint)
        .measure(width, TextFlow::Wrapped)
        .height;
    let api_key = state
        .openai_input(OpenAiField::ApiKey)
        .measure(width, TextFlow::Wrapped)
        .height;
    (endpoint, api_key)
}

fn openai_height(state: &ProviderLoginState, width: u16) -> u16 {
    let (endpoint, api_key) = openai_input_heights(state, width);
    endpoint.saturating_add(api_key).saturating_add(10)
}

fn openai_areas(state: &ProviderLoginState, area: Rect) -> OpenAiAreas {
    let (endpoint_height, api_key_height) = openai_input_heights(state, area.width);
    let [
        heading,
        _,
        provider,
        endpoint_label,
        endpoint,
        api_key_label,
        api_key,
        status,
        _,
        hints,
    ] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(endpoint_height),
        Constraint::Length(1),
        Constraint::Length(api_key_height),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);
    OpenAiAreas {
        heading,
        provider,
        endpoint_label,
        endpoint,
        api_key_label,
        api_key,
        status,
        hints,
    }
}

fn render_openai(
    state: &ProviderLoginState,
    provider: ProviderSpec,
    styles: &UiTheme,
    area: Rect,
    buffer: &mut Buffer,
) {
    let areas = openai_areas(state, area);
    let focused = state.openai_focused_field();
    Paragraph::new("OpenAI-compatible sign-in")
        .style(styles.action_hint)
        .render(areas.heading, buffer);
    Paragraph::new(format!("Provider: {}", provider.id()))
        .style(styles.base)
        .render(areas.provider, buffer);
    Paragraph::new("Endpoint")
        .style(if focused == OpenAiField::Endpoint {
            styles.action_hint
        } else {
            styles.muted
        })
        .render(areas.endpoint_label, buffer);
    state
        .openai_input(OpenAiField::Endpoint)
        .view_with_flow(styles, TextFlow::Wrapped)
        .render(areas.endpoint, buffer);
    Paragraph::new("API key")
        .style(if focused == OpenAiField::ApiKey {
            styles.action_hint
        } else {
            styles.muted
        })
        .render(areas.api_key_label, buffer);
    state
        .openai_input(OpenAiField::ApiKey)
        .view_with_flow(styles, TextFlow::Wrapped)
        .with_style(&API_KEY_MASK_STYLE)
        .render(areas.api_key, buffer);
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
        "Tab focus · Ctrl-V paste · Enter save · Esc back"
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
    use crate::{
        component::text_input::TextInputEvent, provider_login::DEFAULT_OPENAI_ENDPOINT,
        theme::Theme,
    };

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
        for _ in 0..3 {
            state.select_next();
        }
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

    #[test]
    fn openai_form_wraps_both_fields_and_tracks_focus() {
        let mut state = ProviderLoginState::default();
        state.open(None);
        state.select_next();
        state.activate();
        state.activate();
        assert!(render(&state).contains(DEFAULT_OPENAI_ENDPOINT));

        for character in "segment/".repeat(12).chars() {
            state.handle_input_event(TextInputEvent::Insert(character));
        }
        state.focus_next();
        for character in "key-secret".chars() {
            state.handle_input_event(TextInputEvent::Insert(character));
        }
        assert_eq!(state.openai_focused_field(), OpenAiField::ApiKey);
        let content = render(&state);
        assert!(content.contains("Endpoint"));
        assert!(content.contains("API key"));
        assert!(content.contains("segment/"));
        assert!(content.contains("••••••••••"));
        assert!(!content.contains("key-secret"));
        assert!(ProviderLogin::size(&state).height > 16);

        let theme = Theme::default();
        let size = ProviderLogin::size(&state);
        let area = Rect::new(0, 0, size.width, size.height);
        assert!(ProviderLogin::cursor(&state, theme.ui(), area).is_some());
        state.focus_next();
        assert_eq!(state.openai_focused_field(), OpenAiField::Endpoint);
        assert!(ProviderLogin::cursor(&state, theme.ui(), area).is_some());
    }
}
