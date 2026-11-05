use std::{
    collections::HashSet,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use super::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerEntryKind {
    Directory,
    File,
    Other,
}

impl ExplorerEntryKind {
    pub(super) fn sort_rank(self) -> u8 {
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
    pub depth: usize,
    pub expanded: bool,
}

/// A flattened, visible tree under a fixed workspace root.
#[derive(Debug, Default)]
pub struct ExplorerState {
    pub entries: Vec<ExplorerEntry>,
    pub selected: Option<usize>,
    expanded_directories: HashSet<PathBuf>,
}

impl ExplorerState {
    /// Load the root's immediate children and select the first entry.
    pub fn load(root: &Path) -> io::Result<Self> {
        let state = Self::default();
        let entries = fs::read_visible_entries(root, &state.expanded_directories)?;
        let selected = (!entries.is_empty()).then_some(0);
        Ok(Self {
            entries,
            selected,
            ..state
        })
    }

    /// Reload visible entries while preserving expansion and selected path.
    pub fn reload(&mut self, root: &Path) -> io::Result<()> {
        self.rebuild(root)
    }

    /// Unfold the selected directory. Returns false for files or expanded directories.
    pub fn unfold_selected(&mut self, root: &Path) -> io::Result<bool> {
        let Some(path) = self.selected_directory() else {
            return Ok(false);
        };
        if !self.expanded_directories.insert(path.clone()) {
            return Ok(false);
        }

        if let Err(error) = self.rebuild(root) {
            self.expanded_directories.remove(&path);
            return Err(error);
        }
        Ok(true)
    }

    /// Fold the selected directory. Returns false for files or folded directories.
    pub fn fold_selected(&mut self, root: &Path) -> io::Result<bool> {
        let Some(path) = self.selected_directory() else {
            return Ok(false);
        };
        if !self.expanded_directories.remove(&path) {
            return Ok(false);
        }

        if let Err(error) = self.rebuild(root) {
            self.expanded_directories.insert(path);
            return Err(error);
        }
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

    fn selected_directory(&self) -> Option<PathBuf> {
        self.selected_entry().and_then(|entry| {
            (entry.kind == ExplorerEntryKind::Directory).then(|| entry.path.clone())
        })
    }

    fn rebuild(&mut self, root: &Path) -> io::Result<()> {
        let selected_path = self.selected_entry().map(|entry| entry.path.clone());
        // Build first so read failures leave the current visible tree untouched.
        let entries = fs::read_visible_entries(root, &self.expanded_directories)?;
        self.selected = selected_path
            .and_then(|path| entries.iter().position(|entry| entry.path == path))
            .or_else(|| (!entries.is_empty()).then_some(0));
        self.entries = entries;
        Ok(())
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
        assert!(explorer.entries.iter().all(|entry| entry.depth == 0));
    }

    #[test]
    fn unfolds_and_folds_selected_directory() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(folder.join("child.txt"), "child").unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();

        assert!(explorer.unfold_selected(directory.path()).unwrap());
        assert_eq!(explorer.entries.len(), 2);
        assert!(explorer.entries[0].expanded);
        assert_eq!(explorer.entries[1].depth, 1);
        assert_eq!(explorer.selected, Some(0));

        assert!(explorer.fold_selected(directory.path()).unwrap());
        assert_eq!(explorer.entries.len(), 1);
        assert!(!explorer.entries[0].expanded);
        assert_eq!(explorer.selected, Some(0));
        assert!(!explorer.fold_selected(directory.path()).unwrap());
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
