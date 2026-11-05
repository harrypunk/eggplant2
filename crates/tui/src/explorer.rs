use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerEntryKind {
    Directory,
    File,
    Other,
}

impl ExplorerEntryKind {
    fn sort_rank(self) -> u8 {
        match self {
            Self::Directory => 0,
            Self::File => 1,
            Self::Other => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: ExplorerEntryKind,
}

#[derive(Debug, Default)]
pub struct ExplorerState {
    pub current_dir: PathBuf,
    pub entries: Vec<ExplorerEntry>,
    pub selected: Option<usize>,
}

impl ExplorerState {
    /// Load a directory's immediate children and select the first entry.
    pub fn load(root: &Path) -> io::Result<Self> {
        let mut entries = std::fs::read_dir(root)?
            .map(|entry| {
                let entry = entry?;
                let path = entry.path();
                let kind = if path.is_dir() {
                    ExplorerEntryKind::Directory
                } else if path.is_file() {
                    ExplorerEntryKind::File
                } else {
                    ExplorerEntryKind::Other
                };
                Ok(ExplorerEntry {
                    name: entry.file_name(),
                    path,
                    kind,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;

        entries.sort_by(|left, right| {
            left.kind
                .sort_rank()
                .cmp(&right.kind.sort_rank())
                .then_with(|| left.name.cmp(&right.name))
        });
        let selected = (!entries.is_empty()).then_some(0);

        Ok(Self {
            current_dir: root.to_path_buf(),
            entries,
            selected,
        })
    }

    pub fn reload(&mut self) -> io::Result<()> {
        *self = Self::load(&self.current_dir)?;
        Ok(())
    }

    /// Navigate to the parent directory without crossing the workspace root.
    pub fn open_parent(&mut self, workspace_root: &Path) -> io::Result<bool> {
        if self.current_dir == workspace_root {
            return Ok(false);
        }

        let Some(parent) = self.current_dir.parent() else {
            return Ok(false);
        };
        if !parent.starts_with(workspace_root) {
            return Ok(false);
        }

        *self = Self::load(parent)?;
        Ok(true)
    }

    pub fn select_next(&mut self) {
        if self.entries.is_empty() {
            self.selected = None;
            return;
        }

        let selected = self.selected.unwrap_or(0);
        self.selected = Some((selected + 1).min(self.entries.len() - 1));
    }

    pub fn select_previous(&mut self) {
        if self.entries.is_empty() {
            self.selected = None;
            return;
        }

        self.selected = Some(self.selected.unwrap_or(0).saturating_sub(1));
    }

    pub fn selected_entry(&self) -> Option<&ExplorerEntry> {
        self.entries.get(self.selected?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_sorted_entries_and_selects_first() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("b.txt"), "b").unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        std::fs::create_dir(directory.path().join("folder")).unwrap();

        let explorer = ExplorerState::load(directory.path()).unwrap();
        let names = explorer
            .entries
            .iter()
            .map(|entry| entry.name.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(names, ["folder", "a.txt", "b.txt"]);
        assert_eq!(explorer.selected, Some(0));
        assert_eq!(
            explorer.selected_entry().unwrap().kind,
            ExplorerEntryKind::Directory
        );
    }

    #[test]
    fn selection_moves_without_wrapping() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("a.txt"), "a").unwrap();
        std::fs::write(directory.path().join("b.txt"), "b").unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();

        explorer.select_previous();
        assert_eq!(explorer.selected, Some(0));
        explorer.select_next();
        assert_eq!(explorer.selected, Some(1));
        explorer.select_next();
        assert_eq!(explorer.selected, Some(1));
        explorer.select_previous();
        assert_eq!(explorer.selected, Some(0));
    }

    #[test]
    fn empty_directory_has_no_selection() {
        let directory = tempfile::tempdir().unwrap();
        let explorer = ExplorerState::load(directory.path()).unwrap();

        assert!(explorer.entries.is_empty());
        assert_eq!(explorer.selected, None);
        assert!(explorer.selected_entry().is_none());
    }
}
