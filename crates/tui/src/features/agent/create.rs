use std::path::Path;

use editor::{DocumentFactory, EditorSession};
use eggplant_agent::{ToolApproval, ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::{
    tools::{self, ToolContext},
    write::{DiffPreview, MutationTool, WriteOutcome},
};
use crate::filesystem::{self, CreateTarget};

pub struct FileCreation {
    diff: DiffPreview,
}

impl FileCreation {
    pub const fn new() -> Self {
        Self {
            diff: DiffPreview::new(),
        }
    }

    fn apply_result(
        &self,
        editor: &mut EditorSession,
        root: &Path,
        call: &ToolCall,
    ) -> Result<(ToolResult, editor::BufferId), ToolResult> {
        let arguments: CreateArguments = tools::parse_arguments(call, "create")?;
        let target = self.target(call, root, &arguments.path)?;
        let relative = target.relative().to_path_buf();
        let path = filesystem::create(root, &target).map_err(|error| {
            tools::error(
                call,
                format!("failed to create `{}`: {error}", relative.display()),
            )
        })?;
        let document = DocumentFactory::new()
            .open(&path)
            .map_err(|error| tools::error(call, error.to_string()))?;
        let created = editor
            .create_file_document(document, &arguments.content)
            .map_err(|error| tools::error(call, error.to_string()))?;
        Ok((
            ToolResult {
                call_id: call.id.clone(),
                status: ToolResultStatus::Success,
                content: format!(
                    "Created {}; revision is now {}. Automatic save and language synchronization are handled separately; the file is not claimed to be saved.",
                    relative.display(),
                    created.revision
                ),
                truncated: false,
            },
            created.buffer_id,
        ))
    }

    fn target(&self, call: &ToolCall, root: &Path, path: &str) -> Result<CreateTarget, ToolResult> {
        let target = CreateTarget::parse(path).map_err(|error| tools::error(call, error))?;
        if target.is_directory() {
            return Err(tools::error(
                call,
                "create requires a file path, not a directory".to_string(),
            ));
        }
        let path = root.join(target.relative());
        match std::fs::symlink_metadata(&path) {
            Ok(_) => Err(tools::error(
                call,
                format!("create target already exists: {}", path.display()),
            )),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(target),
            Err(error) => Err(tools::error(call, error.to_string())),
        }
    }
}

impl MutationTool for FileCreation {
    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<ToolApproval, ToolResult> {
        let arguments: CreateArguments = tools::parse_arguments(call, "create")?;
        let target = self.target(call, &context.root, &arguments.path)?;
        let relative = target.relative().to_path_buf();
        Ok(ToolApproval {
            call: call.clone(),
            summary: format!("Create new file {}", relative.display()),
            diff: self
                .diff
                .render(&relative.to_string_lossy(), "", &arguments.content),
        })
    }

    fn apply(&self, editor: &mut EditorSession, root: &Path, call: &ToolCall) -> WriteOutcome {
        match self.apply_result(editor, root, call) {
            Ok((result, buffer_id)) => WriteOutcome {
                result,
                edited: Some(buffer_id),
            },
            Err(result) => WriteOutcome {
                result,
                edited: None,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArguments {
    path: String,
    content: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data_object::Model, manager::effect::Effect};

    fn call(path: &str, content: &str) -> ToolCall {
        ToolCall {
            id: "create-1".to_string(),
            name: "create".to_string(),
            arguments: serde_json::json!({"path": path, "content": content}).to_string(),
        }
    }

    fn context(directory: &tempfile::TempDir) -> ToolContext {
        ToolContext {
            root: directory.path().to_path_buf(),
            buffers: Vec::new(),
        }
    }

    #[tokio::test]
    async fn creates_buffer_and_persists_new_file_through_normal_save() {
        let directory = tempfile::tempdir().unwrap();
        let call = call("nested/numbers.py", "print('numbers')\n");
        let tool = FileCreation::new();
        let approval = tool.prepare(&call, &context(&directory)).unwrap();
        assert!(approval.summary.contains("nested/numbers.py"));
        assert!(approval.diff.contains("+print('numbers')"));
        let mut model = Model::default();

        let outcome = tool.apply(&mut model.editor, directory.path(), &call);
        let buffer_id = outcome.edited.unwrap();
        assert_eq!(outcome.result.status, ToolResultStatus::Success);
        assert_eq!(
            model.editor.buffer_text_snapshot(buffer_id).unwrap().text,
            "print('numbers')\n"
        );
        let effect = crate::buffer::save_if_dirty(&mut model, buffer_id).unwrap();
        let Effect::Save(request) = effect else {
            panic!("expected save effect");
        };
        let completion = request.perform().await;
        model.editor.complete_save(completion).unwrap();

        assert_eq!(
            std::fs::read_to_string(directory.path().join("nested/numbers.py")).unwrap(),
            "print('numbers')\n"
        );
    }

    #[test]
    fn create_rejects_existing_file_without_overwriting_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "original").unwrap();
        let call = call("main.rs", "replacement");
        let tool = FileCreation::new();
        assert!(tool.prepare(&call, &context(&directory)).is_err());
        let mut model = Model::default();

        let outcome = tool.apply(&mut model.editor, directory.path(), &call);

        assert_eq!(outcome.result.status, ToolResultStatus::Error);
        assert!(outcome.edited.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn rejection_creates_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let tool = FileCreation::new();
        let outcome = tool.reject(&call("numbers.py", "print('numbers')"));

        assert_eq!(outcome.result.status, ToolResultStatus::Rejected);
        assert!(outcome.edited.is_none());
        assert!(!directory.path().join("numbers.py").exists());
    }
}
