use ratatui::{
    Frame,
    buffer::Buffer,
    layout::{Constraint, Layout, Position, Rect, Size},
    text::Line,
    widgets::{Paragraph, Widget},
};

use crate::{
    component::{
        picker,
        popup::{self, Popup},
        query_input::QueryInput,
    },
    data_object::Model,
    theme::{Theme, UiTheme},
    theme_picker::ThemePickerState,
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
        let sections = self.state.visible_sections();
        let content_width = sections
            .iter()
            .flat_map(|section| &section.entries)
            .map(|(_, entry)| entry.name().len() + 2)
            .max()
            .unwrap_or(0);
        let content_rows = sections
            .iter()
            .map(|section| section.entries.len())
            .sum::<usize>()
            + sections.len()
            + usize::from(self.state.is_discovering())
            + usize::from(sections.is_empty());
        Size {
            width: u16::try_from(content_width + 2)
                .unwrap_or(u16::MAX)
                .min(MAX_WIDTH),
            height: u16::try_from(content_rows + 5)
                .unwrap_or(u16::MAX)
                .min(MAX_HEIGHT),
        }
    }

    pub fn cursor_position(self, area: Rect) -> Option<Position> {
        let (input, _) = content_areas(popup::content_area(area));
        QueryInput::new(self.state.query(), self.theme.ui()).cursor_position(input)
    }
}

pub fn overlay(model: &Model) -> Option<OverlayContribution> {
    if !model.theme_picker.is_open() {
        return None;
    }
    let picker = ThemePicker::new(&model.theme_picker, &model.theme);
    Some(
        OverlayContribution::new(
            "theme_picker",
            OverlayKind::Modal,
            OverlayPlacement::Centered(picker.size()),
            render_overlay,
        )
        .with_cursor(cursor_overlay),
    )
}

fn render_overlay(model: &Model, frame: &mut Frame, area: Rect) {
    frame.render_widget(ThemePicker::new(&model.theme_picker, &model.theme), area);
}

fn cursor_overlay(model: &Model, area: Rect) -> Option<Position> {
    ThemePicker::new(&model.theme_picker, &model.theme).cursor_position(area)
}

struct ThemePickerContent<'a> {
    state: &'a ThemePickerState,
    theme: &'a Theme,
}

impl Widget for ThemePicker<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Popup::new(
            "Theme",
            ThemePickerContent {
                state: self.state,
                theme: self.theme,
            },
            self.theme.ui(),
        )
        .render(area, buf);
    }
}

impl Widget for ThemePickerContent<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (input, results) = content_areas(area);
        let surface = self.theme.ui().popup_surface();
        QueryInput::new(self.state.query(), self.theme.ui()).render(input, buf);
        let lines = result_lines(self.state, surface, self.theme.ui(), results.height);
        Paragraph::new(lines).style(surface).render(results, buf);
    }
}

fn content_areas(area: Rect) -> (Rect, Rect) {
    Layout::vertical([Constraint::Length(3), Constraint::Fill(1)])
        .areas(area)
        .into()
}

fn result_lines(
    state: &ThemePickerState,
    surface: ratatui::style::Style,
    styles: &UiTheme,
    height: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut selected_row = None;
    for section in state.visible_sections() {
        lines.push(Line::styled(
            section.title,
            surface.patch(styles.action_hint),
        ));
        for (index, entry) in section.entries {
            if state.selected_index() == index {
                selected_row = Some(lines.len());
            }
            let selected = state.selected_index() == index;
            let marker = if selected { ">" } else { " " };
            let style = if selected {
                surface.patch(styles.selection)
            } else {
                surface
            };
            lines.push(Line::styled(format!("{marker} {}", entry.name()), style));
        }
        if state.discovering_provider() == Some(section.provider) {
            lines.push(Line::styled("Loading…", surface.patch(styles.muted)));
        }
    }
    if lines.is_empty() {
        lines.push(Line::styled("No matches", surface.patch(styles.muted)));
    }
    picker::visible_lines(lines, selected_row, height)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use super::*;
    use crate::{
        theme::ThemeName,
        theme_picker::{ThemePickerEntry, ThemeProviderId, ThemeProviderSpec},
    };

    #[test]
    fn renders_builtin_names_and_the_selected_row() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::TokyoNight), "tokyonight");
        let theme = Theme::default();
        let backend = TestBackend::new(28, 9);
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
        let revision = state.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(state.complete_discovery(
            revision,
            vec![ThemePickerEntry::ghostty(
                "local-theme",
                PathBuf::from("/themes/local-theme")
            )],
        ));
        let theme = Theme::default();
        let backend = TestBackend::new(32, 11);
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
    fn renders_newly_registered_provider_sections_without_component_changes() {
        let mut state = ThemePickerState::default();
        state
            .register_provider(ThemeProviderSpec::new(
                ThemeProviderId::new("vscode"),
                "VS Code",
            ))
            .unwrap();
        state.open(Some(ThemeName::Default), "default");
        let revision = state.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(state.complete_discovery(
            revision,
            vec![ThemePickerEntry::file(
                ThemeProviderId::new("vscode"),
                "local-vscode-theme",
                PathBuf::from("/themes/local-vscode-theme"),
            )],
        ));
        let theme = Theme::default();
        let backend = TestBackend::new(36, 12);
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
        assert!(content.contains("VS Code"));
        assert!(content.contains("local-vscode-theme"));
    }

    #[test]
    fn filters_rows_from_the_query_input() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::Default), "default");
        let revision = state.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(state.complete_discovery(
            revision,
            vec![ThemePickerEntry::ghostty(
                "local-theme",
                PathBuf::from("/themes/local-theme")
            )],
        ));
        for character in "LoC".chars() {
            state.push_query(character);
        }
        let theme = Theme::default();
        let backend = TestBackend::new(32, 8);
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
        assert!(content.contains("LoC"));
        assert!(content.contains("local-theme"));
        assert!(!content.contains("catppuccin_mocha"));
    }

    #[test]
    fn keeps_the_selected_theme_visible_in_a_small_terminal() {
        let mut state = ThemePickerState::default();
        state.open(Some(ThemeName::CatppuccinMocha), "catppuccin_mocha");
        let theme = Theme::default();
        let backend = TestBackend::new(28, 6);
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
