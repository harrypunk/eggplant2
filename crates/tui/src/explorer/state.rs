use std::{
    collections::HashSet,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use super::fs;
use crate::data_object::Visibility;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerEntryKind {
    Directory,
    File,
    Link,
    Other,
}

impl ExplorerEntryKind {
    pub(super) fn sort_rank(self) -> u8 {
        match self {
            Self::Directory => 0,
            Self::File => 1,
            Self::Link => 2,
            Self::Other => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpansionState {
    Expanded,
    Collapsed,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum IgnoreMode {
    #[default]
    HideIgnored,
    ShowIgnored,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplorerEntry {
    pub name: OsString,
    pub path: PathBuf,
    pub kind: ExplorerEntryKind,
    pub depth: usize,
    pub expansion: ExpansionState,
}

/// Explorer tree and panel visibility under a fixed workspace root.
#[derive(Debug)]
pub struct ExplorerState {
    pub visibility: Visibility,
    pub entries: Vec<ExplorerEntry>,
    pub selected: Option<usize>,
    expanded_directories: HashSet<PathBuf>,
    ignore_mode: IgnoreMode,
}

impl Default for ExplorerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Displayed,
            entries: Vec::new(),
            selected: None,
            expanded_directories: HashSet::new(),
            ignore_mode: IgnoreMode::HideIgnored,
        }
    }
}

impl ExplorerState {
    /// Load the root's immediate children and select the first entry.
    pub fn load(root: &Path) -> io::Result<Self> {
        let state = Self::default();
        let entries =
            fs::read_visible_entries(root, &state.expanded_directories, state.ignore_mode)?;
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

    /// Toggle Git-ignore filtering without losing expansion or selection state.
    pub fn toggle_ignore_mode(&mut self, root: &Path) -> io::Result<IgnoreMode> {
        let previous = self.ignore_mode;
        self.ignore_mode = match previous {
            IgnoreMode::HideIgnored => IgnoreMode::ShowIgnored,
            IgnoreMode::ShowIgnored => IgnoreMode::HideIgnored,
        };
        if let Err(error) = self.rebuild(root) {
            self.ignore_mode = previous;
            return Err(error);
        }
        Ok(self.ignore_mode)
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

    /// Expand ancestors and select an opened file when it is visible.
    ///
    /// Paths outside the workspace or hidden by ignore rules leave the current
    /// tree unchanged. Read failures are transactional as well.
    pub fn reveal(&mut self, root: &Path, path: &Path) -> io::Result<()> {
        let Ok(relative) = path.strip_prefix(root) else {
            return Ok(());
        };
        let mut expanded_directories = self.expanded_directories.clone();
        expanded_directories.extend(
            relative
                .parent()
                .into_iter()
                .flat_map(Path::ancestors)
                .filter(|ancestor| !ancestor.as_os_str().is_empty())
                .map(|ancestor| root.join(ancestor)),
        );

        let entries = fs::read_visible_entries(root, &expanded_directories, self.ignore_mode)?;
        let Some(selected) = entries.iter().position(|entry| entry.path == path) else {
            return Ok(());
        };
        self.expanded_directories = expanded_directories;
        self.entries = entries;
        self.selected = Some(selected);
        Ok(())
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
        let entries = fs::read_visible_entries(root, &self.expanded_directories, self.ignore_mode)?;
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
    fn filters_gitignore_and_built_in_entries_and_toggles_them() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("folder");
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(directory.path().join(".gitignore"), "ignored.txt\n").unwrap();
        std::fs::write(directory.path().join("ignored.txt"), "ignored").unwrap();
        std::fs::write(directory.path().join("visible.txt"), "visible").unwrap();
        std::fs::write(folder.join(".gitignore"), "child-ignored.txt\n").unwrap();
        std::fs::write(folder.join("child-ignored.txt"), "ignored").unwrap();
        std::fs::write(folder.join("child-visible.txt"), "visible").unwrap();
        for name in [".git", "node_modules", "__pycache__"] {
            std::fs::create_dir(directory.path().join(name)).unwrap();
        }
        let names = |explorer: &ExplorerState| {
            explorer
                .entries
                .iter()
                .map(|entry| entry.name.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        let mut explorer = ExplorerState::load(directory.path()).unwrap();

        assert_eq!(explorer.ignore_mode, IgnoreMode::HideIgnored);
        assert_eq!(names(&explorer), ["folder", ".gitignore", "visible.txt"]);

        explorer.unfold_selected(directory.path()).unwrap();
        assert!(!names(&explorer).contains(&"child-ignored.txt".to_string()));
        assert!(names(&explorer).contains(&"child-visible.txt".to_string()));

        assert_eq!(
            explorer.toggle_ignore_mode(directory.path()).unwrap(),
            IgnoreMode::ShowIgnored
        );
        let shown = names(&explorer);
        assert!(shown.contains(&"ignored.txt".to_string()));
        assert!(shown.contains(&"child-ignored.txt".to_string()));
        for name in [".git", "node_modules", "__pycache__"] {
            assert!(shown.contains(&name.to_string()));
        }
        assert_eq!(
            explorer.selected_entry().map(|entry| entry.path.as_path()),
            Some(folder.as_path())
        );

        assert_eq!(
            explorer.toggle_ignore_mode(directory.path()).unwrap(),
            IgnoreMode::HideIgnored
        );
        let hidden = names(&explorer);
        assert!(!hidden.contains(&"ignored.txt".to_string()));
        assert!(!hidden.contains(&"child-ignored.txt".to_string()));
        for name in [".git", "node_modules", "__pycache__"] {
            assert!(!hidden.contains(&name.to_string()));
        }
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
        assert_eq!(explorer.entries[0].expansion, ExpansionState::Expanded);
        assert_eq!(explorer.entries[1].depth, 1);
        assert_eq!(explorer.selected, Some(0));

        assert!(explorer.fold_selected(directory.path()).unwrap());
        assert_eq!(explorer.entries.len(), 1);
        assert_eq!(explorer.entries[0].expansion, ExpansionState::Collapsed);
        assert_eq!(explorer.selected, Some(0));
        assert!(!explorer.fold_selected(directory.path()).unwrap());
    }

    #[test]
    fn reveal_expands_ancestors_and_selects_the_opened_file() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("folder/nested");
        let file = nested.join("note.txt");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(&file, "hello").unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();

        explorer.reveal(directory.path(), &file).unwrap();

        assert_eq!(explorer.selected_entry().unwrap().path, file);
        assert!(
            explorer
                .entries
                .iter()
                .filter(|entry| entry.kind == ExplorerEntryKind::Directory)
                .all(|entry| entry.expansion == ExpansionState::Expanded)
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
