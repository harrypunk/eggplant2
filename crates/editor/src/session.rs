use std::{
    collections::BTreeMap,
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use arc_swap::ArcSwap;
use helix_core::{
    Rope, RopeSlice, Transaction,
    doc_formatter::TextFormat,
    graphemes,
    movement::{self, Direction, Movement},
    syntax,
    text_annotations::TextAnnotations,
};
use helix_view::{Document, View, document::DocumentOpenError, editor::Config, graphics::Rect};
use thiserror::Error;

use crate::snapshot::{CursorPosition, EditorSnapshot};

type MoveFn = fn(
    RopeSlice,
    helix_core::Range,
    Direction,
    usize,
    Movement,
    &TextFormat,
    &mut TextAnnotations,
) -> helix_core::Range;

/// Shared Helix configuration with atomically replaceable snapshots.
///
/// `ArcSwap<Config>` stores an `Arc<Config>` internally. The outer `Arc`
/// shares the swap cell with all documents owned by the session.
type SharedConfig = Arc<ArcSwap<Config>>;

/// Helix documents require a shared syntax loader, even when syntax is unused.
type SharedSyntaxLoader = Arc<ArcSwap<syntax::Loader>>;

/// Eggplant-owned identity for an open buffer.
///
/// Standalone Helix documents do not receive unique IDs, so editor state must
/// not use their `DocumentId` as the buffer identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(u64);

impl fmt::Display for BufferId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditMode {
    #[default]
    Normal,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMovement {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("no active buffer")]
    NoActiveBuffer,
    #[error("buffer {0} does not exist")]
    UnknownBuffer(BufferId),
    #[error("buffer {0} has unsaved changes")]
    BufferModified(BufferId),
    #[error("failed to open `{}`: {source}", path.display())]
    OpenDocument {
        path: PathBuf,
        #[source]
        source: DocumentOpenError,
    },
}

struct Buffer {
    document: Document,
}

struct EditorView {
    buffer: BufferId,
    view: View,
}

/// Owns open buffers and the optional visible editor view.
pub struct EditorSession {
    buffers: BTreeMap<BufferId, Buffer>,
    active_view: Option<EditorView>,
    view_area: Rect,
    next_buffer_id: u64,
    mode: EditMode,
    config: SharedConfig,
    syntax_loader: SharedSyntaxLoader,
}

impl EditorSession {
    pub fn new() -> Self {
        Self {
            buffers: BTreeMap::new(),
            active_view: None,
            view_area: Rect::default(),
            next_buffer_id: 1,
            mode: EditMode::Normal,
            // Helix's Document::from accepts a DynAccess<Config>. ArcSwap
            // provides that interface and can later replace the whole config
            // snapshot during a reload without mutating it under a lock.
            config: Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            // Basic editing does not load grammars; the default loader simply
            // satisfies Document's constructor and is shared by all buffers.
            syntax_loader: Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        }
    }

    /// Create and activate an unsaved in-memory buffer.
    pub fn create_buffer(&mut self, text: &str) -> BufferId {
        let document = Document::from(
            Rope::from(text),
            None,
            self.config.clone(),
            self.syntax_loader.clone(),
        );
        self.insert_document(document)
    }

    /// Open and activate a file-backed buffer.
    ///
    /// Reopening the same canonical path activates its existing buffer. As in
    /// Helix, a nonexistent path creates a new document that can be saved later.
    pub fn open_file(&mut self, path: impl AsRef<Path>) -> Result<BufferId, EditorError> {
        let path = helix_stdx::path::canonicalize(path);
        if let Some(id) = self.buffer_by_path(&path) {
            self.retarget_active_view(id);
            return Ok(id);
        }

        let document = Document::open(
            &path,
            None,
            false,
            self.config.clone(),
            self.syntax_loader.clone(),
        )
        .map_err(|source| EditorError::OpenDocument {
            path: path.clone(),
            source,
        })?;

        Ok(self.insert_document(document))
    }

    pub fn activate_buffer(&mut self, id: BufferId) -> Result<(), EditorError> {
        if !self.buffers.contains_key(&id) {
            return Err(EditorError::UnknownBuffer(id));
        }

        self.retarget_active_view(id);
        Ok(())
    }

    /// Close an unmodified buffer.
    ///
    /// Closing the active buffer selects the next buffer by ID, wrapping to the
    /// first buffer. Closing the last buffer restores the empty editor state.
    pub fn close_buffer(&mut self, id: BufferId) -> Result<(), EditorError> {
        let buffer = self
            .buffers
            .get(&id)
            .ok_or(EditorError::UnknownBuffer(id))?;
        if buffer.document.is_modified() {
            return Err(EditorError::BufferModified(id));
        }

        self.buffers.remove(&id);
        if self.active_buffer() == Some(id) {
            self.active_view = None;
            if let Some(next_id) = self.buffer_after(id) {
                self.retarget_active_view(next_id);
            }
        }

        Ok(())
    }

    pub fn active_buffer(&self) -> Option<BufferId> {
        self.active_view.as_ref().map(|view| view.buffer)
    }

    pub fn active_is_modified(&self) -> bool {
        self.active_view
            .as_ref()
            .is_some_and(|active| self.buffers[&active.buffer].document.is_modified())
    }

    /// Store the latest view dimensions and keep the active cursor visible.
    /// The size is retained while empty so the first newly opened buffer can use it.
    pub fn resize_active_view(&mut self, width: u16, height: u16) {
        self.view_area = Rect::new(0, 0, width, height);
        if let Some(active) = self.active_view.as_mut() {
            active.view.area = self.view_area;
        }

        let scrolloff = self.config.load().scrolloff;
        if let Some((document, view)) = self.active_parts() {
            view.ensure_cursor_in_view(document, scrolloff);
        }
    }

    /// Capture the active view for rendering without exposing Helix internals.
    pub fn active_snapshot(&self) -> Option<EditorSnapshot> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        let view = &active.view;
        let offset = document.view_offset(view.id);
        let first_line = document
            .text()
            .char_to_line(offset.anchor.min(document.text().len_chars()));
        let end_line = (first_line + view.inner_height()).min(document.text().len_lines());
        let lines = (first_line..end_line)
            .map(|line| {
                document
                    .text()
                    .line(line)
                    .to_string()
                    .trim_end_matches(['\r', '\n'])
                    .to_owned()
            })
            .collect();
        let cursor = document
            .selection(view.id)
            .primary()
            .cursor(document.text().slice(..));
        let cursor = view
            .screen_coords_at_pos(document, document.text().slice(..), cursor)
            .map(|position| CursorPosition {
                row: position.row,
                column: position.col,
            });

        Some(EditorSnapshot {
            buffer_id: active.buffer,
            display_name: document.display_name().into_owned(),
            mode: self.mode,
            modified: document.is_modified(),
            lines,
            horizontal_offset: offset.horizontal_offset,
            cursor,
        })
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    pub fn mode(&self) -> EditMode {
        self.mode
    }

    pub fn enter_insert_mode(&mut self) {
        if self.active_view.is_some() {
            self.mode = EditMode::Insert;
        }
    }

    pub fn enter_normal_mode(&mut self) {
        if self.mode == EditMode::Insert
            && let Some((document, view)) = self.active_parts()
        {
            document.append_changes_to_history(view);
        }
        self.mode = EditMode::Normal;
    }

    /// Move the active cursor without extending the selection.
    pub fn move_cursor(&mut self, movement: CursorMovement) -> Result<(), EditorError> {
        let scrolloff = self.config.load().scrolloff;
        let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
        let text = document.text().slice(..);
        let text_format = document.text_format(view.inner_area(document).width, None);
        let mut annotations = view.text_annotations(document, None);

        let selection = document.selection(view.id).clone().transform(|range| {
            let (direction, move_fn): (Direction, MoveFn) = match movement {
                CursorMovement::Left => (Direction::Backward, movement::move_horizontally),
                CursorMovement::Right => (Direction::Forward, movement::move_horizontally),
                CursorMovement::Up => (Direction::Backward, movement::move_vertically),
                CursorMovement::Down => (Direction::Forward, movement::move_vertically),
            };
            move_fn(
                text,
                range,
                direction,
                1,
                Movement::Move,
                &text_format,
                &mut annotations,
            )
        });
        drop(annotations);

        document.set_selection(view.id, selection);
        view.ensure_cursor_in_view(document, scrolloff);
        Ok(())
    }

    /// Insert text at each active selection cursor.
    pub fn insert_text(&mut self, text: &str) -> Result<(), EditorError> {
        if text.is_empty() {
            return Ok(());
        }

        let transaction = {
            let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
            let document_text = document.text().slice(..);
            Transaction::change_by_and_with_selection(
                document.text(),
                document.selection(view.id),
                |range| {
                    let cursor = range.cursor(document_text);
                    ((cursor, cursor, Some(text.into())), None)
                },
            )
        };

        self.apply_active_transaction(transaction)
    }

    /// Delete the grapheme before each active selection cursor.
    pub fn backspace(&mut self) -> Result<(), EditorError> {
        let transaction = {
            let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
            let document_text = document.text().slice(..);
            Transaction::delete_by_and_with_selection(
                document.text(),
                document.selection(view.id),
                |range| {
                    let cursor = range.cursor(document_text);
                    (
                        (
                            graphemes::prev_grapheme_boundary(document_text, cursor),
                            cursor,
                        ),
                        None,
                    )
                },
            )
        };

        self.apply_active_transaction(transaction)
    }

    pub fn undo(&mut self) -> Result<bool, EditorError> {
        let scrolloff = self.config.load().scrolloff;
        let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
        let changed = document.undo(view);
        view.ensure_cursor_in_view(document, scrolloff);
        Ok(changed)
    }

    pub fn redo(&mut self) -> Result<bool, EditorError> {
        let scrolloff = self.config.load().scrolloff;
        let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
        let changed = document.redo(view);
        view.ensure_cursor_in_view(document, scrolloff);
        Ok(changed)
    }

    fn apply_active_transaction(&mut self, transaction: Transaction) -> Result<(), EditorError> {
        let scrolloff = self.config.load().scrolloff;
        let (document, view) = self.active_parts().ok_or(EditorError::NoActiveBuffer)?;
        document.apply(&transaction, view.id);
        view.ensure_cursor_in_view(document, scrolloff);
        Ok(())
    }

    fn active_parts(&mut self) -> Option<(&mut Document, &mut View)> {
        let active = self.active_view.as_mut()?;
        let buffer = self.buffers.get_mut(&active.buffer)?;
        Some((&mut buffer.document, &mut active.view))
    }

    fn insert_document(&mut self, document: Document) -> BufferId {
        let id = self.allocate_buffer_id();
        self.buffers.insert(id, Buffer { document });
        self.retarget_active_view(id);
        id
    }

    fn buffer_by_path(&self, path: &Path) -> Option<BufferId> {
        self.buffers
            .iter()
            .find_map(|(&id, buffer)| (buffer.document.path() == Some(path)).then_some(id))
    }

    fn allocate_buffer_id(&mut self) -> BufferId {
        let id = BufferId(self.next_buffer_id);
        self.next_buffer_id = self
            .next_buffer_id
            .checked_add(1)
            .expect("buffer ID space exhausted");
        id
    }

    fn retarget_active_view(&mut self, id: BufferId) {
        // Load the current snapshot. Cloning the small gutter config keeps the
        // guard independent of any later configuration swap.
        let gutters = self.config.load().gutters.clone();
        let document = &mut self
            .buffers
            .get_mut(&id)
            .expect("retargeting requires an existing buffer")
            .document;

        match self.active_view.as_mut() {
            Some(active) => {
                active.buffer = id;
                active.view.doc = document.id();
                document.ensure_view_init(active.view.id);
            }
            None => {
                let mut view = View::new(document.id(), gutters);
                view.area = self.view_area;
                document.ensure_view_init(view.id);
                self.active_view = Some(EditorView { buffer: id, view });
            }
        }
    }

    fn buffer_after(&self, closed_id: BufferId) -> Option<BufferId> {
        self.buffers
            .range(closed_id..)
            .next()
            .or_else(|| self.buffers.first_key_value())
            .map(|(&id, _)| id)
    }
}

impl Default for EditorSession {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for EditorSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorSession")
            .field("buffer_count", &self.buffer_count())
            .field("active_buffer", &self.active_buffer())
            .field("mode", &self.mode)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use helix_core::{Selection, Transaction};
    use helix_view::graphics::Rect;

    use super::*;

    fn active_parts(session: &mut EditorSession) -> (&mut Document, &mut View) {
        session.active_parts().unwrap()
    }

    fn active_cursor(session: &mut EditorSession) -> usize {
        let (document, view) = active_parts(session);
        document
            .selection(view.id)
            .primary()
            .cursor(document.text().slice(..))
    }

    #[test]
    fn starts_in_empty_state() {
        let mut session = EditorSession::new();
        session.resize_active_view(80, 24);

        assert!(session.is_empty());
        assert!(session.active_snapshot().is_none());
        assert_eq!(session.active_buffer(), None);
        assert_eq!(session.mode(), EditMode::Normal);
    }

    #[test]
    fn creates_distinct_buffers_and_activates_by_id() {
        let mut session = EditorSession::new();
        let first = session.create_buffer("first");
        let second = session.create_buffer("second");

        assert_ne!(first, second);
        assert_eq!(session.buffer_count(), 2);
        assert_eq!(session.active_buffer(), Some(second));

        session.activate_buffer(first).unwrap();
        assert_eq!(session.active_buffer(), Some(first));
    }

    #[test]
    fn rejects_unknown_buffer() {
        let mut session = EditorSession::new();
        let unknown = BufferId(999);

        assert!(matches!(
            session.activate_buffer(unknown),
            Err(EditorError::UnknownBuffer(id)) if id == unknown
        ));
        assert!(matches!(
            session.close_buffer(unknown),
            Err(EditorError::UnknownBuffer(id)) if id == unknown
        ));
    }

    #[test]
    fn closing_active_buffer_selects_next_or_empty_state() {
        let mut session = EditorSession::new();
        let first = session.create_buffer("first");
        let second = session.create_buffer("second");

        session.close_buffer(second).unwrap();
        assert_eq!(session.active_buffer(), Some(first));

        session.close_buffer(first).unwrap();
        assert!(session.is_empty());
        assert_eq!(session.active_buffer(), None);
    }

    #[test]
    fn rejects_closing_modified_buffer() {
        let mut session = EditorSession::new();
        let id = session.create_buffer("hello");
        let (document, view) = active_parts(&mut session);
        document.set_selection(view.id, Selection::point(5));
        let transaction =
            Transaction::insert(document.text(), document.selection(view.id), "!".into());
        document.apply(&transaction, view.id);

        assert!(matches!(
            session.close_buffer(id),
            Err(EditorError::BufferModified(closed_id)) if closed_id == id
        ));
        assert_eq!(session.active_buffer(), Some(id));
    }

    #[test]
    fn editing_without_active_buffer_is_rejected() {
        let mut session = EditorSession::new();

        assert!(matches!(
            session.insert_text("text"),
            Err(EditorError::NoActiveBuffer)
        ));
        assert!(matches!(
            session.backspace(),
            Err(EditorError::NoActiveBuffer)
        ));
        assert!(matches!(session.undo(), Err(EditorError::NoActiveBuffer)));
        assert!(matches!(session.redo(), Err(EditorError::NoActiveBuffer)));
        assert!(matches!(
            session.move_cursor(CursorMovement::Left),
            Err(EditorError::NoActiveBuffer)
        ));
    }

    #[test]
    fn moves_horizontally_within_document_boundaries() {
        let mut session = EditorSession::new();
        session.create_buffer("ab");
        assert_eq!(active_cursor(&mut session), 0);

        session.move_cursor(CursorMovement::Right).unwrap();
        assert_eq!(active_cursor(&mut session), 1);
        session.move_cursor(CursorMovement::Right).unwrap();
        assert_eq!(active_cursor(&mut session), 2);
        session.move_cursor(CursorMovement::Right).unwrap();
        assert_eq!(active_cursor(&mut session), 2);

        session.move_cursor(CursorMovement::Left).unwrap();
        assert_eq!(active_cursor(&mut session), 1);
        session.move_cursor(CursorMovement::Left).unwrap();
        session.move_cursor(CursorMovement::Left).unwrap();
        assert_eq!(active_cursor(&mut session), 0);
        assert!(!session.active_is_modified());
    }

    #[test]
    fn vertical_movement_remembers_the_desired_column() {
        let mut session = EditorSession::new();
        session.create_buffer("abcd\nx\nwxyz");
        let (document, view) = active_parts(&mut session);
        document.set_selection(view.id, Selection::point(3));
        assert_eq!(active_cursor(&mut session), 3);

        session.move_cursor(CursorMovement::Down).unwrap();
        // Helix uses the end-of-line position when the target line is shorter.
        assert_eq!(active_cursor(&mut session), 6);
        session.move_cursor(CursorMovement::Down).unwrap();
        assert_eq!(active_cursor(&mut session), 10);
        session.move_cursor(CursorMovement::Down).unwrap();
        assert_eq!(active_cursor(&mut session), 10);

        session.move_cursor(CursorMovement::Up).unwrap();
        assert_eq!(active_cursor(&mut session), 6);
        session.move_cursor(CursorMovement::Up).unwrap();
        assert_eq!(active_cursor(&mut session), 3);
    }

    #[test]
    fn movement_scrolls_cursor_into_view() {
        let text = (0..=8)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut session = EditorSession::new();
        session.create_buffer(&text);
        let (_, view) = active_parts(&mut session);
        view.area = Rect::new(0, 0, 20, 4);

        for _ in 0..6 {
            session.move_cursor(CursorMovement::Down).unwrap();
        }

        let (document, view) = active_parts(&mut session);
        assert!(document.view_offset(view.id).anchor > 0);
        assert!(view.is_cursor_in_view(document, 0));
    }

    #[test]
    fn inserts_text_and_deletes_previous_character() {
        let mut session = EditorSession::new();
        let id = session.create_buffer("hello");
        session.enter_insert_mode();
        let (document, view) = active_parts(&mut session);
        document.set_selection(view.id, Selection::point(5));

        session.insert_text(", world").unwrap();
        session.backspace().unwrap();

        assert_eq!(session.buffers[&id].document.text(), "hello, worl");
        assert!(session.active_is_modified());
    }

    #[test]
    fn backspace_removes_the_whole_grapheme() {
        let mut session = EditorSession::new();
        let id = session.create_buffer("e\u{301}");
        let (document, view) = active_parts(&mut session);
        let end = document.text().len_chars();
        document.set_selection(view.id, Selection::point(end));

        session.backspace().unwrap();
        assert_eq!(session.buffers[&id].document.text(), "");

        session.backspace().unwrap();
        assert_eq!(session.buffers[&id].document.text(), "");
    }

    #[test]
    fn undo_and_redo_restore_an_insert_session() {
        let mut session = EditorSession::new();
        let id = session.create_buffer("a");
        session.enter_insert_mode();
        let (document, view) = active_parts(&mut session);
        document.set_selection(view.id, Selection::point(1));
        session.insert_text("b").unwrap();
        session.insert_text("c").unwrap();
        session.enter_normal_mode();

        assert_eq!(session.buffers[&id].document.text(), "abc");
        assert!(session.undo().unwrap());
        assert_eq!(session.buffers[&id].document.text(), "a");
        assert!(!session.active_is_modified());
        assert!(session.redo().unwrap());
        assert_eq!(session.buffers[&id].document.text(), "abc");
        assert!(session.active_is_modified());
    }

    #[test]
    fn first_buffer_uses_size_received_while_empty() {
        let mut session = EditorSession::new();
        session.resize_active_view(20, 4);
        session.create_buffer("visible");

        let snapshot = session.active_snapshot().unwrap();
        assert_eq!(snapshot.lines, ["visible"]);
        assert!(snapshot.cursor.is_some());
    }

    #[test]
    fn snapshot_contains_visible_lines_and_cursor() {
        let mut session = EditorSession::new();
        let id = session.create_buffer("zero\none\ntwo");
        session.resize_active_view(20, 3);

        let snapshot = session.active_snapshot().unwrap();
        assert_eq!(snapshot.buffer_id, id);
        assert_eq!(snapshot.lines, ["zero", "one"]);
        assert_eq!(snapshot.cursor, Some(CursorPosition { row: 0, column: 0 }));
        assert_eq!(snapshot.horizontal_offset, 0);
        assert_eq!(snapshot.mode, EditMode::Normal);
        assert!(!snapshot.modified);
    }

    #[test]
    fn snapshot_follows_the_scrolled_cursor() {
        let text = (0..=8)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut session = EditorSession::new();
        session.create_buffer(&text);
        session.resize_active_view(20, 4);

        for _ in 0..6 {
            session.move_cursor(CursorMovement::Down).unwrap();
        }

        let snapshot = session.active_snapshot().unwrap();
        let cursor = snapshot.cursor.unwrap();
        assert_eq!(snapshot.lines.len(), 3);
        assert!(snapshot.lines.contains(&"6".to_string()));
        assert!(cursor.row < snapshot.lines.len());
    }

    #[test]
    fn opens_files_and_deduplicates_canonical_paths() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "first").unwrap();
        std::fs::write(&second_path, "second").unwrap();

        let mut session = EditorSession::new();
        let first = session.open_file(&first_path).unwrap();
        let second = session.open_file(&second_path).unwrap();
        assert_ne!(first, second);
        assert_eq!(session.active_buffer(), Some(second));

        let aliased_path = directory.path().join(".").join("first.txt");
        let reopened = session.open_file(aliased_path).unwrap();
        assert_eq!(reopened, first);
        assert_eq!(session.active_buffer(), Some(first));
        assert_eq!(session.buffer_count(), 2);
        assert_eq!(session.buffers[&first].document.text(), "first");
    }

    #[test]
    fn rejects_directory_without_creating_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let mut session = EditorSession::new();

        let error = session.open_file(directory.path()).unwrap_err();
        assert!(matches!(
            error,
            EditorError::OpenDocument {
                source: DocumentOpenError::IrregularFile,
                ..
            }
        ));
        assert!(session.is_empty());
        assert_eq!(session.active_buffer(), None);
    }

    #[test]
    fn switching_buffers_preserves_cursor_and_scroll() {
        let text = (0..=8)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut session = EditorSession::new();
        let first = session.create_buffer(&text);

        let (expected_offset, expected_selection) = {
            let (document, view) = active_parts(&mut session);
            view.area = Rect::new(0, 0, 20, 4);
            let cursor = document.text().line_to_char(6);
            document.set_selection(view.id, Selection::point(cursor));
            view.ensure_cursor_in_view(document, 0);
            (
                document.view_offset(view.id),
                document.selection(view.id).clone(),
            )
        };

        let second = session.create_buffer("second");
        assert_eq!(session.active_buffer(), Some(second));
        session.activate_buffer(first).unwrap();

        let (document, view) = active_parts(&mut session);
        assert_eq!(document.selection(view.id), &expected_selection);
        assert_eq!(document.view_offset(view.id), expected_offset);
        assert!(view.is_cursor_in_view(document, 0));
    }
}
