use std::{
    ops::ControlFlow::{Break, Continue},
    path::{Path, PathBuf},
};

use editor::{BufferId, EditorSession, ExactTextReplacement};
use eggplant_agent::{ToolApproval, ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::tools::{self, BufferContext, ToolContext};

#[derive(Debug, Clone, Copy)]
pub struct DiffPreview {
    max_bytes: usize,
    max_lines: usize,
    truncation_marker: &'static str,
}

impl DiffPreview {
    pub const fn new() -> Self {
        Self {
            max_bytes: 12 * 1024,
            max_lines: 120,
            truncation_marker: "[diff truncated]",
        }
    }

    pub fn render(&self, path: &str, old_text: &str, new_text: &str) -> String {
        let rows = [
            format!("--- {path}"),
            format!("+++ {path}"),
            "@@ exact replacement @@".to_string(),
        ]
        .into_iter()
        .chain(old_text.lines().map(|line| format!("-{line}")))
        .chain(new_text.lines().map(|line| format!("+{line}")))
        .collect::<Vec<_>>();
        let line_truncated = rows.len() > self.max_lines;
        let output = rows
            .into_iter()
            .take(self.max_lines)
            .map(|row| row + "\n")
            .try_fold(String::new(), |content, row| {
                match content.len() + row.len() <= self.max_bytes {
                    true => Continue(content + row.as_str()),
                    false => Break(content),
                }
            });
        let (content, byte_truncated) = match output {
            Continue(content) => (content, false),
            Break(content) => (content, true),
        };
        match line_truncated || byte_truncated {
            true => content + self.truncation_marker,
            false => content,
        }
    }
}

pub struct WriteOutcome {
    pub result: ToolResult,
    pub edited: Option<BufferId>,
}

pub trait MutationTool {
    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<ToolApproval, ToolResult>;

    fn apply(&self, editor: &mut EditorSession, root: &Path, call: &ToolCall) -> WriteOutcome;

    fn reject(&self, call: &ToolCall) -> WriteOutcome {
        WriteOutcome {
            result: ToolResult {
                call_id: call.id.clone(),
                status: ToolResultStatus::Rejected,
                content: "Write rejected by user; no changes were applied.".to_string(),
                truncated: false,
            },
            edited: None,
        }
    }
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

    pub fn prepare(
        self,
        call: &ToolCall,
        context: &ToolContext,
    ) -> Result<ToolApproval, ToolResult> {
        self.tool().prepare(call, context)
    }

    pub fn apply(self, editor: &mut EditorSession, root: &Path, call: &ToolCall) -> WriteOutcome {
        self.tool().apply(editor, root, call)
    }

    pub fn reject(self, call: &ToolCall) -> WriteOutcome {
        self.tool().reject(call)
    }
}

pub struct ExactReplacement {
    diff: DiffPreview,
}

impl ExactReplacement {
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
    fn prepare(&self, call: &ToolCall, context: &ToolContext) -> Result<ToolApproval, ToolResult> {
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
        Ok(ToolApproval {
            call: call.clone(),
            summary: format!(
                "Replace exact text in {relative} (buffer {}, revision {})",
                arguments.buffer_id, arguments.expected_revision
            ),
            diff: self
                .diff
                .render(&relative, &arguments.old_text, &arguments.new_text),
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
    fn preview_is_immutable_bounded_and_requires_a_unique_match() {
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
        let approval = tool
            .prepare(
                &call(buffer_id, 2, "before", "after"),
                &context(&directory, buffer.clone()),
            )
            .unwrap();
        assert!(approval.summary.contains("main.rs"));
        assert!(approval.diff.contains("-before"));
        assert!(approval.diff.contains("+after"));

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

    #[test]
    fn diff_caps_lines_without_splitting_utf8() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "old").unwrap();
        let mut model = Model::default();
        let buffer_id = create_buffer(&mut model, "old");
        let old_text = "é🙂\n".repeat(DiffPreview::new().max_lines + 1);
        let buffer = BufferContext {
            path,
            snapshot: BufferTextSnapshot {
                buffer_id,
                revision: 2,
                text: old_text.clone(),
            },
        };
        let tool = ExactReplacement::new();
        let approval = tool
            .prepare(
                &call(buffer_id, 2, &old_text, "new"),
                &context(&directory, buffer),
            )
            .unwrap();

        let preview = DiffPreview::new();
        assert!(approval.diff.len() <= preview.max_bytes + preview.truncation_marker.len());
        assert!(approval.diff.ends_with(preview.truncation_marker));
    }
}
