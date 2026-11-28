use std::{
    ops::ControlFlow::{Break, Continue},
    path::{Path, PathBuf},
};

use editor::{BufferId, EditorSession, ExactTextReplacement};
use eggplant_agent::{ToolApproval, ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::tools::{self, BufferContext, ToolContext};

const MAX_DIFF_BYTES: usize = 12 * 1024;
const MAX_DIFF_LINES: usize = 120;
const TRUNCATION_MARKER: &str = "[diff truncated]";

pub const TOOL_NAME: &str = "write";

pub(super) fn is_mutation(call: &ToolCall) -> bool {
    matches!(call.name.as_str(), TOOL_NAME | super::create::TOOL_NAME)
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

pub fn prepare(call: &ToolCall, context: &ToolContext) -> Result<ToolApproval, ToolResult> {
    let arguments: WriteArguments = tools::parse_arguments(call, "write")?;
    let (path, buffer) = captured_target(call, context, &arguments)?;
    unique_match(&buffer.snapshot.text, &arguments.old_text)
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
        diff: bounded_diff(&relative, &arguments.old_text, &arguments.new_text),
    })
}

pub struct WriteOutcome {
    pub result: ToolResult,
    pub edited: Option<BufferId>,
}

pub fn apply(editor: &mut EditorSession, root: &Path, call: &ToolCall) -> WriteOutcome {
    match apply_result(editor, root, call) {
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

pub fn rejected(call: &ToolCall) -> WriteOutcome {
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

fn apply_result(
    editor: &mut EditorSession,
    root: &Path,
    call: &ToolCall,
) -> Result<(ToolResult, BufferId), ToolResult> {
    let arguments: WriteArguments = tools::parse_arguments(call, "write")?;
    validate_replacement(call, &arguments)?;
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
    call: &ToolCall,
    context: &'a ToolContext,
    arguments: &WriteArguments,
) -> Result<(PathBuf, &'a BufferContext), ToolResult> {
    validate_replacement(call, arguments)?;
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
                "write buffer path or revision does not match the captured snapshot".to_string(),
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

pub(super) fn bounded_diff(path: &str, old_text: &str, new_text: &str) -> String {
    let rows = [
        format!("--- {path}"),
        format!("+++ {path}"),
        "@@ exact replacement @@".to_string(),
    ]
    .into_iter()
    .chain(old_text.lines().map(|line| format!("-{line}")))
    .chain(new_text.lines().map(|line| format!("+{line}")))
    .collect::<Vec<_>>();
    let line_truncated = rows.len() > MAX_DIFF_LINES;
    let output = rows
        .into_iter()
        .take(MAX_DIFF_LINES)
        .map(|row| row + "\n")
        .try_fold(String::new(), |content, row| {
            match content.len() + row.len() <= MAX_DIFF_BYTES {
                true => Continue(content + row.as_str()),
                false => Break(content),
            }
        });
    let (content, byte_truncated) = match output {
        Continue(content) => (content, false),
        Break(content) => (content, true),
    };
    match line_truncated || byte_truncated {
        true => content + TRUNCATION_MARKER,
        false => content,
    }
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
        let approval = prepare(
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
            prepare(
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
        let old_text = "é🙂\n".repeat(MAX_DIFF_LINES + 1);
        let buffer = BufferContext {
            path,
            snapshot: BufferTextSnapshot {
                buffer_id,
                revision: 2,
                text: old_text.clone(),
            },
        };
        let approval = prepare(
            &call(buffer_id, 2, &old_text, "new"),
            &context(&directory, buffer),
        )
        .unwrap();

        assert!(approval.diff.len() <= MAX_DIFF_BYTES + TRUNCATION_MARKER.len());
        assert!(approval.diff.ends_with(TRUNCATION_MARKER));
    }
}
