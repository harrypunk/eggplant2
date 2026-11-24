use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    component::popup::Popup,
    provider_login::{
        ProviderAuthMethod, ProviderLoginOption, ProviderLoginPage, ProviderLoginState,
        ProviderSpec,
    },
    theme::UiTheme,
};

/// Pure presentation for the provider-login menu prototype.
pub struct ProviderLogin;

impl ProviderLogin {
    pub fn size(state: &ProviderLoginState) -> Size {
        let height = match state.page() {
            ProviderLoginPage::Placeholder(_) => 8,
            _ => u16::try_from(state.options().len())
                .unwrap_or(u16::MAX)
                .saturating_add(6),
        };
        Size::new(64, height)
    }

    pub fn render(state: &ProviderLoginState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let lines = match state.page() {
            ProviderLoginPage::Placeholder(provider) => placeholder_lines(provider, styles),
            _ => menu_lines(state, styles),
        };
        let content = Paragraph::new(lines).style(styles.popup_surface());
        frame.render_widget(Popup::new("Provider login", content, styles), area);
    }
}

fn menu_lines(state: &ProviderLoginState, styles: &UiTheme) -> Vec<Line<'static>> {
    let (heading, hint) = match state.page() {
        ProviderLoginPage::AuthenticationMethod => {
            ("Select authentication method:", "Enter select · Esc close")
        }
        ProviderLoginPage::Providers(method) => (method_label(method), "Enter select · Esc back"),
        ProviderLoginPage::Placeholder(_) => unreachable!("placeholders use separate content"),
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
    let heading = match provider.method() {
        ProviderAuthMethod::Account => "Account sign-in",
        ProviderAuthMethod::ApiKey => "API key sign-in",
    };
    vec![
        Line::styled(heading, styles.action_hint),
        Line::raw(""),
        Line::styled(format!("Provider: {}", provider.id()), styles.base),
        Line::styled("Sign-in is not implemented yet.", styles.muted),
        Line::raw(""),
        Line::styled("Esc back", styles.muted),
    ]
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
    use crate::theme::Theme;

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
}
