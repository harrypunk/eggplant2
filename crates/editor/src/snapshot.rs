use crate::session::{BufferId, EditMode};

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
    /// Zero-based index of the first visible line in the full document.
    pub first_line: usize,
    /// Total logical line count, used for line-number gutter sizing.
    pub line_count: usize,
    pub lines: Vec<String>,
    pub horizontal_offset: usize,
    pub cursor: Option<CursorPosition>,
}
