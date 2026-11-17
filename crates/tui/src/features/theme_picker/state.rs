use crate::{data_object::Visibility, theme::ThemeName};

/// Selection state for the modal built-in theme list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePickerState {
    visibility: Visibility,
    selected: ThemeName,
}

impl Default for ThemePickerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            selected: ThemeName::Default,
        }
    }
}

impl ThemePickerState {
    pub fn open(&mut self, active: ThemeName) {
        self.visibility = Visibility::Displayed;
        self.selected = active;
    }

    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
    }

    pub fn is_open(&self) -> bool {
        self.visibility == Visibility::Displayed
    }

    pub fn selected(&self) -> ThemeName {
        self.selected
    }

    pub fn select_next(&mut self) {
        let selected = Self::selected_index(self.selected);
        self.selected = ThemeName::ALL[(selected + 1) % ThemeName::ALL.len()];
    }

    pub fn select_previous(&mut self) {
        let selected = Self::selected_index(self.selected);
        self.selected =
            ThemeName::ALL[(selected + ThemeName::ALL.len() - 1) % ThemeName::ALL.len()];
    }

    fn selected_index(selected: ThemeName) -> usize {
        ThemeName::ALL
            .iter()
            .position(|candidate| *candidate == selected)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_the_active_theme_and_wraps_navigation() {
        let mut picker = ThemePickerState::default();
        picker.open(ThemeName::TokyoNight);
        assert!(picker.is_open());
        assert_eq!(picker.selected(), ThemeName::TokyoNight);

        picker.select_next();
        assert_eq!(picker.selected(), ThemeName::CatppuccinMocha);
        picker.select_next();
        assert_eq!(picker.selected(), ThemeName::Default);
        picker.select_previous();
        assert_eq!(picker.selected(), ThemeName::CatppuccinMocha);
        picker.select_previous();
        assert_eq!(picker.selected(), ThemeName::TokyoNight);
        picker.select_previous();
        assert_eq!(picker.selected(), ThemeName::Default);
        picker.select_previous();
        assert_eq!(picker.selected(), ThemeName::CatppuccinMocha);

        picker.close();
        assert!(!picker.is_open());
    }
}
