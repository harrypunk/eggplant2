use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use editor::{DocumentFactory, TextViewSnapshot};

use super::FilePickerRequest;
use crate::data_object::Visibility;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDepth {
    TopLevel,
    Recursive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCollectionState {
    Loading,
    Filtering,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    Empty,
    Loading,
    Text(TextViewSnapshot),
    Unavailable(String),
}

/// Picker state and its persistent file-entry cache.
#[derive(Debug)]
pub struct FilePickerState {
    pub visibility: Visibility,
    pub query: String,
    pub results: Vec<FileEntry>,
    pub selected: Option<usize>,
    pub collection: FileCollectionState,
    pub preview: PreviewState,
    files: Arc<Vec<FileEntry>>,
    depth: ScanDepth,
    preview_cache: HashMap<PathBuf, TextViewSnapshot>,
    pending_preview_path: Option<PathBuf>,
    filter_revision: u64,
    preview_revision: u64,
}

impl Default for FilePickerState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            query: String::new(),
            results: Vec::new(),
            selected: None,
            collection: FileCollectionState::Loading,
            preview: PreviewState::Empty,
            files: Arc::new(Vec::new()),
            depth: ScanDepth::TopLevel,
            preview_cache: HashMap::new(),
            pending_preview_path: None,
            filter_revision: 0,
            preview_revision: 0,
        }
    }
}

impl FilePickerState {
    /// Seed an immediate top-level cache from data already loaded at startup.
    pub fn seed_top_level(&mut self, mut files: Vec<FileEntry>) {
        files.sort_by(|left, right| left.display.cmp(&right.display));
        self.files = Arc::new(files);
        self.depth = ScanDepth::TopLevel;
        self.collection = FileCollectionState::Ready;
    }

    /// Reuse ready entries on reopen; otherwise start with a top-level scan.
    pub fn open(&mut self, root: &Path, documents: &DocumentFactory) -> Option<FilePickerRequest> {
        self.visibility = Visibility::Displayed;
        self.query.clear();
        self.filter_revision = self.filter_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);

        match self.collection {
            FileCollectionState::Ready | FileCollectionState::Filtering => {
                self.collection = FileCollectionState::Ready;
                self.results = self.files.as_ref().clone();
                self.selected = (!self.results.is_empty()).then_some(0);
                self.start_preview(documents)
            }
            FileCollectionState::Loading | FileCollectionState::Failed => {
                self.results.clear();
                self.selected = None;
                self.preview = PreviewState::Loading;
                self.files = Arc::new(Vec::new());
                self.start_scan(root, ScanDepth::TopLevel)
            }
        }
    }

    /// Hide the picker without discarding its ready file-entry cache.
    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
        self.pending_preview_path = None;
        self.filter_revision = self.filter_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }

    pub fn insert(&mut self, root: &Path, character: char) -> Option<FilePickerRequest> {
        self.query.push(character);
        self.update_results(root)
    }

    pub fn backspace(&mut self, root: &Path) -> Option<FilePickerRequest> {
        self.query.pop()?;
        self.update_results(root)
    }

    pub fn complete_scan(
        &mut self,
        root: &Path,
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    ) -> Option<FilePickerRequest> {
        if !self.accepts_filter(revision) {
            return None;
        }

        match result {
            Ok(files) => {
                self.files = Arc::new(files);
                self.collection = FileCollectionState::Ready;
                self.update_results(root)
            }
            Err(error) => {
                self.collection = FileCollectionState::Failed;
                self.results.clear();
                self.selected = None;
                self.preview = PreviewState::Unavailable(error);
                None
            }
        }
    }

    pub fn complete_filter(
        &mut self,
        documents: &DocumentFactory,
        revision: u64,
        results: Vec<FileEntry>,
    ) -> Option<FilePickerRequest> {
        if !self.accepts_filter(revision) {
            return None;
        }

        self.collection = FileCollectionState::Ready;
        self.results = results;
        self.selected = (!self.results.is_empty()).then_some(0);
        self.start_preview(documents)
    }

    pub fn select_next(&mut self, documents: &DocumentFactory) -> Option<FilePickerRequest> {
        if self.results.is_empty() {
            return None;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some((selected + 1).min(self.results.len() - 1));
        self.start_preview(documents)
    }

    pub fn select_previous(&mut self, documents: &DocumentFactory) -> Option<FilePickerRequest> {
        if self.results.is_empty() {
            return None;
        }
        self.selected = Some(self.selected.unwrap_or(0).saturating_sub(1));
        self.start_preview(documents)
    }

    pub fn complete_preview(&mut self, revision: u64, preview: PreviewState) {
        if self.visibility != Visibility::Displayed || self.preview_revision != revision {
            return;
        }
        if let (Some(path), PreviewState::Text(snapshot)) = (&self.pending_preview_path, &preview) {
            self.preview_cache.insert(path.clone(), snapshot.clone());
        }
        self.pending_preview_path = None;
        self.preview = preview;
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.results.get(self.selected?)
    }

    fn update_results(&mut self, root: &Path) -> Option<FilePickerRequest> {
        match (self.collection, self.depth) {
            (FileCollectionState::Ready | FileCollectionState::Filtering, ScanDepth::TopLevel)
                if !self.query.is_empty() =>
            {
                self.start_scan(root, ScanDepth::Recursive)
            }
            (FileCollectionState::Ready | FileCollectionState::Filtering, _) => self.start_filter(),
            (FileCollectionState::Loading | FileCollectionState::Failed, _) => None,
        }
    }

    fn start_scan(&mut self, root: &Path, depth: ScanDepth) -> Option<FilePickerRequest> {
        self.collection = FileCollectionState::Loading;
        self.depth = depth;
        self.filter_revision = self.filter_revision.wrapping_add(1);
        Some(FilePickerRequest::Scan {
            root: root.to_path_buf(),
            depth,
            revision: self.filter_revision,
        })
    }

    fn start_filter(&mut self) -> Option<FilePickerRequest> {
        self.collection = FileCollectionState::Filtering;
        self.filter_revision = self.filter_revision.wrapping_add(1);
        Some(FilePickerRequest::Filter {
            files: self.files.clone(),
            query: self.query.clone(),
            revision: self.filter_revision,
        })
    }

    fn start_preview(&mut self, documents: &DocumentFactory) -> Option<FilePickerRequest> {
        self.preview_revision = self.preview_revision.wrapping_add(1);
        let Some(entry) = self.selected_entry() else {
            self.preview = PreviewState::Empty;
            self.pending_preview_path = None;
            return None;
        };
        let path = entry.path.clone();
        self.pending_preview_path = None;
        if let Some(snapshot) = self.preview_cache.get(&path) {
            self.preview = PreviewState::Text(snapshot.clone());
            return None;
        }

        match self.preview {
            PreviewState::Text(_) => {}
            _ => self.preview = PreviewState::Loading,
        }
        self.pending_preview_path = Some(path.clone());
        Some(FilePickerRequest::Preview {
            documents: documents.clone(),
            path,
            revision: self.preview_revision,
        })
    }

    fn accepts_filter(&self, revision: u64) -> bool {
        self.visibility == Visibility::Displayed && self.filter_revision == revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(name),
            display: name.to_string(),
        }
    }

    fn text_snapshot(lines: &[&str]) -> TextViewSnapshot {
        TextViewSnapshot {
            first_line: 0,
            line_count: lines.len(),
            lines: lines.iter().map(|line| (*line).to_string()).collect(),
            horizontal_offset: 0,
        }
    }

    #[test]
    fn typing_during_top_level_scan_upgrades_to_one_recursive_scan() {
        let mut state = FilePickerState::default();
        let documents = DocumentFactory::new();
        let Some(FilePickerRequest::Scan {
            depth: ScanDepth::TopLevel,
            revision,
            ..
        }) = state.open(Path::new("/workspace"), &documents)
        else {
            panic!("opening should start with a top-level scan");
        };

        assert!(state.insert(Path::new("/workspace"), 'm').is_none());
        let Some(FilePickerRequest::Scan {
            depth: ScanDepth::Recursive,
            revision,
            ..
        }) = state.complete_scan(
            Path::new("/workspace"),
            revision,
            Ok(vec![entry("main.rs"), entry("other.txt")]),
        )
        else {
            panic!("a non-empty query should upgrade to a recursive scan");
        };
        let Some(FilePickerRequest::Filter {
            revision: filter_revision,
            query,
            ..
        }) = state.complete_scan(
            Path::new("/workspace"),
            revision,
            Ok(vec![entry("main.rs"), entry("other.txt")]),
        )
        else {
            panic!("recursive scan completion should filter files");
        };
        assert_eq!(query, "m");

        let Some(FilePickerRequest::Preview {
            revision: first_preview,
            ..
        }) = state.complete_filter(
            &documents,
            filter_revision,
            vec![entry("main.rs"), entry("mod.rs")],
        )
        else {
            panic!("filter completion should preview the first result");
        };
        let Some(FilePickerRequest::Preview {
            revision: second_preview,
            ..
        }) = state.select_next(&documents)
        else {
            panic!("selection should preview the next result");
        };

        state.complete_preview(
            first_preview,
            PreviewState::Unavailable("stale".to_string()),
        );
        assert_eq!(state.preview, PreviewState::Loading);
        state.complete_preview(
            second_preview,
            PreviewState::Text(text_snapshot(&["latest"])),
        );
        assert_eq!(
            state.preview,
            PreviewState::Text(text_snapshot(&["latest"]))
        );
        assert_eq!(state.selected_entry().unwrap().display, "mod.rs");
    }

    #[test]
    fn reopen_reuses_ready_cache_without_scanning() {
        let mut state = FilePickerState::default();
        let documents = DocumentFactory::new();
        let Some(FilePickerRequest::Scan { revision, .. }) =
            state.open(Path::new("/workspace"), &documents)
        else {
            panic!("opening should scan files");
        };
        let Some(FilePickerRequest::Filter {
            revision: filter_revision,
            ..
        }) = state.complete_scan(
            Path::new("/workspace"),
            revision,
            Ok(vec![entry("main.rs")]),
        )
        else {
            panic!("scan completion should filter files");
        };
        assert!(
            state
                .complete_filter(&documents, filter_revision, vec![entry("main.rs")])
                .is_some()
        );
        state.close();

        let request = state.open(Path::new("/workspace"), &documents);
        assert!(matches!(request, Some(FilePickerRequest::Preview { .. })));
        assert_eq!(state.results, vec![entry("main.rs")]);
        assert_eq!(state.collection, FileCollectionState::Ready);
    }

    #[test]
    fn preview_cache_returns_instantly_and_uncached_loads_keep_previous_text() {
        let documents = DocumentFactory::new();
        let mut state = FilePickerState {
            visibility: Visibility::Displayed,
            results: vec![entry("first.txt"), entry("second.txt")],
            selected: Some(0),
            ..FilePickerState::default()
        };
        let first_snapshot = text_snapshot(&["first"]);
        let second_snapshot = text_snapshot(&["second"]);

        let Some(FilePickerRequest::Preview {
            revision: first_revision,
            ..
        }) = state.start_preview(&documents)
        else {
            panic!("the first preview should load");
        };
        state.complete_preview(first_revision, PreviewState::Text(first_snapshot.clone()));
        let Some(FilePickerRequest::Preview {
            revision: second_revision,
            ..
        }) = state.select_next(&documents)
        else {
            panic!("the second preview should load");
        };
        assert_eq!(state.preview, PreviewState::Text(first_snapshot.clone()));
        state.complete_preview(second_revision, PreviewState::Text(second_snapshot));

        assert!(state.select_previous(&documents).is_none());
        assert_eq!(state.preview, PreviewState::Text(first_snapshot));
    }

    #[test]
    fn closing_invalidates_pending_completions() {
        let mut state = FilePickerState::default();
        let documents = DocumentFactory::new();
        let Some(FilePickerRequest::Scan { revision, .. }) =
            state.open(Path::new("/workspace"), &documents)
        else {
            panic!("opening should scan files");
        };
        state.close();

        assert!(
            state
                .complete_scan(
                    Path::new("/workspace"),
                    revision,
                    Ok(vec![entry("main.rs")])
                )
                .is_none()
        );
        assert_eq!(state.visibility, Visibility::Hidden);
    }
}
