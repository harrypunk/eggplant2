use std::{path::Path, sync::Arc};

use unicode_general_category::{GeneralCategory, get_general_category};

use crate::{
    component::{list::ListState, text_input::TextInput},
    data_object::{Message, Model},
    file_search::{FileEntry, ScanDepth, filter_files, scan_files},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

const MAX_FILE_REFERENCES: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileReferenceStatus {
    Loading,
    Filtering,
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy)]
pub struct FileReferenceView<'a> {
    pub entries: &'a [FileEntry],
    pub selected: Option<usize>,
    pub status: FileReferenceStatus,
}

#[derive(Debug, Default)]
pub struct FileReferenceState {
    revision: u64,
    session: Option<ReferenceSession>,
    dismissed: Option<ReferenceToken>,
}

#[derive(Debug)]
struct ReferenceSession {
    token: ReferenceToken,
    current: Option<FileEntry>,
    files: Arc<Vec<FileEntry>>,
    results: ListState<FileEntry>,
    status: FileReferenceStatus,
}

impl ReferenceSession {
    fn new(token: ReferenceToken, current: Option<FileEntry>) -> Self {
        Self {
            token,
            current,
            files: Arc::new(Vec::new()),
            results: ListState::default(),
            status: FileReferenceStatus::Loading,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceToken {
    start: usize,
    end: usize,
    query: String,
    quoted: bool,
}

impl ReferenceToken {
    fn at_cursor(input: &TextInput) -> Option<Self> {
        let cursor = input.cursor_byte();
        let value = input.value();
        let mut candidate: Option<Self> = None;

        for (index, character) in value[..cursor].char_indices() {
            if let Some(token) = candidate.as_mut().filter(|token| token.quoted) {
                if character == '"' {
                    candidate = None;
                } else {
                    token.query.push(character);
                }
                continue;
            }

            if character == '@' && valid_trigger_boundary(value, index) {
                candidate = Some(Self {
                    start: index,
                    end: cursor,
                    query: String::new(),
                    quoted: false,
                });
                continue;
            }

            let Some(token) = candidate.as_mut() else {
                continue;
            };
            if index == token.start + 1 && character == '"' {
                token.quoted = true;
            } else if is_unquoted_delimiter(character) {
                candidate = None;
            } else {
                token.query.push(character);
            }
        }

        return candidate;

        fn valid_trigger_boundary(value: &str, at: usize) -> bool {
            let mut prefix = &value[..at];
            loop {
                let Some((index, previous)) = prefix.char_indices().last() else {
                    return true;
                };
                if matches!(previous, '(' | '[' | '{' | '<' | '`') {
                    prefix = &prefix[..index];
                    continue;
                }
                return previous.is_whitespace() || is_non_ascii_punctuation(previous);
            }
        }
    }
}

fn is_non_ascii_punctuation(character: char) -> bool {
    !character.is_ascii()
        && matches!(
            get_general_category(character),
            GeneralCategory::ConnectorPunctuation
                | GeneralCategory::DashPunctuation
                | GeneralCategory::OpenPunctuation
                | GeneralCategory::ClosePunctuation
                | GeneralCategory::InitialPunctuation
                | GeneralCategory::FinalPunctuation
                | GeneralCategory::OtherPunctuation
        )
}

fn is_unquoted_delimiter(character: char) -> bool {
    character.is_whitespace()
        || matches!(character, '"' | '\'')
        || is_non_ascii_punctuation(character)
}

fn requires_quotes(path: &str) -> bool {
    path.chars()
        .any(|character| character.is_whitespace() || is_non_ascii_punctuation(character))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReferenceAcceptance {
    pub range: std::ops::Range<usize>,
    pub replacement: String,
}

impl FileReferenceState {
    pub fn update(
        &mut self,
        input: &TextInput,
        root: &Path,
        current: Option<FileEntry>,
    ) -> Option<FileReferenceRequest> {
        let Some(token) = ReferenceToken::at_cursor(input) else {
            self.clear();
            return None;
        };
        if self.dismissed.as_ref().is_some_and(|dismissed| {
            dismissed.start == token.start
                && dismissed.query == token.query
                && dismissed.quoted == token.quoted
        }) {
            return None;
        }

        if self
            .session
            .as_ref()
            .is_none_or(|session| session.token.start != token.start)
        {
            self.dismissed = None;
            self.session = Some(ReferenceSession::new(token, current));
            return self.start_scan(root);
        }

        let (query_changed, current_changed, status) = {
            let session = self.session.as_mut().unwrap();
            let query_changed = session.token.query != token.query;
            let current_changed = session.current != current;
            session.token = token;
            session.current = current;
            (query_changed, current_changed, session.status)
        };
        if !query_changed && !current_changed {
            return None;
        }
        match status {
            FileReferenceStatus::Loading => None,
            FileReferenceStatus::Failed => self.start_scan(root),
            FileReferenceStatus::Filtering | FileReferenceStatus::Ready => self.start_filter(),
        }
    }

    pub fn view(&self) -> Option<FileReferenceView<'_>> {
        self.session.as_ref().map(|session| FileReferenceView {
            entries: session.results.items(),
            selected: session.results.selected(),
            status: session.status,
        })
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn select_next(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.results.select_next();
        }
    }

    pub fn select_previous(&mut self) {
        if let Some(session) = self.session.as_mut() {
            session.results.select_previous();
        }
    }

    pub fn acceptance(&self, input: &TextInput) -> Option<FileReferenceAcceptance> {
        let session = self.session.as_ref()?;
        if session.status != FileReferenceStatus::Ready {
            return None;
        }
        let token = &session.token;
        let entry = session.results.selected_item()?;
        let quoted = token.quoted || requires_quotes(&entry.display);
        let reference = if quoted {
            format!("@\"{}\"", entry.display)
        } else {
            format!("@{}", entry.display)
        };
        let end =
            token.end + usize::from(token.quoted && input.value()[token.end..].starts_with('"'));
        Some(FileReferenceAcceptance {
            range: token.start..end,
            replacement: format!("{reference} "),
        })
    }

    pub fn dismiss(&mut self) {
        if let Some(session) = self.session.take() {
            self.dismissed = Some(session.token);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    pub fn clear(&mut self) {
        if self.session.is_some() || self.dismissed.is_some() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.session = None;
        self.dismissed = None;
    }

    pub fn complete_scan(
        &mut self,
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    ) -> Option<FileReferenceRequest> {
        if !self.accepts(revision) {
            return None;
        }
        match result {
            Ok(files) => {
                self.session.as_mut().unwrap().files = Arc::new(files);
                self.start_filter()
            }
            Err(_) => {
                let session = self.session.as_mut().unwrap();
                session.status = FileReferenceStatus::Failed;
                session.results = ListState::default();
                None
            }
        }
    }

    pub fn complete_filter(&mut self, revision: u64, results: Vec<FileEntry>) {
        if !self.accepts(revision) {
            return;
        }
        let session = self.session.as_mut().unwrap();
        session.status = FileReferenceStatus::Ready;
        session.results = ListState::new(results);
        session.results.select_current();
    }

    fn accepts(&self, revision: u64) -> bool {
        self.session.is_some() && self.revision == revision
    }

    fn start_scan(&mut self, root: &Path) -> Option<FileReferenceRequest> {
        self.revision = self.revision.wrapping_add(1);
        let session = self.session.as_mut()?;
        session.status = FileReferenceStatus::Loading;
        session.results = ListState::default();
        Some(FileReferenceRequest::Scan {
            root: root.to_path_buf(),
            revision: self.revision,
        })
    }

    fn start_filter(&mut self) -> Option<FileReferenceRequest> {
        self.revision = self.revision.wrapping_add(1);
        let session = self.session.as_mut()?;
        session.status = FileReferenceStatus::Filtering;
        session.results = ListState::default();
        Some(FileReferenceRequest::Filter {
            files: session.files.clone(),
            query: session.token.query.clone(),
            current: session.current.clone(),
            revision: self.revision,
        })
    }
}

pub enum FileReferenceRequest {
    Scan {
        root: std::path::PathBuf,
        revision: u64,
    },
    Filter {
        files: Arc<Vec<FileEntry>>,
        query: String,
        current: Option<FileEntry>,
        revision: u64,
    },
}

impl FileReferenceRequest {
    pub fn perform(self) -> Message {
        let completion = match self {
            Self::Scan { root, revision } => FileReferenceCompletion::Scan {
                revision,
                result: scan_files(&root, ScanDepth::Recursive),
            },
            Self::Filter {
                files,
                query,
                current,
                revision,
            } => FileReferenceCompletion::Filter {
                revision,
                results: rank_references(&files, &query, current.as_ref()),
            },
        };
        Message::FeatureCompletion(FeatureCompletion::new(completion))
    }
}

impl FeatureEffect for FileReferenceRequest {
    fn perform(self: Box<Self>) -> Message {
        FileReferenceRequest::perform(*self)
    }
}

enum FileReferenceCompletion {
    Scan {
        revision: u64,
        result: Result<Vec<FileEntry>, String>,
    },
    Filter {
        revision: u64,
        results: Vec<FileEntry>,
    },
}

impl Completion for FileReferenceCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let request = match *self {
            Self::Scan { revision, result } => {
                model.agent.complete_file_reference_scan(revision, result)
            }
            Self::Filter { revision, results } => {
                model
                    .agent
                    .complete_file_reference_filter(revision, results);
                None
            }
        };
        request.map(Effect::feature)
    }
}

/// Apply Agent-specific current-buffer priority after shared fuzzy matching.
fn rank_references(
    files: &[FileEntry],
    query: &str,
    current: Option<&FileEntry>,
) -> Vec<FileEntry> {
    let mut results = filter_files(files, query);
    if let Some(current) = current {
        if let Some(position) = results.iter().position(|entry| entry.path == current.path) {
            let current = results.remove(position);
            results.insert(0, current);
        } else if !filter_files(std::slice::from_ref(current), query).is_empty() {
            results.insert(0, current.clone());
        }
    }
    results.truncate(MAX_FILE_REFERENCES);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FileEntry {
        FileEntry {
            path: Path::new("/workspace").join(path),
            display: path.to_string(),
        }
    }

    fn ready_state(input: &TextInput, current: Option<FileEntry>) -> FileReferenceState {
        let mut state = FileReferenceState::default();
        let Some(FileReferenceRequest::Scan { revision, .. }) =
            state.update(input, Path::new("/workspace"), current.clone())
        else {
            panic!("@ should start a workspace scan");
        };
        let files = vec![entry("alpha.rs"), entry("src/main.rs"), entry("z.rs")];
        let Some(FileReferenceRequest::Filter {
            revision,
            files,
            query,
            current,
            ..
        }) = state.complete_scan(revision, Ok(files))
        else {
            panic!("scan completion should start filtering");
        };
        let results = rank_references(&files, &query, current.as_ref());
        state.complete_filter(revision, results);
        state
    }

    #[test]
    fn token_requires_a_boundary_and_supports_quoted_paths() {
        for (text, query) in [
            ("@src", "src"),
            ("see (@src", "src"),
            ("see ((@src", "src"),
            ("see `(@src", "src"),
            ("查看，@src", "src"),
            ("“@src", "src"),
            ("—@src", "src"),
            ("explain @src", "src"),
        ] {
            let input = TextInput::new(text);
            assert_eq!(
                ReferenceToken::at_cursor(&input).map(|token| token.query),
                Some(query.to_string()),
                "{text}"
            );
        }
        for text in [
            "user@example.com",
            "查看@src",
            "foo(@src",
            "Ａ@src",
            "々@src",
            "🙂@src",
            "plain",
        ] {
            assert!(
                ReferenceToken::at_cursor(&TextInput::new(text)).is_none(),
                "{text}"
            );
        }
        let quoted = ReferenceToken::at_cursor(&TextInput::new("read @\"my file")).unwrap();
        assert_eq!(quoted.query, "my file");
        assert!(quoted.quoted);
        assert!(requires_quotes("a—b.txt"));
        assert!(requires_quotes("a“b.txt"));
        assert!(!requires_quotes("Ａ々🙂.rs"));
    }

    #[test]
    fn shared_matching_is_composed_with_current_buffer_priority() {
        let current = entry("src/main.rs");
        let files = vec![entry("alpha.rs"), entry("main.rs"), current.clone()];
        let ranked = rank_references(&files, "", Some(&current));
        assert_eq!(ranked[0], current);
        assert_eq!(ranked.len(), 3);

        let unsaved = entry("unsaved.rs");
        let ranked = rank_references(&files, "unsaved", Some(&unsaved));
        assert_eq!(ranked, vec![unsaved]);
        assert!(rank_references(&files, "alpha", Some(&current))[0].display == "alpha.rs");
    }

    #[test]
    fn selection_wraps_and_acceptance_preserves_the_suffix() {
        let input = TextInput::new("@");
        let mut state = ready_state(&input, Some(entry("src/main.rs")));
        assert_eq!(state.view().unwrap().selected, Some(0));
        state.select_previous();
        assert_eq!(state.view().unwrap().selected, Some(2));
        state.select_next();
        assert_eq!(state.view().unwrap().selected, Some(0));

        let mut input = TextInput::new("read @ before");
        for _ in 0.." before".chars().count() {
            input.handle_event(crate::component::text_input::TextInputEvent::Cursor(
                crate::component::text_input::TextCursorMovement::Left,
            ));
        }
        let state = ready_state(&input, Some(entry("src/main.rs")));
        let acceptance = state.acceptance(&input).unwrap();
        assert!(input.replace_range(acceptance.range, &acceptance.replacement));
        assert_eq!(input.value(), "read @src/main.rs  before");
    }

    #[test]
    fn pending_and_empty_filters_never_accept_stale_candidates() {
        let input = TextInput::new("@");
        let mut state = ready_state(&input, None);
        assert!(state.acceptance(&input).is_some());
        let stale_revision = state.revision;

        let edited = TextInput::new("@q");
        let Some(FileReferenceRequest::Filter { revision, .. }) =
            state.update(&edited, Path::new("/workspace"), None)
        else {
            panic!("editing the query should start a new filter");
        };
        let view = state.view().unwrap();
        assert_eq!(view.status, FileReferenceStatus::Filtering);
        assert!(view.entries.is_empty());
        assert!(state.acceptance(&edited).is_none());

        state.complete_filter(stale_revision, vec![entry("stale.rs")]);
        assert!(state.acceptance(&edited).is_none());
        state.complete_filter(revision, Vec::new());
        assert_eq!(state.view().unwrap().status, FileReferenceStatus::Ready);
        assert!(state.acceptance(&edited).is_none());
    }

    #[test]
    fn escape_preserves_text_and_rejects_stale_completions() {
        let input = TextInput::new("@");
        let mut state = FileReferenceState::default();
        let Some(FileReferenceRequest::Scan { revision, .. }) =
            state.update(&input, Path::new("/workspace"), None)
        else {
            panic!("expected scan");
        };
        state.dismiss();
        assert!(!state.is_active());
        assert!(
            state
                .complete_scan(revision, Ok(vec![entry("main.rs")]))
                .is_none()
        );
        assert!(
            state
                .update(&input, Path::new("/workspace"), None)
                .is_none()
        );
        assert_eq!(input.value(), "@");

        let edited = TextInput::new("@m");
        assert!(
            state
                .update(&edited, Path::new("/workspace"), None)
                .is_some()
        );
    }

    #[test]
    fn changing_quote_style_reopens_a_dismissed_empty_token() {
        let quoted = TextInput::new("@\"");
        let mut state = FileReferenceState::default();
        assert!(
            state
                .update(&quoted, Path::new("/workspace"), None)
                .is_some()
        );
        state.dismiss();

        let unquoted = TextInput::new("@");
        assert!(
            state
                .update(&unquoted, Path::new("/workspace"), None)
                .is_some()
        );
    }

    #[test]
    fn paths_with_spaces_are_inserted_as_quoted_references() {
        let input = TextInput::new("@");
        let mut state = ready_state(&input, None);
        let revision = state.revision;
        state.complete_filter(revision, vec![entry("my folder/a file.txt")]);
        let acceptance = state.acceptance(&input).unwrap();
        assert_eq!(acceptance.replacement, "@\"my folder/a file.txt\" ");
    }
}
