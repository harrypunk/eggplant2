use serde_json::Value;
use url::Url;

use super::navigation::from_lsp_position;
use crate::language::{Diagnostic, DiagnosticBatch, DiagnosticSeverity, LanguageError};

/// Decode `textDocument/publishDiagnostics` parameters into one complete path batch.
pub fn decode_diagnostics(
    value: &Value,
    read_text: impl FnOnce(&std::path::Path) -> Result<String, LanguageError>,
) -> Result<DiagnosticBatch, LanguageError> {
    let object = value
        .as_object()
        .ok_or_else(|| LanguageError("diagnostics parameters must be an object".to_string()))?;
    let uri = object
        .get("uri")
        .and_then(Value::as_str)
        .ok_or_else(|| LanguageError("diagnostics require a URI".to_string()))?;
    let path = Url::parse(uri)
        .ok()
        .and_then(|uri| uri.to_file_path().ok())
        .ok_or_else(|| LanguageError(format!("unsupported diagnostics URI `{uri}`")))?;
    let version = object
        .get("version")
        .map(|value| {
            let value = value.as_i64().ok_or_else(|| {
                LanguageError("diagnostic version must be an integer".to_string())
            })?;
            i32::try_from(value)
                .map_err(|error| LanguageError(format!("diagnostic version is invalid: {error}")))
        })
        .transpose()?;
    let values = object
        .get("diagnostics")
        .and_then(Value::as_array)
        .ok_or_else(|| LanguageError("diagnostics must be an array".to_string()))?;
    if values.is_empty() {
        return Ok(DiagnosticBatch {
            path,
            version,
            diagnostics: Vec::new(),
        });
    }
    let text = read_text(&path)?;
    let diagnostics = values
        .iter()
        .map(|value| decode_diagnostic(value, &path, &text))
        .collect::<Result<_, _>>()?;
    Ok(DiagnosticBatch {
        path,
        version,
        diagnostics,
    })
}

fn decode_diagnostic(
    value: &Value,
    path: &std::path::Path,
    text: &str,
) -> Result<Diagnostic, LanguageError> {
    let range = value
        .get("range")
        .ok_or_else(|| LanguageError("diagnostic requires a range".to_string()))?;
    let start = range
        .get("start")
        .ok_or_else(|| LanguageError("diagnostic range requires a start".to_string()))?;
    let end = range
        .get("end")
        .ok_or_else(|| LanguageError("diagnostic range requires an end".to_string()))?;
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .ok_or_else(|| LanguageError("diagnostic requires a message".to_string()))?;
    let severity = match value.get("severity").and_then(Value::as_i64) {
        None => DiagnosticSeverity::Information,
        Some(1) => DiagnosticSeverity::Error,
        Some(2) => DiagnosticSeverity::Warning,
        Some(3) => DiagnosticSeverity::Information,
        Some(4) => DiagnosticSeverity::Hint,
        Some(other) => {
            return Err(LanguageError(format!(
                "unknown diagnostic severity `{other}`"
            )));
        }
    };
    Ok(Diagnostic {
        path: path.to_path_buf(),
        start: from_lsp_position(text, start)?,
        end: from_lsp_position(text, end)?,
        severity,
        message: message.to_string(),
        source: value
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn decodes_severity_source_version_and_utf16_ranges() {
        let path = PathBuf::from("/workspace/main.rs");
        let uri = Url::from_file_path(&path).unwrap().to_string();
        let diagnostics = (1..=4)
            .map(|severity| {
                serde_json::json!({
                    "range": {
                        "start": {"line": 0, "character": 1},
                        "end": {"line": 0, "character": 3}
                    },
                    "severity": severity,
                    "message": format!("diagnostic {severity}"),
                    "source": "fake-lsp"
                })
            })
            .collect::<Vec<_>>();
        let value = serde_json::json!({
            "uri": uri,
            "version": 7,
            "diagnostics": diagnostics
        });

        let batch = decode_diagnostics(&value, |requested| {
            assert_eq!(requested, Path::new("/workspace/main.rs"));
            Ok("a🦀b".to_string())
        })
        .unwrap();

        assert_eq!(batch.path, path);
        assert_eq!(batch.version, Some(7));
        assert_eq!(batch.diagnostics.len(), 4);
        assert_eq!(batch.diagnostics[0].severity, DiagnosticSeverity::Error);
        assert_eq!(batch.diagnostics[1].severity, DiagnosticSeverity::Warning);
        assert_eq!(
            batch.diagnostics[2].severity,
            DiagnosticSeverity::Information
        );
        assert_eq!(batch.diagnostics[3].severity, DiagnosticSeverity::Hint);
        assert_eq!(batch.diagnostics[0].start.character, 1);
        assert_eq!(batch.diagnostics[0].end.character, 2);
        assert_eq!(batch.diagnostics[0].source.as_deref(), Some("fake-lsp"));
    }

    #[test]
    fn empty_batches_clear_without_reading_document_text() {
        let path = PathBuf::from("/workspace/main.rs");
        let uri = Url::from_file_path(&path).unwrap().to_string();
        let value = serde_json::json!({"uri": uri, "diagnostics": []});

        let batch = decode_diagnostics(&value, |_| unreachable!()).unwrap();
        assert_eq!(batch.path, path);
        assert_eq!(batch.version, None);
        assert!(batch.diagnostics.is_empty());
    }

    #[test]
    fn rejects_invalid_severity_and_non_file_uris() {
        let path = PathBuf::from("/workspace/main.rs");
        let uri = Url::from_file_path(&path).unwrap().to_string();
        let invalid = serde_json::json!({
            "uri": uri,
            "diagnostics": [{
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 0}
                },
                "severity": 9,
                "message": "unknown"
            }]
        });
        assert!(decode_diagnostics(&invalid, |_| Ok(String::new())).is_err());

        let non_file = serde_json::json!({"uri": "untitled:scheme", "diagnostics": []});
        assert!(decode_diagnostics(&non_file, |_| unreachable!()).is_err());
    }
}
