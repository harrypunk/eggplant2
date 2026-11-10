use ratatui::{
    buffer::Buffer,
    layout::{Rect, Size},
    text::Line,
    widgets::{Paragraph, Widget},
};

use super::popup::Popup;
use crate::{
    theme::{Theme, ThemeName},
    theme_picker::ThemePickerState,
};

#[derive(Debug, Clone, Copy)]
pub struct ThemePicker<'a> {
    state: &'a ThemePickerState,
    theme: &'a Theme,
}

impl<'a> ThemePicker<'a> {
    pub fn new(state: &'a ThemePickerState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    pub fn size(&self) -> Size {
        let content_width = ThemeName::ALL
            .map(|name| name.as_str().len() + 2)
            .into_iter()
            .max()
            .unwrap_or(0);
        Size {
            width: u16::try_from(content_width + 2).unwrap_or(u16::MAX),
            height: u16::try_from(ThemeName::ALL.len() + 2).unwrap_or(u16::MAX),
        }
    }
}

impl Widget for ThemePicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let surface = self.theme.ui().popup_surface();
        let lines = ThemeName::ALL
            .into_iter()
            .map(|name| {
                let selected = name == self.state.selected();
                let marker = if selected { ">" } else { " " };
                let style = match selected {
                    true => surface.patch(self.theme.ui().selection),
                    false => surface,
                };
                Line::styled(format!("{marker} {}", name.as_str()), style)
            })
            .collect::<Vec<_>>();
        Popup::new(
            "Theme",
            Paragraph::new(lines).style(surface),
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use ratatui::{Terminal, backend::TestBackend};

    use super::*;

    #[test]
    fn renders_builtin_names_and_the_selected_row() {
        let mut state = ThemePickerState::default();
        state.open(ThemeName::TokyoNight);
        let theme = Theme::default();
        let backend = TestBackend::new(28, 5);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal
            .draw(|frame| frame.render_widget(ThemePicker::new(&state, &theme), frame.area()))
            .unwrap();

        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(content.contains("Theme"));
        assert!(content.contains("  default"));
        assert!(content.contains("> tokyonight"));
        assert!(content.contains("  catppuccin_mocha"));
    }
}
