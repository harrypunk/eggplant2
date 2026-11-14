use std::{ops::Range, path::PathBuf};

use editor::{DocumentFactory, TextViewSnapshot};

use super::ProjectGrepRequest;
use crate::data_object::Visibility;

pub const PREVIEW_HEIGHT: usize = 34;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectGrepResult {
    pub path: PathBuf,
    pub display: String,
    pub line: usize,
    pub text: String,
    pub match_ranges: Vec<Range<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchStatus {
    Idle,
    Searching,
    Ready,
    Failed(String),
}

#[derive(Debug)]
pub struct ProjectGrepState {
    pub visibility: Visibility,
    pub query: String,
    pub results: Vec<ProjectGrepResult>,
    pub selected: Option<usize>,
    pub status: SearchStatus,
    pub preview: Option<TextViewSnapshot>,
    pub preview_loading: bool,
    pub preview_error: Option<String>,
    root: Option<PathBuf>,
    documents: DocumentFactory,
    query_revision: u64,
    preview_revision: u64,
}

impl Default for ProjectGrepState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            query: String::new(),
            results: Vec::new(),
            selected: None,
            status: SearchStatus::Idle,
            preview: None,
            preview_loading: false,
            preview_error: None,
            root: None,
            documents: DocumentFactory::new(),
            query_revision: 0,
            preview_revision: 0,
        }
    }
}

impl ProjectGrepState {
    pub fn open(&mut self, root: PathBuf) {
        self.visibility = Visibility::Displayed;
        self.root = Some(root);
        self.query.clear();
        self.results.clear();
        self.selected = None;
        self.status = SearchStatus::Idle;
        self.clear_preview();
        self.query_revision = self.query_revision.wrapping_add(1);
    }

    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
        self.query_revision = self.query_revision.wrapping_add(1);
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }

    pub fn documents(&self) -> &DocumentFactory {
        &self.documents
    }

    pub fn insert(&mut self, character: char) -> Option<ProjectGrepRequest> {
        self.query.push(character);
        self.start_search()
    }

    pub fn backspace(&mut self) -> Option<ProjectGrepRequest> {
        self.query.pop()?;
        self.start_search()
    }

    pub fn complete_search(
        &mut self,
        query_revision: u64,
        result: Result<Vec<ProjectGrepResult>, String>,
    ) -> Option<ProjectGrepRequest> {
        if self.visibility != Visibility::Displayed || self.query_revision != query_revision {
            return None;
        }
        match result {
            Ok(results) => {
                self.results = results;
                self.selected = (!self.results.is_empty()).then_some(0);
                self.status = SearchStatus::Ready;
            }
            Err(error) => {
                self.results.clear();
                self.selected = None;
                self.status = SearchStatus::Failed(error);
            }
        }
        self.start_preview()
    }

    pub fn select_next(&mut self) -> Option<ProjectGrepRequest> {
        if self.results.is_empty() {
            return None;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some((selected + 1).min(self.results.len() - 1));
        self.start_preview()
    }

    pub fn select_previous(&mut self) -> Option<ProjectGrepRequest> {
        if self.results.is_empty() {
            return None;
        }
        self.selected = Some(self.selected.unwrap_or(0).saturating_sub(1));
        self.start_preview()
    }

    pub fn complete_preview(
        &mut self,
        query_revision: u64,
        preview_revision: u64,
        result: Result<TextViewSnapshot, String>,
    ) -> bool {
        if self.visibility != Visibility::Displayed
            || self.query_revision != query_revision
            || self.preview_revision != preview_revision
        {
            return false;
        }
        self.preview_loading = false;
        match result {
            Ok(snapshot) => {
                self.preview = Some(snapshot);
                self.preview_error = None;
            }
            Err(error) => {
                self.preview = None;
                self.preview_error = Some(error);
            }
        }
        true
    }

    pub fn selected_result(&self) -> Option<&ProjectGrepResult> {
        self.results.get(self.selected?)
    }

    fn start_search(&mut self) -> Option<ProjectGrepRequest> {
        self.query_revision = self.query_revision.wrapping_add(1);
        self.results.clear();
        self.selected = None;
        self.clear_preview();
        if self.query.is_empty() {
            self.status = SearchStatus::Idle;
            return None;
        }
        self.status = SearchStatus::Searching;
        let root = self.root.as_ref()?.clone();
        Some(ProjectGrepRequest::Search {
            root,
            query: self.query.clone(),
            query_revision: self.query_revision,
        })
    }

    fn start_preview(&mut self) -> Option<ProjectGrepRequest> {
        self.preview_revision = self.preview_revision.wrapping_add(1);
        self.clear_preview();
        let (path, line) = {
            let selected = self.selected_result()?;
            (selected.path.clone(), selected.line)
        };
        self.preview_loading = true;
        Some(ProjectGrepRequest::Preview {
            documents: self.documents.clone(),
            path,
            line,
            query_revision: self.query_revision,
            preview_revision: self.preview_revision,
        })
    }

    fn clear_preview(&mut self) {
        self.preview = None;
        self.preview_loading = false;
        self.preview_error = None;
        self.preview_revision = self.preview_revision.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_query_and_preview_completions_are_rejected() {
        let mut state = ProjectGrepState::default();
        state.open(PathBuf::from("/workspace"));
        state.insert('e');
        let stale_query = state.query_revision;
        state.insert('l');
        let current_query = state.query_revision;
        let result = || {
            Ok(vec![ProjectGrepResult {
                path: PathBuf::from("main.rs"),
                display: "main.rs".to_string(),
                line: 0,
                text: "hello".to_string(),
                match_ranges: vec![std::ops::Range { start: 1, end: 4 }],
            }])
        };

        assert!(state.complete_search(stale_query, result()).is_none());
        assert!(state.complete_search(current_query, result()).is_some());
        let stale_preview = state.preview_revision;
        let request = state.start_preview();
        assert!(request.is_some());
        assert!(!state.complete_preview(
            current_query,
            stale_preview,
            Ok(TextViewSnapshot::plain(Vec::new()))
        ));
    }
}
