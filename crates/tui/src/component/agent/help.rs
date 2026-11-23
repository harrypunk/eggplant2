use ratatui::{Frame, layout::Rect, text::Line, widgets::Paragraph};

use crate::{component::popup::Popup, theme::UiTheme};

const HELP_TEXT: [&str; 10] = [
    "Normal",
    "  i              insert text",
    "  ? / Esc        close help",
    "  Ctrl-H / Ctrl-L move focus",
    "",
    "Insert",
    "  Enter          submit prompt",
    "  Esc            return to Normal",
    "  Backspace      delete previous character",
    "  Ctrl-F7        cancel active run",
];

pub struct AgentHelp;

impl AgentHelp {
    pub fn render(styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let lines = HELP_TEXT.into_iter().map(Line::raw).collect::<Vec<_>>();
        let content = Paragraph::new(lines).style(styles.popup_surface());
        frame.render_widget(Popup::new("Agent Help", content, styles), area);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn lists_both_input_modes() {
        let theme = Theme::default();
        let mut terminal = Terminal::new(TestBackend::new(54, 12)).unwrap();
        terminal
            .draw(|frame| AgentHelp::render(theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for expected in [
            "Agent Help",
            "Normal",
            "insert text",
            "close help",
            "Insert",
            "submit prompt",
            "return to Normal",
        ] {
            assert!(content.contains(expected), "missing help text: {expected}");
        }
    }
}
