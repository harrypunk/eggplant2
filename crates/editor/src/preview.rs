use std::path::Path;

use helix_view::{View, editor::GutterConfig, graphics::Rect};

use crate::{
    document::{DocumentFactory, OpenDocumentError},
    snapshot::{TextViewSnapshot, text_view_snapshot},
};

/// Open an independent document/view and return its read-only text projection.
///
/// The Helix values are dropped after snapshotting; previews never become
/// editor buffers and expose no editing or save operations.
pub fn preview_document(
    factory: &DocumentFactory,
    path: &Path,
    width: u16,
    height: u16,
) -> Result<TextViewSnapshot, OpenDocumentError> {
    let mut document = factory.open(path)?;
    let mut view = View::new(document.id(), GutterConfig::default());
    view.area = Rect::new(0, 0, width, height);
    document.ensure_view_init(view.id);
    Ok(text_view_snapshot(&document, &view))
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
        assert_eq!(snapshot.lines[0].text, "zero");
        assert_eq!(snapshot.lines[1].text, "one");
        assert_eq!(snapshot.horizontal_offset, 0);
    }
}
