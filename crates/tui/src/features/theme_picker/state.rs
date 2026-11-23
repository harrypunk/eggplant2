use std::path::PathBuf;

use crate::{component::query_input::Query, data_object::Visibility, theme::ThemeName};

use super::{ThemeProviderId, ThemeProviderRegistry, ThemeSection};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemePickerEntry {
    provider: ThemeProviderId,
    name: String,
    source: ThemeEntrySource,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThemeEntrySource {
    Builtin(ThemeName),
    File(PathBuf),
}

impl ThemePickerEntry {
    pub fn builtin(name: ThemeName) -> Self {
        Self {
            provider: ThemeProviderId::BUILTIN,
            name: name.as_str().to_string(),
            source: ThemeEntrySource::Builtin(name),
        }
    }

    pub fn ghostty(name: impl Into<String>, path: PathBuf) -> Self {
        Self::file(ThemeProviderId::GHOSTTY, name, path)
    }

    pub fn file(provider: ThemeProviderId, name: impl Into<String>, path: PathBuf) -> Self {
        Self {
            provider,
            name: name.into(),
            source: ThemeEntrySource::File(path),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn provider(&self) -> ThemeProviderId {
        self.provider
    }

    pub fn builtin_name(&self) -> Option<ThemeName> {
        match self.source {
            ThemeEntrySource::Builtin(name) => Some(name),
            ThemeEntrySource::File(_) => None,
        }
    }

    pub fn file_path(&self) -> Option<&std::path::Path> {
        match &self.source {
            ThemeEntrySource::File(path) => Some(path),
            ThemeEntrySource::Builtin(_) => None,
        }
    }

    fn identity(&self) -> ThemePickerEntryIdentity {
        match &self.source {
            ThemeEntrySource::Builtin(name) => ThemePickerEntryIdentity::Builtin(*name),
            ThemeEntrySource::File(path) => ThemePickerEntryIdentity::File(path.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThemePickerEntryIdentity {
    Builtin(ThemeName),
    File(PathBuf),
}

#[derive(Debug, Default)]
struct DiscoveryState {
    revision: u64,
    loading: bool,
    provider: Option<ThemeProviderId>,
}

/// Selection and discovery state for built-in and local provider themes.
#[derive(Debug)]
pub struct ThemePickerState {
    visibility: Visibility,
    entries: Vec<ThemePickerEntry>,
    selected: usize,
    preferred_external: Option<String>,
    query: Query,
    discovery: DiscoveryState,
    providers: ThemeProviderRegistry,
}

impl Default for ThemePickerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            entries: builtin_entries(),
            selected: 0,
            preferred_external: None,
            query: Query::default(),
            discovery: DiscoveryState::default(),
            providers: ThemeProviderRegistry::default(),
        }
    }
}

impl ThemePickerState {
    pub fn open(&mut self, active: Option<ThemeName>, active_name: &str) {
        self.visibility = Visibility::Displayed;
        self.query.clear();
        self.preferred_external = active.is_none().then(|| active_name.to_string());
        if let Some(active) = active {
            self.selected = self
                .entries
                .iter()
                .position(|entry| *entry == ThemePickerEntry::builtin(active))
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
        self.discovery.provider = None;
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

    pub fn visible_entries(&self) -> Vec<(usize, &ThemePickerEntry)> {
        self.entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.query.matches(entry.name()))
            .collect()
    }

    pub fn register_provider(
        &mut self,
        provider: super::ThemeProviderSpec,
    ) -> color_eyre::Result<()> {
        self.providers.register(provider)
    }

    pub fn visible_sections(&self) -> Vec<ThemeSection<'_>> {
        self.providers
            .sections(self.visible_entries(), self.discovering_provider())
    }

    pub fn discovering_provider(&self) -> Option<ThemeProviderId> {
        self.discovery
            .loading
            .then_some(self.discovery.provider)
            .flatten()
    }
    pub fn query(&self) -> &str {
        self.query.as_str()
    }

    pub fn query_state(&self) -> &Query {
        &self.query
    }

    pub fn handle_query_event(
        &mut self,
        event: crate::component::text_input::TextInputEvent,
    ) -> crate::component::text_input::TextInputChange {
        let change = self.query.handle_event(event);
        if change.content_changed() {
            self.preferred_external = None;
            self.select_first_visible();
        }
        change
    }

    pub fn push_query(&mut self, character: char) {
        self.handle_query_event(crate::component::text_input::TextInputEvent::Insert(
            character,
        ));
    }

    pub fn backspace_query(&mut self) {
        self.preferred_external = None;
        self.query.pop();
        self.select_first_visible();
    }

    pub fn selection_is_visible(&self) -> bool {
        self.query.matches(self.selected().name())
    }

    pub fn is_discovering(&self) -> bool {
        self.discovery.loading
    }

    pub fn select_next(&mut self) {
        self.preferred_external = None;
        let visible = self.visible_entries();
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|(index, _)| *index == self.selected)
            .unwrap_or(0);
        self.selected = visible[(current + 1) % visible.len()].0;
    }

    pub fn select_previous(&mut self) {
        self.preferred_external = None;
        let visible = self.visible_entries();
        if visible.is_empty() {
            return;
        }
        let current = visible
            .iter()
            .position(|(index, _)| *index == self.selected)
            .unwrap_or(0);
        self.selected = visible[(current + visible.len() - 1) % visible.len()].0;
    }

    fn select_first_visible(&mut self) {
        self.selected = self
            .visible_entries()
            .first()
            .map(|(index, _)| *index)
            .unwrap_or(0);
    }

    pub(crate) fn begin_discovery(&mut self, provider: ThemeProviderId) -> Option<u64> {
        self.discovery.loading = true;
        self.discovery.provider = Some(provider);
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
        self.discovery.provider = None;
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
            .position(|entry| entry.identity() == selected && self.query.matches(entry.name()))
            .unwrap_or_else(|| {
                self.visible_entries()
                    .first()
                    .map(|(index, _)| *index)
                    .unwrap_or(0)
            });
        true
    }

    pub(crate) fn fail_discovery(&mut self, revision: u64) -> bool {
        if !self.discovery.loading || self.discovery.revision != revision {
            return false;
        }
        self.discovery.loading = false;
        self.discovery.provider = None;
        true
    }
}

fn builtin_entries() -> Vec<ThemePickerEntry> {
    ThemeName::ALL
        .into_iter()
        .map(ThemePickerEntry::builtin)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ghostty(name: &str) -> ThemePickerEntry {
        ThemePickerEntry::ghostty(name, PathBuf::from(format!("/themes/{name}")))
    }

    #[test]
    fn opens_on_the_active_theme_and_wraps_navigation() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::TokyoNight), "tokyonight");
        assert!(picker.is_open());
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );

        picker.select_next();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::CatppuccinMocha)
        );
        picker.select_next();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::Default)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::CatppuccinMocha)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::TokyoNight)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::Default)
        );
        picker.select_previous();
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::CatppuccinMocha)
        );

        picker.close();
        assert!(!picker.is_open());
    }

    #[test]
    fn discovery_adds_provider_entries_and_preserves_selection() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::Default), "default");
        let revision = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("zeta"), ghostty("alpha")]));
        assert_eq!(picker.entries().len(), 5);

        picker.select_next();
        picker.select_next();
        picker.select_next();
        assert_eq!(picker.selected().name(), "zeta");
        let revision = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("alpha"), ghostty("zeta")]));
        assert_eq!(picker.selected().name(), "zeta");
    }

    #[test]
    fn active_external_theme_is_selected_after_discovery() {
        let mut picker = ThemePickerState::default();
        picker.open(None, "alpha");
        let revision = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("alpha")]));
        assert_eq!(picker.selected().name(), "alpha");
    }

    #[test]
    fn query_filters_case_insensitively_and_limits_navigation() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::Default), "default");
        let revision = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(picker.complete_discovery(revision, vec![ghostty("local-theme")]));

        for character in "LoC".chars() {
            picker.push_query(character);
        }
        assert_eq!(picker.visible_entries().len(), 1);
        assert_eq!(picker.selected().name(), "local-theme");
        picker.select_next();
        assert_eq!(picker.selected().name(), "local-theme");
        picker.select_previous();
        assert_eq!(picker.selected().name(), "local-theme");

        picker.push_query('z');
        assert!(picker.visible_entries().is_empty());
        assert!(!picker.selection_is_visible());
        for _ in 0..4 {
            picker.backspace_query();
        }
        assert!(picker.selection_is_visible());
        assert_eq!(
            picker.selected(),
            &ThemePickerEntry::builtin(ThemeName::Default)
        );
    }

    #[test]
    fn stale_or_closed_discovery_cannot_replace_entries() {
        let mut picker = ThemePickerState::default();
        picker.open(Some(ThemeName::Default), "default");
        let first = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        let second = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        assert!(!picker.complete_discovery(first, vec![ghostty("stale")]));
        assert_eq!(picker.entries().len(), 3);
        assert!(picker.complete_discovery(second, vec![ghostty("current")]));
        assert_eq!(picker.entries().len(), 4);

        let revision = picker.begin_discovery(ThemeProviderId::GHOSTTY).unwrap();
        picker.close();
        assert!(!picker.complete_discovery(revision, vec![ghostty("stale")]));
        assert_eq!(picker.entries().len(), 4);
    }
}
