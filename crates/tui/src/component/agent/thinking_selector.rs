use ratatui::{
    Frame,
    layout::{Rect, Size},
    text::Line,
    widgets::Paragraph,
};

use crate::{
    agent::ThinkingSelectorState,
    component::{picker, popup::Popup},
    theme::UiTheme,
};

pub struct AgentThinkingSelector;

impl AgentThinkingSelector {
    pub fn size(state: &ThinkingSelectorState) -> Size {
        Size::new(58, state.options().len() as u16 + 7)
    }

    pub fn render(state: &ThinkingSelectorState, styles: &UiTheme, frame: &mut Frame, area: Rect) {
        let mut lines = vec![
            Line::styled("Select thinking mode", styles.action_hint),
            Line::raw(""),
        ];
        lines.extend(state.options().iter().enumerate().map(|(index, option)| {
            picker::selectable_line(
                format!("{:<12} {}", option.label, option.description),
                state.selected() == Some(index),
                styles,
            )
        }));
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            format!(
                "Active: {}",
                state.active_label().unwrap_or("Provider default")
            ),
            styles.muted,
        ));
        lines.push(Line::raw(""));
        lines.push(Line::styled("Enter select · Esc close", styles.muted));
        frame.render_widget(
            Popup::new(
                "Thinking",
                Paragraph::new(lines).style(styles.popup_surface()),
                styles,
            ),
            area,
        );
    }
}

#[cfg(test)]
mod tests {
    use eggplant_agent::{ThinkingId, ThinkingOption};
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn renders_dynamic_options_and_active_policy() {
        let mut state = ThinkingSelectorState::default();
        let options = [
            ThinkingOption::new("off", "Off", "No reasoning"),
            ThinkingOption::new("medium2026", "Medium 2026", "Current reasoning"),
            ThinkingOption::new("reason-v1", "Reason V1", "Extended reasoning"),
        ];
        state.open(None, options.to_vec(), Some(ThinkingId::new("medium2026")));
        let theme = Theme::default();
        let size = AgentThinkingSelector::size(&state);
        let mut terminal = Terminal::new(TestBackend::new(size.width, size.height)).unwrap();
        terminal
            .draw(|frame| AgentThinkingSelector::render(&state, theme.ui(), frame, frame.area()))
            .unwrap();
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Select thinking mode"));
        assert!(content.contains("Reason V1"));
        assert!(content.contains("Active: Medium 2026"));
    }
}
