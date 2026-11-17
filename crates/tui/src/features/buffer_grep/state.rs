use std::sync::Arc;

use editor::{BufferId, BufferTextSnapshot, EditorSession, TextViewSnapshot};

use super::BufferGrepRequest;
use crate::data_object::Visibility;

pub(crate) const PREVIEW_HEIGHT: usize = 34;

pub type GrepMatch = crate::search::MatchLine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchStatus {
    Idle,
    Searching,
    Ready,
    Failed(String),
}

#[derive(Debug)]
pub struct BufferGrepState {
    pub visibility: Visibility,
    pub query: String,
    pub results: Vec<GrepMatch>,
    pub selected: Option<usize>,
    pub status: SearchStatus,
    preview: Option<TextViewSnapshot>,
    source: Option<Arc<BufferTextSnapshot>>,
    query_revision: u64,
}

impl Default for BufferGrepState {
    fn default() -> Self {
        Self {
            visibility: Visibility::Hidden,
            query: String::new(),
            results: Vec::new(),
            selected: None,
            status: SearchStatus::Idle,
            preview: None,
            source: None,
            query_revision: 0,
        }
    }
}

impl BufferGrepState {
    pub fn open(&mut self, source: BufferTextSnapshot) {
        self.visibility = Visibility::Displayed;
        self.query.clear();
        self.results.clear();
        self.selected = None;
        self.status = SearchStatus::Idle;
        self.preview = None;
        self.source = Some(Arc::new(source));
        self.query_revision = self.query_revision.wrapping_add(1);
    }

    pub fn close(&mut self) {
        self.visibility = Visibility::Hidden;
        self.source = None;
        self.query_revision = self.query_revision.wrapping_add(1);
    }

    pub fn source_identity(&self) -> Option<(BufferId, i32)> {
        let source = self.source.as_ref()?;
        Some((source.buffer_id, source.revision))
    }

    pub fn insert(&mut self, character: char) -> Option<BufferGrepRequest> {
        self.query.push(character);
        self.start_search()
    }

    pub fn backspace(&mut self) -> Option<BufferGrepRequest> {
        self.query.pop()?;
        self.start_search()
    }

    pub fn complete(
        &mut self,
        query_revision: u64,
        source: (BufferId, i32),
        active_revision: Option<(BufferId, i32)>,
        result: Result<Vec<GrepMatch>, String>,
    ) -> bool {
        if self.visibility != Visibility::Displayed
            || self.query_revision != query_revision
            || self.source_identity() != Some(source)
            || active_revision != Some(source)
        {
            return false;
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
        true
    }

    pub fn select_next(&mut self) {
        if self.results.is_empty() {
            return;
        }
        let selected = self.selected.unwrap_or(0);
        self.selected = Some((selected + 1) % self.results.len());
    }

    pub fn select_previous(&mut self) {
        if self.results.is_empty() {
            return;
        }
        self.selected =
            Some((self.selected.unwrap_or(0) + self.results.len() - 1) % self.results.len());
    }

    pub fn selected_match(&self) -> Option<&GrepMatch> {
        self.results.get(self.selected?)
    }

    pub fn preview(&self) -> Option<&TextViewSnapshot> {
        self.preview.as_ref()
    }

    pub fn refresh_preview(&mut self, editor: &EditorSession) {
        let selected_line = self.selected_match().map(|result| result.line);
        self.preview = match self.source_identity() == editor.active_revision() {
            true => {
                selected_line.and_then(|line| editor.active_preview_at_line(line, PREVIEW_HEIGHT))
            }
            false => None,
        };
    }

    fn start_search(&mut self) -> Option<BufferGrepRequest> {
        self.query_revision = self.query_revision.wrapping_add(1);
        self.results.clear();
        self.selected = None;
        self.preview = None;
        if self.query.is_empty() {
            self.status = SearchStatus::Idle;
            return None;
        }

        self.status = SearchStatus::Searching;
        let source = self.source.as_ref()?.clone();
        Some(BufferGrepRequest::new(
            source,
            self.query.clone(),
            self.query_revision,
        ))
    }
}

#[cfg(test)]
mod tests {
    use editor::{DocumentFactory, EditorSession};

    use super::*;

    fn buffer_id() -> BufferId {
        EditorSession::new().insert_document(DocumentFactory::new().create(""))
    }

    fn source(revision: i32) -> BufferTextSnapshot {
        BufferTextSnapshot {
            buffer_id: buffer_id(),
            revision,
            text: "hello\nyellow\nworld".to_string(),
        }
    }

    #[test]
    fn stale_query_and_buffer_revisions_are_rejected() {
        let mut state = BufferGrepState::default();
        state.open(source(4));
        state.insert('e');
        let stale_query = state.query_revision;
        state.insert('l');
        let current_query = state.query_revision;
        let identity = state.source_identity().unwrap();
        let result = || {
            Ok(vec![GrepMatch {
                line: 0,
                text: "hello".to_string(),
                match_ranges: vec![std::ops::Range { start: 1, end: 4 }],
            }])
        };

        assert!(!state.complete(stale_query, identity, Some(identity), result()));
        assert!(!state.complete(
            current_query,
            identity,
            Some((identity.0, identity.1 + 1)),
            result()
        ));
        assert!(state.complete(current_query, identity, Some(identity), result()));
        assert_eq!(state.results.len(), 1);
    }

    #[test]
    fn preview_uses_explicit_editor_and_clears_for_a_changed_buffer() {
        let mut editor = EditorSession::new();
        editor.insert_document(DocumentFactory::new().create("hello\nworld"));
        editor.resize_active_view(80, 10);
        let mut state = BufferGrepState::default();
        state.open(editor.active_text_snapshot().unwrap());
        state.results = vec![GrepMatch {
            line: 0,
            text: "hello".to_string(),
            match_ranges: Vec::new(),
        }];
        state.selected = Some(0);

        state.refresh_preview(&editor);
        assert_eq!(state.preview().unwrap().lines[0].text, "hello");

        editor.insert_document(DocumentFactory::new().create("other"));
        state.refresh_preview(&editor);
        assert!(state.preview().is_none());
    }

    #[test]
    fn selection_wraps_and_empty_results_are_noops() {
        let result = |text: &str| GrepMatch {
            line: 0,
            text: text.to_string(),
            match_ranges: Vec::new(),
        };
        let mut state = BufferGrepState {
            results: vec![result("first"), result("second"), result("third")],
            selected: Some(0),
            ..BufferGrepState::default()
        };

        state.select_previous();
        assert_eq!(state.selected_match().unwrap().text, "third");
        state.select_next();
        assert_eq!(state.selected_match().unwrap().text, "first");

        let mut empty = BufferGrepState::default();
        empty.select_next();
        empty.select_previous();
        assert_eq!(empty.selected, None);
    }
}
