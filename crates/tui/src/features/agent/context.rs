use super::{
    AgentContextProvider, EditorContext,
    tools::{BufferContext, ToolContext},
};
use crate::data_object::Model;

impl AgentContextProvider for Model {
    fn agent_context(&self) -> Result<EditorContext, String> {
        let Some(buffer) = self.editor.active_buffer() else {
            return Err("Open a workspace file to use the agent".to_string());
        };
        if self.workspace.is_none()
            || crate::capabilities::readonly::active_buffer_is_read_only(self)
        {
            return Err("Agent is available only for workspace files".to_string());
        }
        let path = self
            .editor
            .buffer_path(buffer)
            .ok_or_else(|| "Open a workspace file to use the agent".to_string())?
            .to_path_buf();
        let (_, revision) = self
            .editor
            .active_revision()
            .ok_or_else(|| "Open a workspace file to use the agent".to_string())?;
        let line = self
            .editor
            .active_position()
            .map(|position| position.line + 1)
            .unwrap_or(1);
        Ok(EditorContext {
            buffer,
            path,
            revision,
            line,
        })
    }
}

/// Capture the workspace and open-file snapshots required by one provider run.
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
