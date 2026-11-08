use std::{path::PathBuf, sync::Arc};

use super::FilePickerRequest;
use crate::data_object::Visibility;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: PathBuf,
    pub display: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileCollectionState {
    Loading,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewState {
    Empty,
    Loading,
    Text(Vec<String>),
    Unavailable(String),
}

/// Picker state retained in the central model while background work runs.
#[derive(Debug)]
pub struct FilePickerState {
    pub visibility: Visibility,
    pub query: String,
    pub results: Vec<FileEntry>,
    pub selected: Option<usize>,
    pub collection: FileCollectionState,
    pub preview: PreviewState,
    files: Arc<Vec<FileEntry>>,
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
            filter_revision: 0,
            preview_revision: 0,
        }
    }
}

impl FilePickerState {
    pub fn open(&mut self, root: &std::path::Path) -> FilePickerRequest {
        self.visibility = Visibility::Displayed;
        self.query.clear();
        self.results.clear();
        self.selected = None;
        self.collection = FileCollectionState::Loading;
        self.preview = PreviewState::Loading;
        self.files = Arc::new(Vec::new());
        self.filter_revision = self.filter_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);
        FilePickerRequest::Scan {
            root: root.to_path_buf(),
            revision: self.filter_revision,
        }
    }

    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
        self.filter_revision = self.filter_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }

    pub fn insert(&mut self, character: char) -> Option<FilePickerRequest> {
        self.query.push(character);
        self.start_filter()
    }

    pub fn backspace(&mut self) -> Option<FilePickerRequest> {
        self.query.pop()?;
        self.start_filter()
    }

    pub fn complete_scan(
        &mut self,
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
                self.start_filter()
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
        revision: u64,
        results: Vec<FileEntry>,
    ) -> Option<FilePickerRequest> {
        if !self.accepts_filter(revision) {
            return None;
        }

        self.results = results;
        self.selected = (!self.results.is_empty()).then_some(0);
        self.start_preview()
    }

    pub fn select_next(&mut self) -> Option<FilePickerRequest> {
        if self.results.is_empty() {
            return None;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some((selected + 1).min(self.results.len() - 1));
        self.start_preview()
    }

    pub fn select_previous(&mut self) -> Option<FilePickerRequest> {
        if self.results.is_empty() {
            return None;
        }
        self.selected = Some(self.selected.unwrap_or(0).saturating_sub(1));
        self.start_preview()
    }

    pub fn complete_preview(&mut self, revision: u64, preview: PreviewState) {
        if self.visibility == Visibility::Displayed && self.preview_revision == revision {
            self.preview = preview;
        }
    }

    pub fn selected_entry(&self) -> Option<&FileEntry> {
        self.results.get(self.selected?)
    }

    fn start_filter(&mut self) -> Option<FilePickerRequest> {
        match self.collection {
            FileCollectionState::Ready => {
                self.filter_revision = self.filter_revision.wrapping_add(1);
                Some(FilePickerRequest::Filter {
                    files: self.files.clone(),
                    query: self.query.clone(),
                    revision: self.filter_revision,
                })
            }
            FileCollectionState::Loading | FileCollectionState::Failed => None,
        }
    }

    fn start_preview(&mut self) -> Option<FilePickerRequest> {
        self.preview_revision = self.preview_revision.wrapping_add(1);
        let Some(entry) = self.selected_entry() else {
            self.preview = PreviewState::Empty;
            return None;
        };
        let path = entry.path.clone();
        self.preview = PreviewState::Loading;
        Some(FilePickerRequest::Preview {
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
    use std::path::Path;

    use super::*;

    fn entry(name: &str) -> FileEntry {
        FileEntry {
            path: PathBuf::from(name),
            display: name.to_string(),
        }
    }

    #[test]
    fn typing_during_scan_filters_with_the_latest_query() {
        let mut state = FilePickerState::default();
        let FilePickerRequest::Scan { revision, .. } = state.open(Path::new("/workspace")) else {
            panic!("opening should scan files");
        };

        assert!(state.insert('m').is_none());
        let Some(FilePickerRequest::Filter {
            revision: filter_revision,
            query,
            ..
        }) = state.complete_scan(revision, Ok(vec![entry("main.rs"), entry("other.txt")]))
        else {
            panic!("scan completion should filter files");
        };
        assert_eq!(query, "m");

        let Some(FilePickerRequest::Preview {
            revision: first_preview,
            ..
        }) = state.complete_filter(filter_revision, vec![entry("main.rs"), entry("mod.rs")])
        else {
            panic!("filter completion should preview the first result");
        };
        let Some(FilePickerRequest::Preview {
            revision: second_preview,
            ..
        }) = state.select_next()
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
            PreviewState::Text(vec!["latest".to_string()]),
        );
        assert_eq!(
            state.preview,
            PreviewState::Text(vec!["latest".to_string()])
        );
        assert_eq!(state.selected_entry().unwrap().display, "mod.rs");
    }

    #[test]
    fn closing_invalidates_pending_completions() {
        let mut state = FilePickerState::default();
        let FilePickerRequest::Scan { revision, .. } = state.open(Path::new("/workspace")) else {
            panic!("opening should scan files");
        };
        state.close();

        assert!(
            state
                .complete_scan(revision, Ok(vec![entry("main.rs")]))
                .is_none()
        );
        assert_eq!(state.visibility, Visibility::Hidden);
    }
}
