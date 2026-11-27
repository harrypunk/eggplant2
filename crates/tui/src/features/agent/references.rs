use std::{cmp::Reverse, path::Path, sync::Arc};

use unicode_general_category::{GeneralCategory, get_general_category};

use crate::{
    component::{list::ListState, text_input::TextInput},
    data_object::{Message, Model},
    file_search::{FileEntry, ScanDepth, filter_files, scan_files},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

const MAX_FILE_REFERENCES: usize = 20;
const OPENING_WRAPPERS: &[char] = &['(', '[', '{', '<', '`'];
const QUOTE_DELIMITERS: &[char] = &['"', '\''];
const PUNCTUATION_CATEGORIES: &[GeneralCategory] = &[
    GeneralCategory::ConnectorPunctuation,
    GeneralCategory::DashPunctuation,
    GeneralCategory::OpenPunctuation,
    GeneralCategory::ClosePunctuation,
    GeneralCategory::InitialPunctuation,
    GeneralCategory::FinalPunctuation,
    GeneralCategory::OtherPunctuation,
];

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
    search: ReferenceSearch,
}

/// Search phases own only the data that is valid in that phase.
#[derive(Debug)]
enum ReferenceSearch {
    CurrentFile(ListState<FileEntry>),
    Loading,
    Filtering,
    Ready(ListState<FileEntry>),
    Failed,
}

impl ReferenceSearch {
    fn current_file(current: Option<&FileEntry>) -> Self {
        let mut results = ListState::new(current.cloned().into_iter().collect());
        results.select_current();
        Self::CurrentFile(results)
    }

    fn current_results(&self) -> Option<&ListState<FileEntry>> {
        match self {
            Self::CurrentFile(results) => Some(results),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceTransition {
    Unchanged,
    AwaitScan,
    ShowCurrent,
    Scan,
    Filter,
}

impl ReferenceSession {
    fn new(token: ReferenceToken, current: Option<FileEntry>) -> Self {
        let search = if token.query.is_empty() {
            ReferenceSearch::current_file(current.as_ref())
        } else {
            ReferenceSearch::Loading
        };
        Self {
            token,
            current,
            files: Arc::new(Vec::new()),
            search,
        }
    }

    fn advance(
        &mut self,
        token: ReferenceToken,
        current: Option<FileEntry>,
    ) -> ReferenceTransition {
        let changed = self.token.query != token.query || self.current != current;
        let query_empty = token.query.is_empty();
        self.token = token;
        self.current = current;

        if query_empty {
            let showing_current = self.search.current_results().is_some();
            if changed || !showing_current {
                self.search = ReferenceSearch::current_file(self.current.as_ref());
            }
            return if showing_current {
                ReferenceTransition::Unchanged
            } else {
                ReferenceTransition::ShowCurrent
            };
        }

        match (changed, &self.search) {
            (false, _) => ReferenceTransition::Unchanged,
            (true, ReferenceSearch::Loading) => ReferenceTransition::AwaitScan,
            (true, ReferenceSearch::Failed | ReferenceSearch::CurrentFile(_)) => {
                ReferenceTransition::Scan
            }
            (true, ReferenceSearch::Filtering | ReferenceSearch::Ready(_)) => {
                ReferenceTransition::Filter
            }
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
        let prefix = &value[..cursor];
        prefix
            .char_indices()
            .filter(|(index, character)| *character == '@' && valid_trigger_boundary(value, *index))
            .find_map(|(start, _)| Self::from_fragment(start, cursor, &prefix[start + 1..]))
    }

    /// Validate the complete fragment after a trigger without maintaining lexer state.
    fn from_fragment(start: usize, end: usize, fragment: &str) -> Option<Self> {
        match fragment.strip_prefix('"') {
            Some(query) => (!query.contains('"')).then(|| Self {
                start,
                end,
                query: query.to_string(),
                quoted: true,
            }),
            None => (!fragment.chars().any(is_unquoted_delimiter)).then(|| Self {
                start,
                end,
                query: fragment.to_string(),
                quoted: false,
            }),
        }
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.start == other.start && self.query == other.query && self.quoted == other.quoted
    }
}

fn valid_trigger_boundary(value: &str, at: usize) -> bool {
    let boundary = value[..at].trim_end_matches(|character| OPENING_WRAPPERS.contains(&character));
    boundary
        .chars()
        .next_back()
        .is_none_or(|character| character.is_whitespace() || is_non_ascii_punctuation(character))
}

fn is_non_ascii_punctuation(character: char) -> bool {
    !character.is_ascii() && PUNCTUATION_CATEGORIES.contains(&get_general_category(character))
}

fn is_unquoted_delimiter(character: char) -> bool {
    character.is_whitespace()
        || QUOTE_DELIMITERS.contains(&character)
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
        if self
            .dismissed
            .as_ref()
            .is_some_and(|dismissed| dismissed.same_identity(&token))
        {
            return None;
        }

        if self
            .session
            .as_ref()
            .is_none_or(|session| session.token.start != token.start)
        {
            return self.begin_session(token, current, root);
        }

        let transition = self.session.as_mut().unwrap().advance(token, current);
        match transition {
            ReferenceTransition::Unchanged | ReferenceTransition::AwaitScan => None,
            ReferenceTransition::ShowCurrent => {
                self.revision = self.revision.wrapping_add(1);
                None
            }
            ReferenceTransition::Scan => self.start_scan(root),
            ReferenceTransition::Filter => self.start_filter(),
        }
    }

    pub fn view(&self) -> Option<FileReferenceView<'_>> {
        self.session.as_ref().map(|session| {
            let (status, results) = match &session.search {
                ReferenceSearch::CurrentFile(results) => {
                    (FileReferenceStatus::Ready, Some(results))
                }
                ReferenceSearch::Loading => (FileReferenceStatus::Loading, None),
                ReferenceSearch::Filtering => (FileReferenceStatus::Filtering, None),
                ReferenceSearch::Ready(results) => (FileReferenceStatus::Ready, Some(results)),
                ReferenceSearch::Failed => (FileReferenceStatus::Failed, None),
            };
            FileReferenceView {
                entries: results.map_or(&[], |results| results.items()),
                selected: results.and_then(|results| results.selected()),
                status,
            }
        })
    }

    pub fn is_active(&self) -> bool {
        self.session.is_some()
    }

    pub fn select_next(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let ReferenceSearch::Ready(results) | ReferenceSearch::CurrentFile(results) =
                &mut session.search
        {
            results.select_next();
        }
    }

    pub fn select_previous(&mut self) {
        if let Some(session) = self.session.as_mut()
            && let ReferenceSearch::Ready(results) | ReferenceSearch::CurrentFile(results) =
                &mut session.search
        {
            results.select_previous();
        }
    }

    pub fn acceptance(&self, input: &TextInput) -> Option<FileReferenceAcceptance> {
        let session = self.session.as_ref()?;
        let results = match &session.search {
            ReferenceSearch::CurrentFile(results) | ReferenceSearch::Ready(results) => results,
            _ => return None,
        };
        let token = &session.token;
        let entry = results.selected_item()?;
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
                self.session.as_mut().unwrap().search = ReferenceSearch::Failed;
                None
            }
        }
    }

    pub fn complete_filter(&mut self, revision: u64, results: Vec<FileEntry>) {
        if !self.accepts(revision) {
            return;
        }
        let session = self.session.as_mut().unwrap();
        let mut results = ListState::new(results);
        results.select_current();
        session.search = ReferenceSearch::Ready(results);
    }

    fn begin_session(
        &mut self,
        token: ReferenceToken,
        current: Option<FileEntry>,
        root: &Path,
    ) -> Option<FileReferenceRequest> {
        let show_current = token.query.is_empty();
        self.dismissed = None;
        self.session = Some(ReferenceSession::new(token, current));
        if show_current {
            self.revision = self.revision.wrapping_add(1);
            None
        } else {
            self.start_scan(root)
        }
    }

    fn accepts(&self, revision: u64) -> bool {
        self.session.is_some() && self.revision == revision
    }

    fn start_scan(&mut self, root: &Path) -> Option<FileReferenceRequest> {
        self.revision = self.revision.wrapping_add(1);
        let session = self.session.as_mut()?;
        session.search = ReferenceSearch::Loading;
        Some(FileReferenceRequest::Scan {
            root: root.to_path_buf(),
            revision: self.revision,
        })
    }

    fn start_filter(&mut self) -> Option<FileReferenceRequest> {
        self.revision = self.revision.wrapping_add(1);
        let session = self.session.as_mut()?;
        session.search = ReferenceSearch::Filtering;
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
    let mut ranked = filter_files(files, query);
    let missing_current = current
        .filter(|current| {
            !ranked.iter().any(|entry| entry.path == current.path)
                && !filter_files(std::slice::from_ref(current), query).is_empty()
        })
        .cloned();
    ranked.extend(missing_current);

    // Stable sorting preserves fuzzy rank within the non-current partition.
    ranked.sort_by_key(|entry| Reverse(current.is_some_and(|current| current.path == entry.path)));
    ranked.into_iter().take(MAX_FILE_REFERENCES).collect()
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
        let quoted_at = ReferenceToken::at_cursor(&TextInput::new("@\"foo @bar")).unwrap();
        assert_eq!(quoted_at.query, "foo @bar");
        assert!(quoted_at.quoted);
        assert_eq!(
            ReferenceToken::at_cursor(&TextInput::new("@one @two")).map(|token| token.query),
            Some("two".to_string())
        );
        assert_eq!(
            ReferenceToken::at_cursor(&TextInput::new("@\"one\" @two")).map(|token| token.query),
            Some("two".to_string())
        );
        assert!(requires_quotes("a—b.txt"));
        assert!(requires_quotes("a“b.txt"));
        assert!(!requires_quotes("Ａ々🙂.rs"));
    }

    #[test]
    fn shared_matching_is_composed_with_current_buffer_priority() {
        let current = entry("src/main.rs");
        let files = vec![entry("alpha.rs"), entry("main.rs"), current.clone()];
        let ranked = rank_references(&files, "", Some(&current));
        assert_eq!(
            ranked
                .iter()
                .map(|entry| entry.display.as_str())
                .collect::<Vec<_>>(),
            ["src/main.rs", "alpha.rs", "main.rs"]
        );

        let unsaved = entry("unsaved.rs");
        let ranked = rank_references(&files, "unsaved", Some(&unsaved));
        assert_eq!(ranked, vec![unsaved]);
        assert!(rank_references(&files, "alpha", Some(&current))[0].display == "alpha.rs");
    }

    #[test]
    fn empty_query_shows_only_current_file_without_scanning() {
        let input = TextInput::new("@");
        let current = entry("src/main.rs");
        let mut state = FileReferenceState::default();

        assert!(
            state
                .update(&input, Path::new("/workspace"), Some(current.clone()))
                .is_none()
        );
        let view = state.view().unwrap();
        assert_eq!(view.status, FileReferenceStatus::Ready);
        assert_eq!(view.entries, &[current]);
        assert_eq!(view.selected, Some(0));
        assert_eq!(
            state.acceptance(&input).unwrap().replacement,
            "@src/main.rs "
        );

        let mut without_current = FileReferenceState::default();
        assert!(
            without_current
                .update(&input, Path::new("/workspace"), None)
                .is_none()
        );
        assert!(without_current.view().unwrap().entries.is_empty());
    }

    #[test]
    fn first_query_scans_and_returning_to_empty_cancels_it() {
        let empty = TextInput::new("@");
        let current = entry("src/main.rs");
        let mut state = FileReferenceState::default();
        assert!(
            state
                .update(&empty, Path::new("/workspace"), Some(current.clone()))
                .is_none()
        );

        let query = TextInput::new("@m");
        let Some(FileReferenceRequest::Scan { revision, .. }) =
            state.update(&query, Path::new("/workspace"), Some(current.clone()))
        else {
            panic!("the first query character should start a scan");
        };
        assert_eq!(state.view().unwrap().status, FileReferenceStatus::Loading);

        assert!(
            state
                .update(&empty, Path::new("/workspace"), Some(current.clone()))
                .is_none()
        );
        assert_eq!(state.view().unwrap().entries, &[current]);
        assert!(
            state
                .complete_scan(revision, Ok(vec![entry("stale.rs")]))
                .is_none()
        );
    }

    #[test]
    fn selection_wraps_and_acceptance_preserves_the_suffix() {
        let input = TextInput::new("@.");
        let mut state = ready_state(&input, Some(entry("src/main.rs")));
        assert_eq!(state.view().unwrap().selected, Some(0));
        state.select_previous();
        assert_eq!(state.view().unwrap().selected, Some(2));
        state.select_next();
        assert_eq!(state.view().unwrap().selected, Some(0));

        let mut input = TextInput::new("read @. before");
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
        let input = TextInput::new("@.");
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
        let input = TextInput::new("@m");
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
        assert_eq!(input.value(), "@m");

        let edited = TextInput::new("@ma");
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
                .is_none()
        );
        assert!(state.is_active());
        state.dismiss();

        let unquoted = TextInput::new("@");
        assert!(
            state
                .update(&unquoted, Path::new("/workspace"), None)
                .is_none()
        );
        assert!(state.is_active());
    }

    #[test]
    fn paths_with_spaces_are_inserted_as_quoted_references() {
        let input = TextInput::new("@.");
        let mut state = ready_state(&input, None);
        let revision = state.revision;
        state.complete_filter(revision, vec![entry("my folder/a file.txt")]);
        let acceptance = state.acceptance(&input).unwrap();
        assert_eq!(acceptance.replacement, "@\"my folder/a file.txt\" ");
    }
}
