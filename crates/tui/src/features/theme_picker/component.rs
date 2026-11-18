use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Rect, Size},
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::{
    component::{
        picker,
        popup::{self, Popup},
    },
    data_object::Model,
    theme::Theme,
    theme_picker::{ThemePickerEntry, ThemePickerState},
    ui::{OverlayContribution, OverlayKind, OverlayPlacement},
};

const MAX_WIDTH: u16 = 64;
const MAX_HEIGHT: u16 = 20;

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
        let content_width = self
            .state
            .entries()
            .iter()
            .map(|entry| entry.name().len() + 2)
            .max()
            .unwrap_or(0);
        let section_rows = usize::from(
            self.state.is_discovering()
                || self
                    .state
                    .entries()
                    .iter()
                    .any(|entry| matches!(entry, ThemePickerEntry::Ghostty { .. })),
        );
        Size {
            width: u16::try_from(content_width + 2)
                .unwrap_or(u16::MAX)
                .min(MAX_WIDTH),
            height: u16::try_from(self.state.entries().len() + section_rows + 2)
                .unwrap_or(u16::MAX)
                .min(MAX_HEIGHT),
        }
    }
}

pub fn overlay(model: &Model) -> Option<OverlayContribution> {
    if !model.theme_picker.is_open() {
        return None;
    }
    let picker = ThemePicker::new(&model.theme_picker, &model.theme);
    Some(OverlayContribution::new(
        "theme_picker",
        OverlayKind::Modal,
        OverlayPlacement::Centered(picker.size()),
        render_overlay,
    ))
}

fn render_overlay(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(ThemePicker::new(&model.theme_picker, &model.theme), area);
}

impl Widget for ThemePicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let surface = self.theme.ui().popup_surface();
        let mut lines = Vec::new();
        let mut selected_row = None;
        let mut has_ghostty_section = false;
        for (index, entry) in self.state.entries().iter().enumerate() {
            if matches!(entry, ThemePickerEntry::Ghostty { .. }) && !has_ghostty_section {
                has_ghostty_section = true;
                lines.push(Line::styled(
                    "ghostty",
                    surface.patch(self.theme.ui().action_hint),
                ));
            }
            if self.state.selected_index() == index {
                selected_row = Some(lines.len());
            }
            let selected = self.state.selected_index() == index;
            let marker = if selected { ">" } else { " " };
            let style = match selected {
                true => surface.patch(self.theme.ui().selection),
                false => surface,
            };
            lines.push(Line::styled(format!("{marker} {}", entry.name()), style));
        }
        if self.state.is_discovering() {
            if !has_ghostty_section {
                lines.push(Line::styled(
                    "ghostty",
                    surface.patch(self.theme.ui().action_hint),
                ));
            }
            lines.push(Line::styled(
                "Loading…",
                surface.patch(self.theme.ui().muted),
            ));
        }
        let lines = picker::visible_lines(lines, selected_row, popup::content_area(area).height);
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
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::theme::ThemeName;

    #[test]
    fn renders_builtin_names_and_the_selected_row() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::TokyoNight), "tokyonight");
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

    #[test]
    fn renders_a_ghostty_section_with_local_themes() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::Default), "default");
        let revision = state.begin_discovery().unwrap();
        assert!(state.complete_discovery(
            revision,
            vec![ThemePickerEntry::Ghostty {
                name: "local-theme".to_string(),
                path: PathBuf::from("/themes/local-theme"),
            }],
        ));
        let theme = Theme::default();
        let backend = TestBackend::new(32, 7);
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
        assert!(content.contains("ghostty"));
        assert!(content.contains("  local-theme"));
    }

    #[test]
    fn keeps_the_selected_theme_visible_in_a_small_terminal() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::CatppuccinMocha), "catppuccin_mocha");
        let theme = Theme::default();
        let backend = TestBackend::new(28, 3);
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
        assert!(content.contains("> catppuccin_mocha"));
        assert!(!content.contains("  default"));
    }
}
