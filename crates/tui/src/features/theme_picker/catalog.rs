use color_eyre::eyre::eyre;

use super::ThemePickerEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThemeProviderId(&'static str);

impl ThemeProviderId {
    pub const BUILTIN: Self = Self("builtin");
    pub const GHOSTTY: Self = Self("ghostty");

    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub fn as_str(self) -> &'static str {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeProviderSpec {
    pub id: ThemeProviderId,
    pub title: &'static str,
}

impl ThemeProviderSpec {
    pub const fn new(id: ThemeProviderId, title: &'static str) -> Self {
        Self { id, title }
    }
}

#[derive(Debug, Clone)]
pub struct ThemeProviderRegistry {
    providers: Vec<ThemeProviderSpec>,
}

impl Default for ThemeProviderRegistry {
    fn default() -> Self {
        Self {
            providers: vec![
                ThemeProviderSpec::new(ThemeProviderId::BUILTIN, "builtin"),
                ThemeProviderSpec::new(ThemeProviderId::GHOSTTY, "ghostty"),
            ],
        }
    }
}

impl ThemeProviderRegistry {
    pub fn register(&mut self, provider: ThemeProviderSpec) -> color_eyre::Result<()> {
        if self
            .providers
            .iter()
            .any(|candidate| candidate.id == provider.id)
        {
            return Err(eyre!("duplicate theme provider `{}`", provider.id.as_str()));
        }
        self.providers.push(provider);
        Ok(())
    }

    pub fn sections<'a>(
        &'a self,
        entries: Vec<(usize, &'a ThemePickerEntry)>,
        discovering: Option<ThemeProviderId>,
    ) -> Vec<ThemeSection<'a>> {
        self.providers
            .iter()
            .filter_map(|provider| {
                let entries = entries
                    .iter()
                    .filter(|(_, entry)| entry.provider() == provider.id)
                    .copied()
                    .collect::<Vec<_>>();
                if entries.is_empty() && discovering != Some(provider.id) {
                    return None;
                }
                Some(ThemeSection {
                    provider: provider.id,
                    title: provider.title,
                    entries,
                })
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct ThemeSection<'a> {
    pub provider: ThemeProviderId,
    pub title: &'static str,
    pub entries: Vec<(usize, &'a ThemePickerEntry)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme_picker::ThemePickerEntry;

    #[test]
    fn registration_is_ordered_and_rejects_duplicates() {
        let mut registry = ThemeProviderRegistry::default();
        registry
            .register(ThemeProviderSpec::new(
                ThemeProviderId::new("vscode"),
                "VS Code",
            ))
            .unwrap();
        assert!(
            registry
                .register(ThemeProviderSpec::new(
                    ThemeProviderId::new("vscode"),
                    "duplicate",
                ))
                .is_err()
        );

        let entry = ThemePickerEntry::file(
            ThemeProviderId::new("vscode"),
            "example",
            std::path::PathBuf::from("/themes/example"),
        );
        let entries = vec![(0, &entry)];
        let sections = registry.sections(entries, None);
        assert_eq!(sections.len(), 1);
        assert_eq!(sections[0].title, "VS Code");
        assert_eq!(sections[0].entries.len(), 1);
    }
}
