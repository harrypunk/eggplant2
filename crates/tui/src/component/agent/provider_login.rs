use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::{component::popup::Popup, theme::UiTheme};

/// Placeholder presentation for the future provider-login form.
pub struct ProviderLogin;

impl ProviderLogin {
    pub fn render(styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let content = Paragraph::new(vec![
            Line::styled("Provider login is not implemented yet.", styles.base),
            Line::raw(""),
            Line::styled("Esc close", styles.muted),
        ])
        .style(styles.popup_surface());
        frame.render_widget(Popup::new("Provider login", content, styles), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_placeholder_without_a_model() {
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(58, 8)).unwrap();
        terminal
            .draw(|frame| ProviderLogin::render(theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Provider login"));
        assert!(content.contains("not implemented yet"));
        assert!(content.contains("Esc close"));
    }
}
