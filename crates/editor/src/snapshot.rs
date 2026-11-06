use crate::session::{BufferId, EditMode};

/// Cursor coordinates relative to the editor's visible text area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorPosition {
    pub row: usize,
    pub column: usize,
}

/// Filename-only buffer summary used by tabs and other buffer lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BufferInfo {
    pub id: BufferId,
    pub name: String,
    pub modified: bool,
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
    pub modified: bool,
    pub lines: Vec<String>,
    pub horizontal_offset: usize,
    pub cursor: Option<CursorPosition>,
}
