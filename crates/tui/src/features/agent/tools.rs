use std::{
    borrow::Cow,
    ops::ControlFlow::{self, Break, Continue},
    path::{Path, PathBuf},
};

use editor::{BufferTextSnapshot, read_text_file_bounded};
use eggplant_agent::{ToolCall, ToolResult, ToolResultStatus};
use serde::Deserialize;

use crate::search::TextSearcher;

pub const MAX_ARGUMENT_BYTES: usize = 16 * 1024;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_GREP_MATCHES: usize = 50;
const MAX_OUTPUT: usize = 50 * 1024;
const TRUNCATION_MARKER: &str = "\n[truncated]";

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub root: PathBuf,
    pub active_path: Option<PathBuf>,
    pub active_snapshot: Option<BufferTextSnapshot>,
}

pub fn execute(call: &ToolCall, context: &ToolContext) -> ToolResult {
    match call.arguments.len() > MAX_ARGUMENT_BYTES {
        true => error(call, "tool arguments exceed the byte limit".to_string()),
        false => match call.name.as_str() {
            "read" => read(call, context),
            "grep" => grep(call, context),
            _ => error(call, format!("unknown tool `{}`", call.name)),
        },
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArguments {
    path: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

fn read(call: &ToolCall, context: &ToolContext) -> ToolResult {
    read_result(call, context).unwrap_or_else(|result| result)
}

fn read_result(call: &ToolCall, context: &ToolContext) -> Result<ToolResult, ToolResult> {
    let arguments: ReadArguments = parse_arguments(call, "read")?;
    let offset = arguments.offset.unwrap_or(1);
    let limit = arguments.limit.unwrap_or(100);
    (offset > 0 && (1..=200).contains(&limit))
        .then_some(())
        .ok_or_else(|| {
            error(
                call,
                "read offset must be at least 1 and limit must be between 1 and 200".to_string(),
            )
        })?;
    let path = resolve(&context.root, &arguments.path).map_err(|message| error(call, message))?;
    let text = source_text(&path, context).map_err(|message| error(call, message))?;
    let output = text
        .lines()
        .enumerate()
        .skip(offset - 1)
        .take(limit)
        .map(|(index, line)| format!("{}:{}\n", index + 1, line))
        .try_fold(String::new(), |content, row| {
            match content.len() + row.len() <= MAX_OUTPUT {
                true => Continue(content + row.as_str()),
                false => Break(content),
            }
        });
    let (content, output_truncated) = match output {
        Continue(content) => (content, false),
        Break(content) => (content, true),
    };
    let range_truncated = (offset - 1).saturating_add(limit) < text.lines().count();
    Ok(success(call, content, output_truncated || range_truncated))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrepArguments {
    query: String,
    path: Option<String>,
    context: Option<usize>,
}

#[derive(Default)]
struct GrepOutput {
    content: String,
    matched: usize,
    truncated: bool,
}

impl GrepOutput {
    fn with_row(self, row: String) -> ControlFlow<Self, Self> {
        match self.content.len() + row.len() <= MAX_OUTPUT {
            true => Continue(Self {
                content: self.content + row.as_str(),
                ..self
            }),
            false => Break(Self {
                truncated: true,
                ..self
            }),
        }
    }

    fn with_match(self) -> Self {
        Self {
            matched: self.matched + 1,
            ..self
        }
    }

    fn truncated(self) -> Self {
        Self {
            truncated: true,
            ..self
        }
    }

    fn or_no_matches(self) -> Self {
        match self.content.is_empty() && !self.truncated {
            true => Self {
                content: "No matches".to_string(),
                ..self
            },
            false => self,
        }
    }
}

fn grep(call: &ToolCall, context: &ToolContext) -> ToolResult {
    grep_result(call, context).unwrap_or_else(|result| result)
}

fn grep_result(call: &ToolCall, context: &ToolContext) -> Result<ToolResult, ToolResult> {
    let arguments: GrepArguments = parse_arguments(call, "grep")?;
    (!arguments.query.is_empty())
        .then_some(())
        .ok_or_else(|| error(call, "grep requires a non-empty query".to_string()))?;
    let context_lines = arguments.context.unwrap_or(0);
    (context_lines <= 2)
        .then_some(())
        .ok_or_else(|| error(call, "grep context must be between 0 and 2".to_string()))?;
    let searcher = TextSearcher::new(&arguments.query).map_err(|message| error(call, message))?;
    let output = match arguments.path {
        Some(path) => grep_path(call, context, &searcher, &path, context_lines),
        None => grep_workspace(call, context, &searcher, context_lines),
    }?;
    let output = output.or_no_matches();
    Ok(success(call, output.content, output.truncated))
}

fn grep_path(
    call: &ToolCall,
    context: &ToolContext,
    searcher: &TextSearcher,
    path: &str,
    context_lines: usize,
) -> Result<GrepOutput, ToolResult> {
    let path = resolve(&context.root, path).map_err(|message| error(call, message))?;
    let text = source_text(&path, context).map_err(|message| error(call, message))?;
    search_file(
        searcher,
        &display_path(&context.root, &path),
        &text,
        context_lines,
        GrepOutput::default(),
    )
    .map_err(|message| error(call, message))
}

fn grep_workspace(
    call: &ToolCall,
    context: &ToolContext,
    searcher: &TextSearcher,
    context_lines: usize,
) -> Result<GrepOutput, ToolResult> {
    let mut walk = crate::filesystem::workspace_walk(&context.root);
    walk.sort_by_file_path(|left, right| left.cmp(right));
    let output = walk
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.depth() > 0 && entry.file_type().is_some_and(|kind| kind.is_file()))
        .try_fold(GrepOutput::default(), |output, entry| {
            match output.truncated {
                true => Break(Ok(output)),
                false => {
                    let path = entry.path();
                    let next = match source_text(path, context) {
                        Ok(text) => search_file(
                            searcher,
                            &display_path(&context.root, path),
                            &text,
                            context_lines,
                            output,
                        ),
                        Err(_) => Ok(output),
                    };
                    match next {
                        Ok(output) if output.truncated => Break(Ok(output)),
                        Ok(output) => Continue(output),
                        Err(message) => Break(Err(message)),
                    }
                }
            }
        });
    match output {
        Continue(output) => Ok(output),
        Break(output) => output.map_err(|message| error(call, message)),
    }
}

fn search_file(
    searcher: &TextSearcher,
    display: &str,
    text: &str,
    context_lines: usize,
    output: GrepOutput,
) -> Result<GrepOutput, String> {
    let lines = text.lines().collect::<Vec<_>>();
    let output = searcher
        .search(text)?
        .into_iter()
        .try_fold(output, |output, matched_line| {
            match output.matched >= MAX_GREP_MATCHES {
                true => Break(output.truncated()),
                false => {
                    let start = matched_line.line.saturating_sub(context_lines);
                    let end = matched_line
                        .line
                        .saturating_add(context_lines + 1)
                        .min(lines.len());
                    let rows = (start..end)
                        .map(|index| format!("{display}:{}:{}\n", index + 1, lines[index]));
                    match rows
                        .into_iter()
                        .try_fold(output, |output, row| output.with_row(row))
                    {
                        Continue(output) => Continue(output.with_match()),
                        Break(output) => Break(output),
                    }
                }
            }
        });
    Ok(match output {
        Continue(output) | Break(output) => output,
    })
}

fn parse_arguments<T: for<'de> Deserialize<'de>>(
    call: &ToolCall,
    name: &str,
) -> Result<T, ToolResult> {
    serde_json::from_str(&call.arguments).map_err(|_| {
        error(
            call,
            format!("{name} arguments do not match the tool schema"),
        )
    })
}

fn resolve(root: &Path, input: &str) -> Result<PathBuf, String> {
    (!input.is_empty() && !Path::new(input).is_absolute())
        .then_some(())
        .ok_or_else(|| "path must be workspace-relative".to_string())?;
    let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
    let path = std::fs::canonicalize(root.join(input)).map_err(|error| error.to_string())?;
    path.starts_with(&root)
        .then_some(path)
        .ok_or_else(|| "path escapes the workspace".to_string())
}

fn source_text<'a>(path: &Path, context: &'a ToolContext) -> Result<Cow<'a, str>, String> {
    context
        .active_snapshot
        .as_ref()
        .filter(|_| context.active_path.as_deref() == Some(path))
        .map(|snapshot| {
            (snapshot.text.len() as u64 <= MAX_FILE_BYTES)
                .then_some(Cow::Borrowed(snapshot.text.as_str()))
                .ok_or_else(|| format!("text file exceeds the {MAX_FILE_BYTES}-byte limit"))
        })
        .unwrap_or_else(|| {
            read_text_file_bounded(path, MAX_FILE_BYTES)
                .map(Cow::Owned)
                .map_err(|error| error.to_string())
        })
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

pub fn bound_result(result: ToolResult, max_bytes: usize) -> ToolResult {
    match result.content.len() <= max_bytes {
        true => result,
        false => {
            let marker = match max_bytes >= TRUNCATION_MARKER.len() {
                true => TRUNCATION_MARKER,
                false => "",
            };
            let content = truncate_utf8(&result.content, max_bytes - marker.len()) + marker;
            ToolResult {
                content,
                truncated: true,
                ..result
            }
        }
    }
}

fn success(call: &ToolCall, content: String, truncated: bool) -> ToolResult {
    let content = match truncated {
        true => truncate_utf8(&content, MAX_OUTPUT - TRUNCATION_MARKER.len()) + TRUNCATION_MARKER,
        false => content,
    };
    ToolResult {
        call_id: call.id.clone(),
        status: ToolResultStatus::Success,
        content,
        truncated,
    }
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    let boundary = (0..=max_bytes.min(text.len()))
        .rev()
        .find(|boundary| text.is_char_boundary(*boundary))
        .unwrap_or(0);
    text[..boundary].to_string()
}

fn error(call: &ToolCall, message: String) -> ToolResult {
    ToolResult {
        call_id: call.id.clone(),
        status: ToolResultStatus::Error,
        content: message.chars().take(512).collect(),
        truncated: false,
    }
}

#[cfg(test)]
mod tests {
    use crate::data_object::Model;

    use super::*;

    fn call(name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: "call-1".to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
    }

    fn context(directory: &tempfile::TempDir) -> ToolContext {
        ToolContext {
            root: directory.path().to_path_buf(),
            active_path: None,
            active_snapshot: None,
        }
    }

    #[test]
    fn read_is_bounded_and_rejects_workspace_escape() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "one\ntwö🙂\nthree\nfour").unwrap();
        let context = context(&directory);
        let result = execute(
            &call("read", r#"{"path":"main.rs","offset":2,"limit":2}"#),
            &context,
        );
        assert_eq!(result.content, "2:twö🙂\n3:three\n\n[truncated]");
        assert!(result.truncated);

        [
            r#"{"path":"/etc/passwd"}"#,
            r#"{"path":"../outside"}"#,
            r#"{"path":"main.rs","offset":0}"#,
            r#"{"path":"main.rs","limit":201}"#,
        ]
        .into_iter()
        .map(|arguments| execute(&call("read", arguments), &context))
        .for_each(|result| assert_eq!(result.status, ToolResultStatus::Error));
    }

    #[cfg(unix)]
    #[test]
    fn read_rejects_symlink_escape() {
        let workspace = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "secret").unwrap();
        std::os::unix::fs::symlink(&secret, workspace.path().join("link.txt")).unwrap();

        let result = execute(
            &call("read", r#"{"path":"link.txt"}"#),
            &context(&workspace),
        );
        assert_eq!(result.status, ToolResultStatus::Error);
        assert!(!result.content.contains("secret"));
    }

    #[test]
    fn read_and_grep_can_use_the_current_buffer_snapshot() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "hello").unwrap();
        let mut model = Model::default();
        let buffer = crate::test_utils::create_buffer(&mut model, "hello");
        let context = ToolContext {
            root: directory.path().to_path_buf(),
            active_path: Some(path.clone()),
            active_snapshot: Some(BufferTextSnapshot {
                buffer_id: buffer,
                revision: 2,
                text: "changed HELLO".to_string(),
            }),
        };
        let result = execute(&call("read", r#"{"path":"main.rs"}"#), &context);
        assert!(result.content.contains("changed HELLO"));

        let result = execute(
            &call("grep", r#"{"query":"changed","path":"main.rs"}"#),
            &context,
        );
        assert!(result.content.contains("main.rs:1:changed HELLO"));
    }

    #[test]
    fn grep_is_sorted_smart_case_bounded_and_explicitly_truncated() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("b.txt"), "needle\n").unwrap();
        std::fs::write(directory.path().join("a.txt"), "NEEDLE\n").unwrap();
        let result = execute(&call("grep", r#"{"query":"needle"}"#), &context(&directory));
        assert_eq!(result.status, ToolResultStatus::Success);
        assert!(result.content.starts_with("a.txt:1:NEEDLE\n"));
        assert!(result.content.contains("b.txt:1:needle\n"));

        std::fs::write(directory.path().join("many.txt"), "needle\n".repeat(60)).unwrap();
        let result = execute(
            &call("grep", r#"{"query":"needle","path":"many.txt"}"#),
            &context(&directory),
        );
        assert!(result.truncated);
        assert!(result.content.ends_with("[truncated]"));
        assert_eq!(
            result.content.matches("many.txt:").count(),
            MAX_GREP_MATCHES
        );
    }

    #[test]
    fn invalid_arguments_regexes_and_large_files_are_tool_errors() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "hello").unwrap();
        let large = directory.path().join("large.txt");
        std::fs::write(&large, vec![b'a'; MAX_FILE_BYTES as usize + 1]).unwrap();
        [
            ("read", "not-json"),
            ("read", r#"{"path":"large.txt"}"#),
            ("read", r#"{"path":"main.rs","extra":true}"#),
            ("grep", r#"{"query":"["}"#),
            ("grep", r#"{"query":"x","context":3}"#),
            ("unknown", "{}"),
        ]
        .into_iter()
        .map(|(name, arguments)| execute(&call(name, arguments), &context(&directory)))
        .for_each(|result| assert_eq!(result.status, ToolResultStatus::Error));
    }

    #[test]
    fn cumulative_result_truncation_preserves_utf8() {
        let result = ToolResult {
            call_id: "call-1".to_string(),
            status: ToolResultStatus::Success,
            content: "é🙂".repeat(100),
            truncated: false,
        };
        let result = bound_result(result, 31);
        assert!(result.content.len() <= 31);
        assert!(result.content.is_char_boundary(result.content.len()));
        assert!(result.content.ends_with("[truncated]"));
        assert!(result.truncated);
    }
}
