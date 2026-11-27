use crate::data_object::Context;

pub type CommandBinding = (&'static str, &'static str);

/// Shared behavior composed into a context without central feature-membership lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeymapCapability {
    ListNavigation,
    /// List navigation plus `j`/`k` for contexts without printable text input.
    VimListNavigation,
    ModalActions,
    /// Permission to open the configured leader menu; the chord is resolved at runtime.
    LeaderMenu,
    /// Shared form focus and clipboard actions.
    FormActions,
    /// Insert-mode editing commands shared by editor sessions.
    InsertEditing,
    /// Vim-style cursor motions shared by normal/visual editing.
    Motions,
}

/// A feature-owned keymap declaration using its stable configuration name and typed context.
#[derive(Debug, Clone, Copy)]
pub struct ContextKeymap {
    pub name: &'static str,
    pub context: Context,
    pub commands: &'static [CommandBinding],
    pub capabilities: &'static [KeymapCapability],
}

impl ContextKeymap {
    pub const fn new(name: &'static str, context: Context) -> Self {
        Self {
            name,
            context,
            commands: &[],
            capabilities: &[],
        }
    }

    pub const fn with_commands(mut self, commands: &'static [CommandBinding]) -> Self {
        self.commands = commands;
        self
    }

    pub const fn with_capabilities(mut self, capabilities: &'static [KeymapCapability]) -> Self {
        self.capabilities = capabilities;
        self
    }
}

/// Static feature declarations assembled into one explicit input-composition container.
#[derive(Debug, Default, Clone)]
pub struct KeymapCatalog {
    entries: Vec<ContextKeymap>,
}

impl KeymapCatalog {
    pub fn extend(mut self, entries: impl IntoIterator<Item = ContextKeymap>) -> Self {
        self.entries.extend(entries);
        self
    }

    pub fn iter(&self) -> impl Iterator<Item = &ContextKeymap> {
        self.entries.iter()
    }

    pub fn contexts_with(
        &self,
        capability: KeymapCapability,
    ) -> impl Iterator<Item = Context> + '_ {
        self.entries
            .iter()
            .filter(move |entry| entry.capabilities.contains(&capability))
            .map(|entry| entry.context)
    }
}
