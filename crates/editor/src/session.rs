use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    ops::Bound,
    path::Path,
};

use helix_view::{
    Document, View,
    editor::{Config, GutterConfig},
    graphics::Rect,
};
use thiserror::Error;

use crate::{
    active_editor::{ActiveEditor, SelectionBehavior},
    open::OpenedDocument,
    save::{SaveCompletion, SaveError, SaveRequest},
    snapshot::{
        BufferInfo, BufferTextSnapshot, CursorPosition, DirtyState, EditorSnapshot,
        TextViewSnapshot, apply_diff_snapshot, apply_selection_snapshot, text_view_snapshot,
        text_view_snapshot_at,
    },
    viewport,
};

/// Eggplant-owned identity for an open buffer.
///
/// Standalone Helix documents do not receive unique IDs, so editor state must
/// not use their `DocumentId` as the buffer identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BufferId(u64);

impl BufferId {
    /// Reconstruct an untrusted external identity for lookup in open editor state.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

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
    Visual,
    VisualLine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorMovement {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordMovement {
    NextStart,
    PreviousStart,
    NextEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAlignment {
    Top,
    Center,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertPosition {
    AfterCursor,
    LineEnd,
    FirstNonBlank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PastePosition {
    After,
    Before,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineBoundary {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenLineDirection {
    Below,
    Above,
}

/// Zero-based logical line and Unicode-scalar offset within that line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DocumentPosition {
    pub line: usize,
    pub character: usize,
}

impl From<VerticalDirection> for CursorMovement {
    fn from(direction: VerticalDirection) -> Self {
        match direction {
            VerticalDirection::Up => Self::Up,
            VerticalDirection::Down => Self::Down,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ExactTextReplacement<'a> {
    pub buffer_id: BufferId,
    pub path: &'a Path,
    pub expected_revision: i32,
    pub old_text: &'a str,
    pub new_text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppliedTextEdit {
    pub revision: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreatedDocument {
    pub buffer_id: BufferId,
    pub revision: i32,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExactTextError {
    #[error("no active editor view")]
    NoActiveView,
    #[error("buffer {0} does not exist")]
    UnknownBuffer(BufferId),
    #[error("buffer {0} path does not match the approved path")]
    PathMismatch(BufferId),
    #[error("buffer {buffer_id} revision changed from {expected} to {actual}")]
    RevisionConflict {
        buffer_id: BufferId,
        expected: i32,
        actual: i32,
    },
    #[error("buffer {0} has a save in progress")]
    SaveInProgress(BufferId),
    #[error("old text must be non-empty and differ from new text")]
    InvalidReplacement,
    #[error("buffer {0} no longer contains exactly one approved old-text match")]
    ExactTextConflict(BufferId),
}

#[derive(Debug, Error)]
pub enum EditorError {
    #[error("no active buffer")]
    NoActiveBuffer,
    #[error("buffer {0} does not exist")]
    UnknownBuffer(BufferId),
    #[error("buffer {0} has unsaved changes")]
    BufferModified(BufferId),
    #[error("buffer {0} has a save in progress")]
    SaveInProgress(BufferId),
    #[error("failed to save buffer {buffer_id}: {source}")]
    SaveDocument {
        buffer_id: BufferId,
        #[source]
        source: SaveError,
    },
    #[error("failed to reload buffer {buffer_id}: {source}")]
    ReloadDocument {
        buffer_id: BufferId,
        #[source]
        source: anyhow::Error,
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
    saving_buffers: HashSet<BufferId>,
    next_buffer_id: u64,
    mode: EditMode,
    scrolloff: usize,
    gutters: GutterConfig,
}

impl EditorSession {
    pub fn new() -> Self {
        let config = Config::default();
        Self {
            buffers: BTreeMap::new(),
            active_view: None,
            view_area: Rect::default(),
            saving_buffers: HashSet::new(),
            next_buffer_id: 1,
            mode: EditMode::Normal,
            scrolloff: config.scrolloff,
            gutters: viewport::gutters(),
        }
    }

    /// Insert and activate a document, deduplicating canonical file paths.
    pub fn insert_document(&mut self, document: Document) -> BufferId {
        if let Some(id) = document.path().and_then(|path| self.buffer_by_path(path)) {
            self.retarget_active_view(id);
            return id;
        }

        let id = self.allocate_buffer_id();
        self.buffers.insert(id, Buffer { document });
        self.retarget_active_view(id);
        id
    }

    pub fn activate_buffer(&mut self, id: BufferId) -> Result<(), EditorError> {
        if !self.buffers.contains_key(&id) {
            return Err(EditorError::UnknownBuffer(id));
        }

        self.retarget_active_view(id);
        Ok(())
    }

    pub fn activate_next_buffer(&mut self) -> Result<(), EditorError> {
        self.activate_adjacent_buffer(false)
    }

    pub fn activate_previous_buffer(&mut self) -> Result<(), EditorError> {
        self.activate_adjacent_buffer(true)
    }

    fn activate_adjacent_buffer(&mut self, previous: bool) -> Result<(), EditorError> {
        let active = self.active_buffer().ok_or(EditorError::NoActiveBuffer)?;
        if self.buffers.len() < 2 {
            return Ok(());
        }

        let target = if previous {
            self.buffers
                .range(..active)
                .next_back()
                .or_else(|| self.buffers.last_key_value())
        } else {
            self.buffers
                .range((Bound::Excluded(active), Bound::Unbounded))
                .next()
                .or_else(|| self.buffers.first_key_value())
        }
        .map(|(&id, _)| id)
        .ok_or(EditorError::NoActiveBuffer)?;

        self.activate_buffer(target)
    }

    /// Close an unmodified buffer that is not being saved.
    /// Closing the active buffer selects the next buffer by ID, wrapping to the
    /// first buffer. Closing the last buffer restores the empty editor state.
    pub fn close_buffer(&mut self, id: BufferId) -> Result<(), EditorError> {
        self.ensure_buffer_closeable(id)?;

        self.buffers.remove(&id);
        if self.active_buffer() == Some(id) {
            self.active_view = None;
            if let Some(next_id) = self.buffer_after(id) {
                self.retarget_active_view(next_id);
            }
        }

        Ok(())
    }

    /// Atomically close clean, non-saving buffers after validating every target.
    pub fn close_buffers(&mut self, ids: &[BufferId]) -> Result<(), EditorError> {
        for &id in ids {
            self.ensure_buffer_closeable(id)?;
        }
        for &id in ids {
            self.close_buffer(id)?;
        }
        Ok(())
    }

    fn ensure_buffer_closeable(&self, id: BufferId) -> Result<(), EditorError> {
        let buffer = self
            .buffers
            .get(&id)
            .ok_or(EditorError::UnknownBuffer(id))?;
        if self.saving_buffers.contains(&id) {
            return Err(EditorError::SaveInProgress(id));
        }
        if buffer.document.is_modified() {
            return Err(EditorError::BufferModified(id));
        }
        Ok(())
    }

    pub fn active_buffer(&self) -> Option<BufferId> {
        self.active_view.as_ref().map(|view| view.buffer)
    }

    /// Stable identities of all open file-backed buffers.
    pub fn file_buffer_ids(&self) -> Vec<BufferId> {
        self.buffers
            .iter()
            .filter_map(|(&id, buffer)| buffer.document.path().is_some().then_some(id))
            .collect()
    }

    /// Canonical path of an open file-backed buffer, if it has one.
    pub fn buffer_path(&self, id: BufferId) -> Option<&Path> {
        self.buffers.get(&id)?.document.path()
    }

    pub fn buffer_version(&self, id: BufferId) -> Option<i32> {
        Some(self.buffers.get(&id)?.document.version())
    }

    /// Apply a diff base only if the buffer still has the expected revision.
    pub fn set_buffer_diff_base(
        &mut self,
        id: BufferId,
        expected_version: i32,
        base: Vec<u8>,
    ) -> Result<bool, EditorError> {
        let buffer = self
            .buffers
            .get_mut(&id)
            .ok_or(EditorError::UnknownBuffer(id))?;
        if buffer.document.version() != expected_version {
            return Ok(false);
        }
        buffer.document.set_diff_base(base);
        Ok(true)
    }

    pub fn active_is_modified(&self) -> bool {
        self.active_view
            .as_ref()
            .is_some_and(|active| self.buffers[&active.buffer].document.is_modified())
    }

    pub fn has_modified_buffers(&self) -> bool {
        self.buffers
            .values()
            .any(|buffer| buffer.document.is_modified())
    }

    pub fn has_saving_buffers(&self) -> bool {
        !self.saving_buffers.is_empty()
    }

    pub fn is_saving(&self, id: BufferId) -> bool {
        self.saving_buffers.contains(&id)
    }

    pub fn is_modified(&self, id: BufferId) -> Option<bool> {
        self.buffers
            .get(&id)
            .map(|buffer| buffer.document.is_modified())
    }

    pub fn first_dirty_buffer(&self) -> Option<BufferId> {
        self.buffers
            .iter()
            .find_map(|(&id, buffer)| buffer.document.is_modified().then_some(id))
    }

    /// First modified file-backed buffer, excluding unsavable scratch documents.
    pub fn first_dirty_file_buffer(&self) -> Option<BufferId> {
        self.buffers.iter().find_map(|(&id, buffer)| {
            (buffer.document.is_modified() && buffer.document.path().is_some()).then_some(id)
        })
    }

    pub fn active_view_height(&self) -> Option<usize> {
        self.active_view
            .as_ref()
            .map(|active| active.view.inner_height())
    }

    pub fn active_view_width(&self) -> Option<usize> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        Some(active.view.inner_width(document).into())
    }

    /// Store the latest view dimensions and keep the active cursor visible.
    /// The size is retained while empty so the first newly opened buffer can use it.
    pub fn resize_active_view(&mut self, width: u16, height: u16) {
        self.view_area = viewport::area(width, height);
        if let Some(active) = self.active_view.as_mut() {
            active.view.area = self.view_area;
        }

        let scrolloff = self.scrolloff;
        if let Some((document, view)) = self.active_parts() {
            view.ensure_cursor_in_view(document, scrolloff);
        }
    }

    /// Return the active cursor as an absolute document line and character offset.
    pub fn active_position(&self) -> Option<DocumentPosition> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        let cursor = document
            .selection(active.view.id)
            .primary()
            .cursor(document.text().slice(..));
        let line = document.text().char_to_line(cursor);
        Some(DocumentPosition {
            line,
            character: cursor - document.text().line_to_char(line),
        })
    }

    /// Capture the active view for rendering without exposing Helix internals.
    pub fn active_snapshot(&self) -> Option<EditorSnapshot> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        let view = &active.view;
        let mut text = text_view_snapshot(document, view);
        if self.mode == EditMode::Visual || self.mode == EditMode::VisualLine {
            apply_selection_snapshot(&mut text, document, view);
        }
        apply_diff_snapshot(&mut text, document);
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
            buffers: self.buffer_infos(),
            mode: self.mode,
            modified: DirtyState::from(document.is_modified()),
            text,
            cursor,
        })
    }

    /// Return the active buffer identity and document version, which changes on every edit.
    pub fn active_revision(&self) -> Option<(BufferId, i32)> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        Some((active.buffer, document.version()))
    }

    /// Clone one buffer's text into an owned snapshot suitable for background work.
    pub fn buffer_text_snapshot(&self, buffer_id: BufferId) -> Option<BufferTextSnapshot> {
        let document = &self.buffers.get(&buffer_id)?.document;
        Some(BufferTextSnapshot {
            buffer_id,
            revision: document.version(),
            text: document.text().to_string(),
        })
    }

    /// Clone active text into an owned snapshot suitable for background search.
    pub fn active_text_snapshot(&self) -> Option<BufferTextSnapshot> {
        self.active_buffer()
            .and_then(|buffer_id| self.buffer_text_snapshot(buffer_id))
    }

    /// Project a match-centered active-buffer range with current syntax highlights.
    pub fn active_preview_at_line(&self, line: usize, height: usize) -> Option<TextViewSnapshot> {
        let active = self.active_view.as_ref()?;
        let document = &self.buffers.get(&active.buffer)?.document;
        let max_first_line = document.text().len_lines().saturating_sub(height.max(1));
        let first_line = line.saturating_sub(height / 2).min(max_first_line);
        Some(text_view_snapshot_at(document, first_line, height))
    }

    pub fn buffer_count(&self) -> usize {
        self.buffers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Borrow the active document/view through the focused editing capability.
    pub fn active_editor(&mut self) -> Result<ActiveEditor<'_>, EditorError> {
        let scrolloff = self.scrolloff;
        let selection_behavior = match self.mode {
            EditMode::Visual => SelectionBehavior::Extend,
            EditMode::VisualLine => SelectionBehavior::ExtendLines,
            EditMode::Normal | EditMode::Insert => SelectionBehavior::Move,
        };
        self.active_parts()
            .map(|(document, view)| {
                ActiveEditor::new(document, view, scrolloff, selection_behavior)
            })
            .ok_or(EditorError::NoActiveBuffer)
    }

    pub fn mode(&self) -> EditMode {
        self.mode
    }

    pub fn enter_insert_mode(&mut self) {
        if self.active_view.is_some() {
            self.mode = EditMode::Insert;
        }
    }

    pub fn enter_visual_mode(&mut self) {
        if self.active_view.is_some() {
            if let Ok(mut editor) = self.active_editor() {
                editor.start_selection();
            }
            self.mode = EditMode::Visual;
        }
    }

    pub fn enter_visual_line_mode(&mut self) {
        if self.active_view.is_some() {
            if let Ok(mut editor) = self.active_editor() {
                editor.start_line_selection();
            }
            self.mode = EditMode::VisualLine;
        }
    }

    pub fn enter_normal_mode(&mut self) {
        if let EditMode::Visual | EditMode::VisualLine = self.mode
            && let Ok(mut editor) = self.active_editor()
        {
            editor.collapse_selection();
        }
        if self.mode == EditMode::Insert
            && let Some((document, view)) = self.active_parts()
        {
            document.append_changes_to_history(view);
        }
        if self.mode == EditMode::Insert
            && let Ok(mut editor) = self.active_editor()
        {
            editor.move_cursor_to_normal();
        }
        self.mode = EditMode::Normal;
    }

    /// Apply a prepared external reload if the buffer has not changed since scheduling.
    /// Returns `false` for a stale completion and `true` after applying the document state.
    pub fn complete_external_reload(
        &mut self,
        id: BufferId,
        expected_version: i32,
        result: Result<OpenedDocument, String>,
    ) -> Result<bool, EditorError> {
        if self.saving_buffers.contains(&id) {
            return Err(EditorError::SaveInProgress(id));
        }
        let active_buffer = self.active_buffer();
        let view = &mut self
            .active_view
            .as_mut()
            .ok_or(EditorError::NoActiveBuffer)?
            .view;
        let document = &mut self
            .buffers
            .get_mut(&id)
            .ok_or(EditorError::UnknownBuffer(id))?
            .document;
        if document.version() != expected_version {
            return Ok(false);
        }
        if document.is_modified() {
            return Err(EditorError::BufferModified(id));
        }

        let prepared = result
            .and_then(|opened| opened.into_prepared())
            .map_err(anyhow::Error::msg)
            .and_then(|prepared| {
                if prepared.existed {
                    Ok(prepared)
                } else {
                    Err(anyhow::Error::msg("file no longer exists"))
                }
            })
            .map_err(|source| EditorError::ReloadDocument {
                buffer_id: id,
                source,
            })?;
        let transaction = helix_core::diff::compare_ropes(document.text(), &prepared.text);
        document.apply(&transaction, view.id);
        document.append_changes_to_history(view);
        document.reset_modified();
        document.pickup_last_saved_time();
        document.language = prepared.language;
        document.syntax = prepared.syntax;
        document.detect_indent_and_line_ending();
        if active_buffer == Some(id) {
            view.ensure_cursor_in_view(document, self.scrolloff);
        }
        Ok(true)
    }

    /// Apply one approved exact-text replacement without changing the active buffer.
    pub fn replace_exact_text(
        &mut self,
        replacement: ExactTextReplacement<'_>,
    ) -> Result<AppliedTextEdit, ExactTextError> {
        if replacement.old_text.is_empty() || replacement.old_text == replacement.new_text {
            return Err(ExactTextError::InvalidReplacement);
        }
        if self.saving_buffers.contains(&replacement.buffer_id) {
            return Err(ExactTextError::SaveInProgress(replacement.buffer_id));
        }
        let active_buffer = self.active_buffer();
        let view = &mut self
            .active_view
            .as_mut()
            .ok_or(ExactTextError::NoActiveView)?
            .view;
        let document = &mut self
            .buffers
            .get_mut(&replacement.buffer_id)
            .ok_or(ExactTextError::UnknownBuffer(replacement.buffer_id))?
            .document;
        if document.path() != Some(replacement.path) {
            return Err(ExactTextError::PathMismatch(replacement.buffer_id));
        }
        let actual = document.version();
        if actual != replacement.expected_revision {
            return Err(ExactTextError::RevisionConflict {
                buffer_id: replacement.buffer_id,
                expected: replacement.expected_revision,
                actual,
            });
        }
        let text = document.text().to_string();
        let matches = text
            .match_indices(replacement.old_text)
            .take(2)
            .collect::<Vec<_>>();
        let (start_byte, end_byte) = match matches.as_slice() {
            [(start, _)] => (*start, *start + replacement.old_text.len()),
            _ => return Err(ExactTextError::ExactTextConflict(replacement.buffer_id)),
        };
        let (start, end) = (
            document.text().byte_to_char(start_byte),
            document.text().byte_to_char(end_byte),
        );
        let transaction = helix_core::Transaction::change(
            document.text(),
            [(start, end, Some(replacement.new_text.into()))].into_iter(),
        );
        document.ensure_view_init(view.id);
        document
            .apply(&transaction, view.id)
            .then_some(())
            .ok_or(ExactTextError::ExactTextConflict(replacement.buffer_id))?;
        document.append_changes_to_history(view);
        let revision = document.version();

        if active_buffer == Some(replacement.buffer_id) {
            view.ensure_cursor_in_view(document, self.scrolloff);
        }
        Ok(AppliedTextEdit { revision })
    }

    /// Initialize a newly opened file-backed document with full text and edit history.
    pub fn create_file_document(
        &mut self,
        document: Document,
        text: &str,
    ) -> Result<CreatedDocument, ExactTextError> {
        let buffer_id = self.insert_document(document);
        let view = self
            .active_view
            .as_mut()
            .ok_or(ExactTextError::NoActiveView)?;
        let document = &mut self
            .buffers
            .get_mut(&buffer_id)
            .ok_or(ExactTextError::UnknownBuffer(buffer_id))?
            .document;
        if document.text() != text {
            let transaction = helix_core::Transaction::change(
                document.text(),
                [(0, document.text().len_chars(), Some(text.into()))].into_iter(),
            );
            document.ensure_view_init(view.view.id);
            document
                .apply(&transaction, view.view.id)
                .then_some(())
                .ok_or(ExactTextError::ExactTextConflict(buffer_id))?;
            document.append_changes_to_history(&mut view.view);
        }
        Ok(CreatedDocument {
            buffer_id,
            revision: document.version(),
        })
    }

    /// Prepare an asynchronous save of one buffer's current snapshot.
    pub fn save_buffer(&mut self, id: BufferId) -> Result<SaveRequest, EditorError> {
        if self.saving_buffers.contains(&id) {
            return Err(EditorError::SaveInProgress(id));
        }
        let buffer = self
            .buffers
            .get_mut(&id)
            .ok_or(EditorError::UnknownBuffer(id))?;
        let future = buffer
            .document
            .save(None::<&Path>, false)
            .map_err(|source| EditorError::SaveDocument {
                buffer_id: id,
                source: SaveError::from(source),
            })?;
        self.saving_buffers.insert(id);
        Ok(SaveRequest {
            buffer_id: id,
            future: Box::pin(future),
        })
    }

    /// Apply a save completion without clearing edits made after its snapshot.
    pub fn complete_save(&mut self, completion: SaveCompletion) -> Result<usize, EditorError> {
        let id = completion.buffer_id;
        self.saving_buffers.remove(&id);
        let buffer = self
            .buffers
            .get_mut(&id)
            .ok_or(EditorError::UnknownBuffer(id))?;
        let saved = completion
            .result
            .map_err(|source| EditorError::SaveDocument {
                buffer_id: id,
                source,
            })?;
        buffer
            .document
            .set_last_saved_revision(saved.revision, saved.save_time);
        Ok(saved.revision)
    }

    fn active_parts(&mut self) -> Option<(&mut Document, &mut View)> {
        let active = self.active_view.as_mut()?;
        let buffer = self.buffers.get_mut(&active.buffer)?;
        Some((&mut buffer.document, &mut active.view))
    }

    fn buffer_infos(&self) -> Vec<BufferInfo> {
        self.buffers
            .iter()
            .map(|(&id, buffer)| {
                let document = &buffer.document;
                let name = document
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| document.display_name().into_owned());
                BufferInfo {
                    id,
                    name,
                    modified: DirtyState::from(document.is_modified()),
                }
            })
            .collect()
    }

    pub fn buffer_by_path(&self, path: &Path) -> Option<BufferId> {
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
        let gutters = self.gutters.clone();
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
mod tests;
