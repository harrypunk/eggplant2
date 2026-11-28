use eggplant_agent::{ToolCall, ToolDescriptor, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::{
    host::{AgentHost, CreateTextFile},
    registry::{AgentTool, MainThreadTool, PreparedTool, ToolOutcome},
    tools::{self, ToolContext},
};
use crate::filesystem::CreateTarget;

#[derive(Debug)]
pub struct FileCreation {
    descriptor: ToolDescriptor,
}

impl FileCreation {
    pub fn new() -> Self {
        Self {
            descriptor: ToolDescriptor::new(
                "create",
                "Create one new text file without overwriting an existing path. Read the workspace first. This must be the only tool call in its round.",
                serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "path": {"type": "string", "description": "Workspace-relative path for the new file"},
                        "content": {"type": "string", "description": "Complete UTF-8 text content for the new file"}
                    },
                    "required": ["path", "content"]
                }),
            )
            .exclusive(),
        }
    }

    fn target(
        &self,
        call: &ToolCall,
        root: &std::path::Path,
        path: &str,
    ) -> Result<CreateTarget, ToolResult> {
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

impl AgentTool for FileCreation {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<PreparedTool, ToolResult> {
        let arguments: CreateArguments = tools::parse_arguments(call, "create")?;
        let target = self.target(call, &context.root, &arguments.path)?;
        let relative = target.relative().to_path_buf();
        Ok(PreparedTool::MainThread(Box::new(PreparedFileCreation {
            call: call.clone(),
            relative,
            request: CreateTextFile::new(context.root.clone(), target, arguments.content),
        })))
    }
}

#[derive(Debug)]
struct PreparedFileCreation {
    call: ToolCall,
    relative: std::path::PathBuf,
    request: CreateTextFile,
}

impl PreparedFileCreation {
    fn execute_result(self, host: &mut dyn AgentHost) -> Result<ToolOutcome, ToolResult> {
        let change = host
            .create_text_file(self.request)
            .map_err(|error| tools::error(&self.call, error.to_string()))?;
        Ok(ToolOutcome::edited(
            ToolResult {
                call_id: self.call.id,
                status: ToolResultStatus::Success,
                content: format!(
                    "Created {}; revision is now {}. Automatic save and language synchronization are handled separately; the file is not claimed to be saved.",
                    self.relative.display(),
                    change.revision
                ),
                truncated: false,
            },
            change,
        ))
    }
}

impl MainThreadTool for PreparedFileCreation {
    fn execute(self: Box<Self>, host: &mut dyn AgentHost) -> ToolOutcome {
        self.execute_result(host)
            .unwrap_or_else(ToolOutcome::immediate)
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
        let prepared = tool.prepare(&call, &context(&directory)).unwrap();
        assert!(!directory.path().join("nested/numbers.py").exists());
        let PreparedTool::MainThread(operation) = prepared else {
            panic!("expected main-thread creation");
        };
        let mut model = Model::default();

        let outcome = operation.execute(&mut model);
        let buffer_id = outcome.change.unwrap().buffer_id;
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
    fn execution_rejects_a_file_created_after_preparation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        let call = call("main.rs", "replacement");
        let tool = FileCreation::new();
        let prepared = tool.prepare(&call, &context(&directory)).unwrap();
        std::fs::write(&path, "original").unwrap();
        let PreparedTool::MainThread(operation) = prepared else {
            panic!("expected main-thread creation");
        };
        let mut model = Model::default();

        let outcome = operation.execute(&mut model);

        assert_eq!(outcome.result.status, ToolResultStatus::Error);
        assert!(outcome.change.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn preparation_creates_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let tool = FileCreation::new();
        tool.prepare(
            &call("numbers.py", "print('numbers')"),
            &context(&directory),
        )
        .unwrap();

        assert!(!directory.path().join("numbers.py").exists());
    }
}
