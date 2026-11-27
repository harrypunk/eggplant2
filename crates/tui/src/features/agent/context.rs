use super::{
    EditorContext,
    tools::{BufferContext, ToolContext},
};
use crate::data_object::Model;

/// Capture the active workspace file as optional Agent tool context.
///
/// Agent availability depends on the project, not on an open buffer. External, standalone, and
/// untitled buffers do not provide file context.
pub(crate) fn editor_context(model: &Model) -> Option<EditorContext> {
    let root = model.workspace.as_ref()?.root();
    let buffer = model.editor.active_buffer()?;
    let path = model.editor.buffer_path(buffer)?;
    if !path.starts_with(root) {
        return None;
    }
    let (_, revision) = model.editor.active_revision()?;
    let line = model
        .editor
        .active_position()
        .map(|position| position.line + 1)
        .unwrap_or(1);
    Some(EditorContext {
        buffer,
        path: path.to_path_buf(),
        revision,
        line,
    })
}

/// Capture the workspace and any open-file snapshots available to provider tools.
pub(crate) fn tool_context(model: &Model) -> Option<ToolContext> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    let buffers = model
        .editor
        .file_buffer_ids()
        .into_iter()
        .filter_map(|buffer_id| {
            let path = model.editor.buffer_path(buffer_id)?.to_path_buf();
            let snapshot = model.editor.buffer_text_snapshot(buffer_id)?;
            path.starts_with(&root)
                .then_some(BufferContext { path, snapshot })
        })
        .collect();
    Some(ToolContext { root, buffers })
}
