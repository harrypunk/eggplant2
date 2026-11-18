use std::path::PathBuf;

use crate::{data_object::Visibility, theme::ThemeName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemePickerEntry {
    Builtin(ThemeName),
    Ghostty { name: String, path: PathBuf },
}

impl ThemePickerEntry {
    pub fn name(&self) -> &str {
        match self {
            Self::Builtin(name) => name.as_str(),
            Self::Ghostty { name, .. } => name,
        }
    }

    fn identity(&self) -> ThemePickerEntryIdentity {
        match self {
            Self::Builtin(name) => ThemePickerEntryIdentity::Builtin(*name),
            Self::Ghostty { path, .. } => ThemePickerEntryIdentity::Ghostty(path.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThemePickerEntryIdentity {
    Builtin(ThemeName),
    Ghostty(PathBuf),
}

#[derive(Debug, Default)]
struct DiscoveryState {
    revision: u64,
    loading: bool,
}

/// Selection and discovery state for built-in and local provider themes.
#[derive(Debug)]
pub struct ThemePickerState {
    visibility: Visibility,
    entries: Vec<ThemePickerEntry>,
    selected: usize,
    preferred_external: Option<String>,
    discovery: DiscoveryState,
}

impl Default for ThemePickerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            entries: builtin_entries(),
            selected: 0,
            preferred_external: None,
            discovery: DiscoveryState::default(),
        }
    }
}

impl ThemePickerState {
    pub fn open(&mut self, active: Option<ThemeName>, active_name: &str) {
        self.visibility = Visibility::Displayed;
        self.preferred_external = active.is_none().then(|| active_name.to_string());
        if let Some(active) = active {
            self.selected = self
                .entries
                .iter()
                .position(|entry| *entry == ThemePickerEntry::Builtin(active))
                .unwrap_or(0);
        } else if let Some(selected) = self
            .entries
            .iter()
            .position(|entry| entry.name() == active_name)
        {
            self.selected = selected;
        }
    }

    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
        self.discovery.revision = self.discovery.revision.wrapping_add(1);
        self.discovery.loading = false;
    }

    pub fn is_open(&self) -> bool {
        self.visibility == Visibility::Displayed
    }

    pub fn selected(&self) -> &ThemePickerEntry {
        &self.entries[self.selected]
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn entries(&self) -> &[ThemePickerEntry] {
        &self.entries
    }

    pub fn is_discovering(&self) -> bool {
        self.discovery.loading
    }

    pub fn select_next(&mut self) {
        self.preferred_external = None;
        self.selected = (self.selected + 1) % self.entries.len();
    }

    pub fn select_previous(&mut self) {
        self.preferred_external = None;
        self.selected = (self.selected + self.entries.len() - 1) % self.entries.len();
    }

    pub(crate) fn begin_discovery(&mut self) -> Option<u64> {
        self.discovery.loading = true;
        self.discovery.revision = self.discovery.revision.wrapping_add(1);
        Some(self.discovery.revision)
    }

    pub(crate) fn complete_discovery(
        &mut self,
        revision: u64,
        entries: Vec<ThemePickerEntry>,
    ) -> bool {
        if !self.discovery.loading || self.discovery.revision != revision {
            return false;
        }
        self.discovery.loading = false;
        let selected = self.selected().identity();
        let preferred = self.preferred_external.take();
        self.entries = builtin_entries().into_iter().chain(entries).collect();
        if let Some(preferred) = preferred
            && let Some(index) = self
                .entries
                .iter()
                .position(|entry| entry.name() == preferred)
        {
            self.selected = index;
            return true;
        }
        self.selected = self
            .entries
            .iter()
            .position(|entry| entry.identity() == selected)
            .unwrap_or(0);
        true
    }

    pub(crate) fn fail_discovery(&mut self, revision: u64) -> bool {
        if !self.discovery.loading || self.discovery.revision != revision {
            return false;
        }
        self.discovery.loading = false;
        true
    }
}

fn builtin_entries() -> Vec<ThemePickerEntry> {
    ThemeName::ALL
        .into_iter()
        .map(ThemePickerEntry::Builtin)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ghostty(name: &str) -> ThemePickerEntry {
        ThemePickerEntry::Ghostty {
            name: name.to_string(),
            path: PathBuf::from(format!("/themes/{name}")),
        }
    }

    #[test]
    fn opens_on_the_active_theme_and_wraps_navigation() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::TokyoNight), "tokyonight");
        assert!(picker.is_open());
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::TokyoNight)
        );

        picker.select_next();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::CatppuccinMocha)
        );
        picker.select_next();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::Default)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::CatppuccinMocha)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::TokyoNight)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::Default)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::Builtin(ThemeName::CatppuccinMocha)
        );

        picker.close();
        assert!(!picker.is_open());
    }

    #[test]
    fn discovery_adds_provider_entries_and_preserves_selection() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::Default), "default");
        let revision = picker.begin_discovery().unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("zeta"), ghostty("alpha")]));
        assert_eq!(picker.entries().len(), 5);

        picker.select_next();
        picker.select_next();
        picker.select_next();
        assert_eq!(picker.selected().name(), "zeta");
        let revision = picker.begin_discovery().unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("alpha"), ghostty("zeta")]));
        assert_eq!(picker.selected().name(), "zeta");
    }

    #[test]
    fn active_external_theme_is_selected_after_discovery() {
        let mut picker = ThemePickerState::default();
        picker.open(None, "alpha");
        let revision = picker.begin_discovery().unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("alpha")]));
        assert_eq!(picker.selected().name(), "alpha");
    }

    #[test]
    fn stale_or_closed_discovery_cannot_replace_entries() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::Default), "default");
        let first = picker.begin_discovery().unwrap();
        let second = picker.begin_discovery().unwrap();
        assert!(!picker.complete_discovery(first, vec![ghostty("stale")]));
        assert_eq!(picker.entries().len(), 3);
        assert!(picker.complete_discovery(second, vec![ghostty("current")]));
        assert_eq!(picker.entries().len(), 4);

        let revision = picker.begin_discovery().unwrap();
        picker.close();
        assert!(!picker.complete_discovery(revision, vec![ghostty("stale")]));
        assert_eq!(picker.entries().len(), 4);
    }
}
