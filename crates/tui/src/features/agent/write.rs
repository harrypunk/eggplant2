use std::path::PathBuf;

use editor::BufferId;
use eggplant_agent::{ToolCall, ToolDescriptor, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::{
    host::AgentHost,
    registry::{AgentTool, MainThreadTool, PreparedTool, ToolOutcome},
    tools::{self, BufferContext, ToolContext},
};

#[derive(Debug)]
pub struct ExactReplacement {
    descriptor: ToolDescriptor,
}

impl ExactReplacement {
    pub fn new() -> Self {
        Self {
            descriptor: ToolDescriptor::new(
                "write",
                "Apply one exact-text replacement in an open buffer. Read the buffer first for its ID and revision. This must be the only tool call in its round.",
                serde_json::json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": {
                        "buffer_id": {"type": "integer", "minimum": 1},
                        "path": {"type": "string", "description": "Workspace-relative canonical buffer path"},
                        "expected_revision": {"type": "integer"},
                        "old_text": {"type": "string", "minLength": 1},
                        "new_text": {"type": "string"}
                    },
                    "required": ["buffer_id", "path", "expected_revision", "old_text", "new_text"]
                }),
            )
            .exclusive(),
        }
    }

    fn captured_target<'a>(
        &self,
        call: &ToolCall,
        context: &'a ToolContext,
        arguments: &WriteArguments,
    ) -> Result<(PathBuf, &'a BufferContext), ToolResult> {
        Self::validate_replacement(call, arguments)?;
        let path = tools::resolve(&context.root, &arguments.path)
            .map_err(|message| tools::error(call, message))?;
        let buffer = context
            .buffers
            .iter()
            .find(|buffer| buffer.snapshot.buffer_id == BufferId::from_raw(arguments.buffer_id))
            .ok_or_else(|| {
                tools::error(
                    call,
                    "write target is not a captured open buffer".to_string(),
                )
            })?;
        (buffer.path == path && buffer.snapshot.revision == arguments.expected_revision)
            .then_some((path, buffer))
            .ok_or_else(|| {
                tools::error(
                    call,
                    "write buffer path or revision does not match the captured snapshot"
                        .to_string(),
                )
            })
    }

    fn validate_replacement(call: &ToolCall, arguments: &WriteArguments) -> Result<(), ToolResult> {
        (!arguments.old_text.is_empty() && arguments.old_text != arguments.new_text)
            .then_some(())
            .ok_or_else(|| {
                tools::error(
                    call,
                    "write old_text must be non-empty and differ from new_text".to_string(),
                )
            })
    }

    fn unique_match(text: &str, old_text: &str) -> bool {
        text.match_indices(old_text).take(2).count() == 1
    }
}

impl AgentTool for ExactReplacement {
    fn descriptor(&self) -> &ToolDescriptor {
        &self.descriptor
    }

    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<PreparedTool, ToolResult> {
        let arguments: WriteArguments = tools::parse_arguments(call, "write")?;
        let (path, buffer) = self.captured_target(call, context, &arguments)?;
        Self::unique_match(&buffer.snapshot.text, &arguments.old_text)
            .then_some(())
            .ok_or_else(|| {
                tools::error(
                    call,
                    "write requires exactly one old-text match in the captured buffer".to_string(),
                )
            })?;
        let relative = tools::display_path(&context.root, &path);
        Ok(PreparedTool::MainThread(Box::new(
            PreparedExactReplacement {
                call: call.clone(),
                buffer_id: BufferId::from_raw(arguments.buffer_id),
                path,
                relative,
                expected_revision: arguments.expected_revision,
                old_text: arguments.old_text,
                new_text: arguments.new_text,
            },
        )))
    }
}

#[derive(Debug)]
struct PreparedExactReplacement {
    call: ToolCall,
    buffer_id: BufferId,
    path: PathBuf,
    relative: String,
    expected_revision: i32,
    old_text: String,
    new_text: String,
}

impl PreparedExactReplacement {
    fn execute_result(self, host: &mut dyn AgentHost) -> Result<ToolOutcome, ToolResult> {
        let change = host
            .replace_exact_text(editor::ExactTextReplacement {
                buffer_id: self.buffer_id,
                path: &self.path,
                expected_revision: self.expected_revision,
                old_text: &self.old_text,
                new_text: &self.new_text,
            })
            .map_err(|error| tools::error(&self.call, error.to_string()))?;
        let result = ToolResult {
            call_id: self.call.id,
            status: ToolResultStatus::Success,
            content: format!(
                "Applied exact replacement to {}; revision is now {}. Automatic save and language synchronization are handled separately; the file is not claimed to be saved.",
                self.relative, change.revision
            )
            .chars()
            .take(512)
            .collect(),
            truncated: false,
        };
        Ok(ToolOutcome::edited(result, change))
    }
}

impl MainThreadTool for PreparedExactReplacement {
    fn execute(self: Box<Self>, host: &mut dyn AgentHost) -> ToolOutcome {
        self.execute_result(host)
            .unwrap_or_else(ToolOutcome::immediate)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteArguments {
    buffer_id: u64,
    path: String,
    expected_revision: i32,
    old_text: String,
    new_text: String,
}

#[cfg(test)]
mod tests {
    use editor::BufferTextSnapshot;

    use super::*;
    use crate::{data_object::Model, test_utils::create_buffer};

    fn call(buffer_id: BufferId, revision: i32, old_text: &str, new_text: &str) -> ToolCall {
        ToolCall {
            id: "write-1".to_string(),
            name: "write".to_string(),
            arguments: serde_json::json!({
                "buffer_id": buffer_id.as_raw(),
                "path": "main.rs",
                "expected_revision": revision,
                "old_text": old_text,
                "new_text": new_text,
            })
            .to_string(),
        }
    }

    fn context(directory: &tempfile::TempDir, buffer: BufferContext) -> ToolContext {
        ToolContext {
            root: directory.path().to_path_buf(),
            buffers: vec![buffer],
        }
    }

    #[test]
    fn preparation_is_immutable_and_requires_a_unique_match() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "before").unwrap();
        let mut model = Model::default();
        let buffer_id = create_buffer(&mut model, "before");
        let buffer = BufferContext {
            path,
            snapshot: BufferTextSnapshot {
                buffer_id,
                revision: 2,
                text: "before".to_string(),
            },
        };
        let tool = ExactReplacement::new();
        let prepared = tool
            .prepare(
                &call(buffer_id, 2, "before", "after"),
                &context(&directory, buffer.clone()),
            )
            .unwrap();
        assert!(matches!(prepared, PreparedTool::MainThread(_)));
        assert_eq!(
            std::fs::read_to_string(directory.path().join("main.rs")).unwrap(),
            "before"
        );

        let mut duplicate = buffer;
        duplicate.snapshot.text = "before before".to_string();
        assert!(
            tool.prepare(
                &call(buffer_id, 2, "before", "after"),
                &context(&directory, duplicate),
            )
            .is_err()
        );
    }
}
