use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::{command_menu::CommandMenuState, component::popup::Popup, theme::UiTheme};

/// Contextual hints for the pending leader sequence.
///
/// This is not the future complete/searchable keymap reference. The component
/// owns presentation and intrinsic size; the page owns popup placement.
#[derive(Debug, Clone)]
pub struct KeyHints<'a> {
    title: String,
    lines: Vec<Line<'static>>,
    styles: &'a UiTheme,
}

impl<'a> KeyHints<'a> {
    pub fn new(menu: &CommandMenuState, styles: &'a UiTheme) -> Self {
        let lines = menu
            .items()
            .iter()
            .map(|item| {
                Line::from(vec![
                    Span::styled(
                        item.key.to_string(),
                        styles.popup_surface().patch(styles.key_hint),
                    ),
                    Span::raw(" "),
                    Span::raw(item.description),
                ])
            })
            .collect();
        Self {
            title: menu.context_title(),
            lines,
            styles,
        }
    }

    pub fn size(&self) -> Size {
        let content_width = self
            .lines
            .iter()
            .map(Line::width)
            .max()
            .unwrap_or(0)
            .max(Line::from(self.title.as_str()).width());
        let width = u16::try_from(content_width)
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        let height = u16::try_from(self.lines.len())
            .unwrap_or(u16::MAX)
            .saturating_add(2);
        Size { width, height }
    }
}

impl Widget for KeyHints<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(self.title, Paragraph::new(self.lines), self.styles).render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::KeyCode;

    use super::*;
    use crate::theme::Theme;

    #[test]
    fn sizes_a_vertical_list_to_its_content_and_borders() {
        let mut menu = CommandMenuState::default();
        menu.handle_key(KeyCode::Char(' '));
        let theme = Theme::default();
        let hints = KeyHints::new(&menu, theme.ui());

        assert_eq!(hints.lines.len(), 7);
        assert_eq!(hints.size().height, 9);
        assert_eq!(hints.size().width, 16);
    }

    #[test]
    fn title_and_size_follow_the_active_context_stack() {
        let mut menu = CommandMenuState::default();
        menu.handle_key(KeyCode::Char(' '));
        menu.handle_key(KeyCode::Char('b'));
        let theme = Theme::default();
        let hints = KeyHints::new(&menu, theme.ui());

        assert_eq!(hints.title, "Leader > buffer");
        assert_eq!(hints.size().width, 19);

        let mut menu = CommandMenuState::default();
        menu.handle_key(KeyCode::Char(' '));
        menu.handle_key(KeyCode::Char('q'));
        let hints = KeyHints::new(&menu, theme.ui());

        assert_eq!(hints.title, "Leader > quit");
        assert_eq!(hints.size().width, 15);
    }
}
