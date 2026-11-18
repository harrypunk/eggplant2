use std::{
    collections::HashSet,
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

use super::{
    fs,
    work::{ExplorerAction, ExplorerRefreshKind, ExplorerRequest},
};
use crate::{data_object::Visibility, navigation::VerticalNavigation};

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
    refresh_generation: u64,
}

impl Default for ExplorerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Displayed,
            entries: Vec::new(),
            selected: None,
            expanded_directories: HashSet::new(),
            ignore_mode: IgnoreMode::HideIgnored,
            refresh_generation: 0,
        }
    }
}

struct FoldPlan {
    index: usize,
    end: usize,
    path: PathBuf,
}

fn fold_plan(
    entries: &[ExplorerEntry],
    selected: usize,
    expanded_directories: &HashSet<PathBuf>,
) -> Option<FoldPlan> {
    let entry = entries.get(selected)?;
    let index = if entry.kind == ExplorerEntryKind::Directory
        && expanded_directories.contains(&entry.path)
    {
        selected
    } else {
        parent_directory_index(entries, selected)?
    };
    let target = entries.get(index)?;
    if !expanded_directories.contains(&target.path) {
        return None;
    }
    let end = entries[index + 1..]
        .iter()
        .position(|entry| entry.depth <= target.depth)
        .map(|offset| index + 1 + offset)
        .unwrap_or(entries.len());
    Some(FoldPlan {
        index,
        end,
        path: target.path.clone(),
    })
}

fn parent_directory_index(entries: &[ExplorerEntry], selected: usize) -> Option<usize> {
    let depth = entries.get(selected)?.depth.checked_sub(1)?;
    entries[..selected]
        .iter()
        .rposition(|entry| entry.depth == depth && entry.kind == ExplorerEntryKind::Directory)
}

impl ExplorerState {
    pub(crate) fn reload_request(
        &mut self,
        root: &Path,
        kind: ExplorerRefreshKind,
    ) -> ExplorerRequest {
        self.request(
            root,
            self.expanded_directories.clone(),
            self.ignore_mode,
            ExplorerAction::Refresh(kind),
        )
    }

    pub(crate) fn unfold_request(&mut self, root: &Path) -> Option<ExplorerRequest> {
        let entry = self.selected_entry()?;
        if entry.kind != ExplorerEntryKind::Directory {
            return None;
        }
        let path = entry.path.clone();
        let depth = entry.depth;
        let mut expanded_directories = self.expanded_directories.clone();
        if !expanded_directories.insert(path.clone()) {
            return None;
        }
        Some(self.request(
            root,
            expanded_directories,
            self.ignore_mode,
            ExplorerAction::Expansion { path, depth },
        ))
    }

    /// Fold the selected directory or nearest visible parent without touching the filesystem.
    pub(crate) fn fold_selected(&mut self) -> bool {
        let Some(selected) = self.selected else {
            return false;
        };
        let Some(plan) = fold_plan(&self.entries, selected, &self.expanded_directories) else {
            return false;
        };
        self.expanded_directories.remove(&plan.path);
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        self.entries.drain(plan.index + 1..plan.end);
        self.entries[plan.index].expansion = ExpansionState::Collapsed;
        self.selected = Some(plan.index);
        true
    }

    pub(crate) fn toggle_ignore_request(&mut self, root: &Path) -> ExplorerRequest {
        let ignore_mode = match self.ignore_mode {
            IgnoreMode::HideIgnored => IgnoreMode::ShowIgnored,
            IgnoreMode::ShowIgnored => IgnoreMode::HideIgnored,
        };
        self.request(
            root,
            self.expanded_directories.clone(),
            ignore_mode,
            ExplorerAction::Ignore(ignore_mode),
        )
    }

    pub(crate) fn reveal_request(&mut self, root: &Path, path: &Path) -> Option<ExplorerRequest> {
        let relative = path.strip_prefix(root).ok()?;
        let mut expanded_directories = self.expanded_directories.clone();
        expanded_directories.extend(
            relative
                .parent()
                .into_iter()
                .flat_map(Path::ancestors)
                .filter(|ancestor| !ancestor.as_os_str().is_empty())
                .map(|ancestor| root.join(ancestor)),
        );
        Some(self.request(
            root,
            expanded_directories,
            self.ignore_mode,
            ExplorerAction::Reveal {
                path: path.to_path_buf(),
            },
        ))
    }

    fn request(
        &mut self,
        root: &Path,
        expanded_directories: HashSet<PathBuf>,
        ignore_mode: IgnoreMode,
        action: ExplorerAction,
    ) -> ExplorerRequest {
        self.refresh_generation = self.refresh_generation.wrapping_add(1);
        ExplorerRequest {
            root: root.to_path_buf(),
            expanded_directories,
            ignore_mode,
            selected_path: self.selected_entry().map(|entry| entry.path.clone()),
            generation: self.refresh_generation,
            action,
        }
    }

    pub(super) fn refresh_generation(&self) -> u64 {
        self.refresh_generation
    }

    pub(super) fn replace_tree(
        &mut self,
        entries: Vec<ExplorerEntry>,
        expanded_directories: HashSet<PathBuf>,
        ignore_mode: IgnoreMode,
        selected_path: Option<PathBuf>,
    ) {
        self.selected = selected_path
            .and_then(|path| entries.iter().position(|entry| entry.path == path))
            .or_else(|| (!entries.is_empty()).then_some(0));
        self.entries = entries;
        self.expanded_directories = expanded_directories;
        self.ignore_mode = ignore_mode;
    }

    pub(super) fn expand_directory(
        &mut self,
        request: &ExplorerRequest,
        entries: Vec<ExplorerEntry>,
    ) -> bool {
        let ExplorerAction::Expansion { path, .. } = &request.action else {
            return false;
        };
        let Some(index) = self.entries.iter().position(|entry| &entry.path == path) else {
            return false;
        };
        if self.entries[index].kind != ExplorerEntryKind::Directory {
            return false;
        }

        let depth = self.entries[index].depth;
        let end = self.entries[index + 1..]
            .iter()
            .position(|entry| entry.depth <= depth)
            .map(|offset| index + 1 + offset)
            .unwrap_or(self.entries.len());
        drop(self.entries.splice(index + 1..end, entries));
        self.entries[index].expansion = ExpansionState::Expanded;
        self.expanded_directories = request.expanded_directories.clone();
        self.selected = request
            .selected_path
            .as_ref()
            .and_then(|path| self.entries.iter().position(|entry| &entry.path == path))
            .or_else(|| (!self.entries.is_empty()).then_some(0));
        true
    }

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

    /// Toggle Git-ignore filtering in synchronous component/state tests.
    #[cfg(test)]
    pub fn toggle_ignore_mode(&mut self, root: &Path) -> io::Result<IgnoreMode> {
        let request = self.toggle_ignore_request(root);
        let mode = request.ignore_mode;
        self.complete_test_request(request)?;
        Ok(mode)
    }

    /// Unfold the selected directory in synchronous component/state tests.
    #[cfg(test)]
    pub fn unfold_selected(&mut self, root: &Path) -> io::Result<bool> {
        let Some(request) = self.unfold_request(root) else {
            return Ok(false);
        };
        self.complete_test_request(request)?;
        Ok(true)
    }

    #[cfg(test)]
    fn complete_test_request(&mut self, request: ExplorerRequest) -> io::Result<()> {
        match self.complete(request.execute()) {
            super::work::ExplorerApply::Applied(_) | super::work::ExplorerApply::Unchanged => {
                Ok(())
            }
            super::work::ExplorerApply::Failed(_, error) => Err(error),
            super::work::ExplorerApply::Ignored => {
                unreachable!("a single synchronous test request cannot become stale")
            }
        }
    }

    /// Expand ancestors and select an opened file in synchronous state tests.
    #[cfg(test)]
    pub fn reveal(&mut self, root: &Path, path: &Path) -> io::Result<()> {
        let Some(request) = self.reveal_request(root, path) else {
            return Ok(());
        };
        self.complete_test_request(request)
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

impl VerticalNavigation for ExplorerState {
    fn item_count(&self) -> usize {
        self.entries.len()
    }

    fn move_to(&mut self, index: usize) {
        self.selected = self
            .entries
            .len()
            .checked_sub(1)
            .map(|last| index.min(last));
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

        assert!(explorer.fold_selected());
        assert_eq!(explorer.entries.len(), 1);
        assert_eq!(explorer.entries[0].expansion, ExpansionState::Collapsed);
        assert_eq!(explorer.selected, Some(0));
        assert!(!explorer.fold_selected());
    }

    #[test]
    fn file_or_collapsed_directory_folds_the_nearest_visible_parent() {
        let directory = tempfile::tempdir().unwrap();
        let parent = directory.path().join("parent");
        let child = parent.join("child");
        let note = child.join("note.txt");
        let root_file = directory.path().join("root.txt");
        std::fs::create_dir_all(&child).unwrap();
        std::fs::write(&note, "note").unwrap();
        std::fs::write(&root_file, "root").unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();

        assert!(explorer.unfold_selected(directory.path()).unwrap());
        explorer.selected = explorer
            .entries
            .iter()
            .position(|entry| entry.path == child);
        assert!(explorer.unfold_selected(directory.path()).unwrap());
        explorer.selected = explorer.entries.iter().position(|entry| entry.path == note);

        assert!(explorer.fold_selected());
        assert_eq!(explorer.selected_entry().unwrap().path, child);
        assert!(!explorer.entries.iter().any(|entry| entry.path == note));
        assert_eq!(
            explorer.selected_entry().unwrap().expansion,
            ExpansionState::Collapsed
        );

        assert!(explorer.fold_selected());
        assert_eq!(explorer.selected_entry().unwrap().path, parent);
        assert!(!explorer.entries.iter().any(|entry| entry.path == child));
        assert_eq!(
            explorer.selected_entry().unwrap().expansion,
            ExpansionState::Collapsed
        );

        assert!(!explorer.fold_selected());
        explorer.selected = explorer
            .entries
            .iter()
            .position(|entry| entry.path == root_file);
        assert!(!explorer.fold_selected());
        assert_eq!(explorer.selected_entry().unwrap().path, root_file);
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
    fn reveal_is_transactional_for_hidden_outside_and_failed_reads() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        let ignored = nested.join("ignored.txt");
        let visible = directory.path().join("visible.txt");
        let gitignore = directory.path().join(".gitignore");
        std::fs::write(&gitignore, "nested/ignored.txt\n").unwrap();
        std::fs::write(&ignored, "ignored").unwrap();
        std::fs::write(&visible, "visible").unwrap();
        let outside = tempfile::tempdir().unwrap();
        let mut explorer = ExplorerState::load(directory.path()).unwrap();
        let original_entries = explorer.entries.clone();
        let original_expansion = explorer.expanded_directories.clone();
        let original_selected = explorer.selected;

        explorer.reveal(directory.path(), &ignored).unwrap();
        explorer
            .reveal(directory.path(), &outside.path().join("outside.txt"))
            .unwrap();
        assert_eq!(explorer.entries, original_entries);
        assert_eq!(explorer.expanded_directories, original_expansion);
        assert_eq!(explorer.selected, original_selected);

        std::fs::remove_file(&ignored).unwrap();
        std::fs::remove_dir(&nested).unwrap();
        std::fs::remove_file(&gitignore).unwrap();
        std::fs::remove_file(&visible).unwrap();
        std::fs::remove_dir(directory.path()).unwrap();
        assert!(explorer.reveal(directory.path(), &visible).is_err());
        assert_eq!(explorer.entries, original_entries);
        assert_eq!(explorer.expanded_directories, original_expansion);
        assert_eq!(explorer.selected, original_selected);
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
