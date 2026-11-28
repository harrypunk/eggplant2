use std::path::{Path, PathBuf};

use editor::{BufferId, EditorSession, ExactTextReplacement};
use eggplant_agent::{ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::tools::{self, BufferContext, ToolContext};

pub struct MutationOutcome {
    pub result: ToolResult,
    pub edited: Option<BufferId>,
}

pub trait MutationTool {
    fn validate(&self, call: &ToolCall, context: &ToolContext) -> Result<(), ToolResult>;

    fn apply(&self, editor: &mut EditorSession, root: &Path, call: &ToolCall) -> MutationOutcome;
}

#[derive(Debug, Clone, Copy)]
pub enum MutationKind {
    ExactReplacement,
    FileCreation,
}

impl MutationKind {
    pub fn from_call(call: &ToolCall) -> Option<Self> {
        match call.name.as_str() {
            "write" => Some(Self::ExactReplacement),
            "create" => Some(Self::FileCreation),
            _ => None,
        }
    }

    fn tool(self) -> &'static dyn MutationTool {
        static EXACT_REPLACEMENT: ExactReplacement = ExactReplacement::new();
        static FILE_CREATION: super::create::FileCreation = super::create::FileCreation::new();
        match self {
            Self::ExactReplacement => &EXACT_REPLACEMENT,
            Self::FileCreation => &FILE_CREATION,
        }
    }

    pub fn validate(self, call: &ToolCall, context: &ToolContext) -> Result<(), ToolResult> {
        self.tool().validate(call, context)
    }

    pub fn apply(
        self,
        editor: &mut EditorSession,
        root: &Path,
        call: &ToolCall,
    ) -> MutationOutcome {
        self.tool().apply(editor, root, call)
    }
}

pub struct ExactReplacement;

impl ExactReplacement {
    pub const fn new() -> Self {
        Self
    }

    fn apply_result(
        &self,
        editor: &mut EditorSession,
        root: &Path,
        call: &ToolCall,
    ) -> Result<(ToolResult, BufferId), ToolResult> {
        let arguments: WriteArguments = tools::parse_arguments(call, "write")?;
        Self::validate_replacement(call, &arguments)?;
        let path =
            tools::resolve(root, &arguments.path).map_err(|message| tools::error(call, message))?;
        let buffer_id = BufferId::from_raw(arguments.buffer_id);
        let applied = editor
            .replace_exact_text(ExactTextReplacement {
                buffer_id,
                path: &path,
                expected_revision: arguments.expected_revision,
                old_text: &arguments.old_text,
                new_text: &arguments.new_text,
            })
            .map_err(|error| tools::error(call, error.to_string()))?;
        let relative = tools::display_path(root, &path);
        Ok((
            ToolResult {
                call_id: call.id.clone(),
                status: ToolResultStatus::Success,
                content: format!(
                    "Applied exact replacement to {relative}; revision is now {}. Automatic save and language synchronization are handled separately; the file is not claimed to be saved.",
                    applied.revision
                )
                .chars()
                .take(512)
                .collect(),
                truncated: false,
            },
            buffer_id,
        ))
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
        let matches = text.match_indices(old_text).take(2).collect::<Vec<_>>();
        matches.len() == 1
    }
}

impl MutationTool for ExactReplacement {
    fn validate(&self, call: &ToolCall, context: &ToolContext) -> Result<(), ToolResult> {
        let arguments: WriteArguments = tools::parse_arguments(call, "write")?;
        let (_, buffer) = self.captured_target(call, context, &arguments)?;
        Self::unique_match(&buffer.snapshot.text, &arguments.old_text)
            .then_some(())
            .ok_or_else(|| {
                tools::error(
                    call,
                    "write requires exactly one old-text match in the captured buffer".to_string(),
                )
            })
    }

    fn apply(&self, editor: &mut EditorSession, root: &Path, call: &ToolCall) -> MutationOutcome {
        match self.apply_result(editor, root, call) {
            Ok((result, buffer_id)) => MutationOutcome {
                result,
                edited: Some(buffer_id),
            },
            Err(result) => MutationOutcome {
                result,
                edited: None,
            },
        }
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
    fn validation_is_immutable_and_requires_a_unique_match() {
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
        tool.validate(
            &call(buffer_id, 2, "before", "after"),
            &context(&directory, buffer.clone()),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(directory.path().join("main.rs")).unwrap(),
            "before"
        );

        let mut duplicate = buffer;
        duplicate.snapshot.text = "before before".to_string();
        assert!(
            tool.validate(
                &call(buffer_id, 2, "before", "after"),
                &context(&directory, duplicate),
            )
            .is_err()
        );
    }
}
