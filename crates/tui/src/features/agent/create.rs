use std::path::Path;

use editor::{DocumentFactory, EditorSession};
use eggplant_agent::{ToolApproval, ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use super::{tools, write::WriteOutcome};
use crate::filesystem::{self, CreateTarget};

pub const TOOL_NAME: &str = "create";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateArguments {
    path: String,
    content: String,
}

pub fn prepare(call: &ToolCall, root: &Path) -> Result<ToolApproval, ToolResult> {
    let arguments: CreateArguments = tools::parse_arguments(call, TOOL_NAME)?;
    let target = target(call, root, &arguments.path)?;
    let relative = target.relative().to_path_buf();
    Ok(ToolApproval {
        call: call.clone(),
        summary: format!("Create new file {}", relative.display()),
        diff: super::write::bounded_diff(&relative.to_string_lossy(), "", &arguments.content),
    })
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
    super::write::rejected(call)
}

fn apply_result(
    editor: &mut EditorSession,
    root: &Path,
    call: &ToolCall,
) -> Result<(ToolResult, editor::BufferId), ToolResult> {
    let arguments: CreateArguments = tools::parse_arguments(call, TOOL_NAME)?;
    let target = target(call, root, &arguments.path)?;
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

fn target(call: &ToolCall, root: &Path, path: &str) -> Result<CreateTarget, ToolResult> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data_object::Model, manager::effect::Effect};

    fn call(path: &str, content: &str) -> ToolCall {
        ToolCall {
            id: "create-1".to_string(),
            name: TOOL_NAME.to_string(),
            arguments: serde_json::json!({"path": path, "content": content}).to_string(),
        }
    }

    #[tokio::test]
    async fn creates_buffer_and_persists_new_file_through_normal_save() {
        let directory = tempfile::tempdir().unwrap();
        let call = call("nested/numbers.py", "print('numbers')\n");
        let approval = prepare(&call, directory.path()).unwrap();
        assert!(approval.summary.contains("nested/numbers.py"));
        assert!(approval.diff.contains("+print('numbers')"));
        let mut model = Model::default();

        let outcome = apply(&mut model.editor, directory.path(), &call);
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
        assert!(prepare(&call, directory.path()).is_err());
        let mut model = Model::default();

        let outcome = apply(&mut model.editor, directory.path(), &call);

        assert_eq!(outcome.result.status, ToolResultStatus::Error);
        assert!(outcome.edited.is_none());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    }

    #[test]
    fn rejection_creates_no_file() {
        let directory = tempfile::tempdir().unwrap();
        let outcome = rejected(&call("numbers.py", "print('numbers')"));

        assert_eq!(outcome.result.status, ToolResultStatus::Rejected);
        assert!(outcome.edited.is_none());
        assert!(!directory.path().join("numbers.py").exists());
    }
}
