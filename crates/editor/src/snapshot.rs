use helix_view::{Document, View};

use crate::session::{BufferId, EditMode};

/// Read-only projection of a document viewport, shared by editors and previews.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextViewSnapshot {
    /// Zero-based index of the first visible line in the full document.
    pub first_line: usize,
    /// Total logical line count, used for line-number gutter sizing.
    pub line_count: usize,
    pub lines: Vec<String>,
    pub horizontal_offset: usize,
}

/// Cursor coordinates relative to the editor's visible text area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyState {
    Clean,
    Dirty,
}

impl From<bool> for DirtyState {
    fn from(modified: bool) -> Self {
        match modified {
            true => Self::Dirty,
            false => Self::Clean,
        }
    }
}

/// Filename-only buffer summary used by tabs and other buffer lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferInfo {
    pub id: BufferId,
    pub name: String,
    pub modified: DirtyState,
}

/// Owned, read-only data needed to render one active editor view.
///
/// This keeps Helix document/view types inside the editor crate. The snapshot
/// currently contains logical lines; visual soft-wrap rendering is deferred.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSnapshot {
    pub buffer_id: BufferId,
    pub display_name: String,
    pub buffers: Vec<BufferInfo>,
    pub mode: EditMode,
    pub modified: DirtyState,
    pub text: TextViewSnapshot,
    pub cursor: Option<CursorPosition>,
}

/// Project a Helix document/view without exposing either type to the TUI.
pub(crate) fn text_view_snapshot(document: &Document, view: &View) -> TextViewSnapshot {
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

    TextViewSnapshot {
        first_line,
        line_count: document.text().len_lines(),
        lines,
        horizontal_offset: offset.horizontal_offset,
    }
}
