use editor::DocumentPosition;
use serde_json::Value;
use url::Url;

use crate::language::{LanguageError, NavigationKind, NavigationRequest, SourceLocation};

pub fn navigation_method(kind: NavigationKind) -> &'static str {
    match kind {
        NavigationKind::Definition => "textDocument/definition",
        NavigationKind::References => "textDocument/references",
        NavigationKind::Implementation => "textDocument/implementation",
    }
}

pub fn navigation_params(request: &NavigationRequest, text: &str) -> Result<Value, LanguageError> {
    let uri = Url::from_file_path(&request.path).map_err(|_| {
        LanguageError(format!(
            "cannot create a file URI for `{}`",
            request.path.display()
        ))
    })?;
    let mut params = serde_json::json!({
        "textDocument": {"uri": uri.to_string()},
        "position": to_lsp_position(text, request.position),
    });
    if request.kind == NavigationKind::References {
        params["context"] = serde_json::json!({"includeDeclaration": true});
    }
    Ok(params)
}

pub fn decode_locations(
    value: &Value,
    mut read_text: impl FnMut(&std::path::Path) -> Result<String, LanguageError>,
) -> Result<Vec<SourceLocation>, LanguageError> {
    match value {
        Value::Null => Ok(Vec::new()),
        Value::Array(locations) => locations
            .iter()
            .map(|location| decode_location(location, &mut read_text))
            .collect(),
        location => decode_location(location, &mut read_text).map(|location| vec![location]),
    }
}

fn decode_location(
    value: &Value,
    read_text: &mut impl FnMut(&std::path::Path) -> Result<String, LanguageError>,
) -> Result<SourceLocation, LanguageError> {
    let object = value
        .as_object()
        .ok_or_else(|| LanguageError("location must be an object".to_string()))?;
    let uri = object
        .get("uri")
        .or_else(|| object.get("targetUri"))
        .and_then(Value::as_str)
        .ok_or_else(|| LanguageError("location requires a URI".to_string()))?;
    let path = Url::parse(uri)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
        .ok_or_else(|| LanguageError(format!("unsupported location URI `{uri}`")))?;
    let range = object
        .get("range")
        .or_else(|| object.get("targetSelectionRange"))
        .or_else(|| object.get("targetRange"))
        .ok_or_else(|| LanguageError("location requires a range".to_string()))?;
    let start = range
        .get("start")
        .ok_or_else(|| LanguageError("location range requires a start".to_string()))?;
    let end = range
        .get("end")
        .ok_or_else(|| LanguageError("location range requires an end".to_string()))?;
    let text = read_text(&path)?;
    Ok(SourceLocation {
        path,
        start: from_lsp_position(&text, start)?,
        end: from_lsp_position(&text, end)?,
    })
}

fn to_lsp_position(text: &str, position: DocumentPosition) -> Value {
    let character = line_at(text, position.line)
        .chars()
        .take(position.character)
        .map(char::len_utf16)
        .sum::<usize>();
    serde_json::json!({"line": position.line, "character": character})
}

fn from_lsp_position(text: &str, position: &Value) -> Result<DocumentPosition, LanguageError> {
    let line = number(position, "line")?;
    let target = number(position, "character")?;
    let mut utf16 = 0;
    let mut character = 0;
    for current in line_at(text, line).chars() {
        if utf16 + current.len_utf16() > target {
            break;
        }
        utf16 += current.len_utf16();
        character += 1;
    }
    Ok(DocumentPosition { line, character })
}

fn number(value: &Value, field: &str) -> Result<usize, LanguageError> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| LanguageError(format!("position requires a non-negative `{field}`")))
}

fn line_at(text: &str, line: usize) -> &str {
    let line = text.split('\n').nth(line).unwrap_or("");
    line.strip_suffix('\r').unwrap_or(line)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use editor::{BufferId, DocumentPosition, EditorSession};

    use super::*;
    use crate::language::NavigationRequestId;

    fn buffer() -> BufferId {
        EditorSession::new().insert_document(editor::DocumentFactory::new().create(""))
    }

    #[test]
    fn builds_reference_params_with_utf16_positions() {
        let path = PathBuf::from("/workspace/🦀.rs");
        let params = navigation_params(
            &NavigationRequest {
                id: NavigationRequestId(1),
                kind: NavigationKind::References,
                buffer: buffer(),
                path: path.clone(),
                revision: 4,
                position: DocumentPosition {
                    line: 0,
                    character: 2,
                },
            },
            "a🦀b",
        )
        .unwrap();

        assert_eq!(
            navigation_method(NavigationKind::References),
            "textDocument/references"
        );
        assert_eq!(params["position"]["character"], 3);
        assert_eq!(params["context"]["includeDeclaration"], true);
        let uri = params["textDocument"]["uri"].as_str().unwrap();
        assert_eq!(Url::parse(uri).unwrap().to_file_path().unwrap(), path);
    }

    #[test]
    fn decodes_locations_and_location_links_with_editor_character_offsets() {
        let path = PathBuf::from("/workspace/main.rs");
        let uri = Url::from_file_path(&path).unwrap().to_string();
        let value = serde_json::json!([
            {
                "uri": uri,
                "range": {
                    "start": {"line": 0, "character": 1},
                    "end": {"line": 0, "character": 3}
                }
            },
            {
                "targetUri": uri,
                "targetSelectionRange": {
                    "start": {"line": 0, "character": 3},
                    "end": {"line": 0, "character": 4}
                }
            }
        ]);

        let locations = decode_locations(&value, |requested| {
            assert_eq!(requested, Path::new("/workspace/main.rs"));
            Ok("a🦀b".to_string())
        })
        .unwrap();

        assert_eq!(locations.len(), 2);
        assert_eq!(locations[0].start.character, 1);
        assert_eq!(locations[0].end.character, 2);
        assert_eq!(locations[1].start.character, 2);
        assert_eq!(locations[1].end.character, 3);
    }

    #[test]
    fn accepts_empty_results_and_rejects_non_file_locations() {
        assert!(
            decode_locations(&Value::Null, |_| unreachable!())
                .unwrap()
                .is_empty()
        );
        let value = serde_json::json!({
            "uri": "untitled:scheme",
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": 0, "character": 0}
            }
        });
        assert!(decode_locations(&value, |_| unreachable!()).is_err());
    }
}
