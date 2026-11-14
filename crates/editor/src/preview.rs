use std::path::Path;

use crate::{
    document::{DocumentFactory, OpenDocumentError},
    snapshot::{TextViewSnapshot, text_view_snapshot_at},
};

/// Open an independent document/view and return its read-only text projection.
///
/// The Helix values are dropped after snapshotting; previews never become
/// editor buffers and expose no editing or save operations.
pub fn preview_document(
    factory: &DocumentFactory,
    path: &Path,
    _width: u16,
    height: u16,
) -> Result<TextViewSnapshot, OpenDocumentError> {
    preview_document_at(factory, path, 0, height.into())
}

/// Project a syntax-highlighted document range centered on a zero-based line.
pub fn preview_document_at(
    factory: &DocumentFactory,
    path: &Path,
    line: usize,
    height: usize,
) -> Result<TextViewSnapshot, OpenDocumentError> {
    let document = factory.open(path)?;
    let max_first_line = document.text().len_lines().saturating_sub(height.max(1));
    let first_line = line.saturating_sub(height / 2).min(max_first_line);
    Ok(text_view_snapshot_at(&document, first_line, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projects_a_read_only_document_viewport() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "zero\none\ntwo\nthree").unwrap();

        let snapshot = preview_document(&DocumentFactory::new(), &path, 20, 3).unwrap();

        assert_eq!(snapshot.first_line, 0);
        assert_eq!(snapshot.line_count, 4);
        assert_eq!(snapshot.lines.len(), 3);
        assert_eq!(snapshot.lines[0].text, "zero");
        assert_eq!(snapshot.lines[1].text, "one");
        assert_eq!(snapshot.horizontal_offset, 0);

        let empty = preview_document(&DocumentFactory::new(), &path, 0, 0).unwrap();
        assert!(empty.lines.is_empty());
    }

    #[test]
    fn centers_preview_lines_and_retains_syntax() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        let text = (0..100)
            .map(|line| format!("fn function_{line}() {{}}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, text).unwrap();

        let snapshot = preview_document_at(&DocumentFactory::new(), &path, 50, 34).unwrap();

        assert_eq!(snapshot.first_line, 33);
        assert_eq!(snapshot.lines.len(), 34);
        assert_eq!(snapshot.lines[17].text, "fn function_50() {}");
        assert!(
            snapshot.lines[17]
                .highlights
                .iter()
                .any(|span| !span.highlights.is_empty())
        );
    }
}
