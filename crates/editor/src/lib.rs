//! Eggplant's editor core and Helix adapter.

mod document;
mod save;
mod session;
mod snapshot;

pub use document::{DocumentFactory, OpenDocumentError};
pub use save::{SaveCompletion, SaveError, SaveRequest};
pub use session::{BufferId, CursorMovement, EditMode, EditorError, EditorSession};
pub use snapshot::{BufferInfo, CursorPosition, DirtyState, EditorSnapshot};

#[cfg(test)]
mod spike_tests {
    use std::{path::Path, sync::Arc};

    use arc_swap::ArcSwap;
    use helix_core::{Rope, Selection, Transaction, syntax};
    use helix_view::{
        DocumentId, View,
        document::Document,
        editor::{Config, GutterConfig},
        graphics::Rect,
    };

    fn document(text: &str) -> Document {
        Document::from(
            Rope::from(text),
            None,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        )
    }

    #[test]
    fn standalone_document_supports_edit_and_undo() {
        let mut document = document("hello");
        let mut view = View::new(document.id(), GutterConfig::default());
        document.ensure_view_init(view.id);
        document.set_selection(view.id, Selection::point(5));

        let transaction =
            Transaction::insert(document.text(), document.selection(view.id), "!".into());
        assert!(document.apply(&transaction, view.id));
        document.append_changes_to_history(&mut view);

        assert_eq!(document.text(), "hello!");
        assert!(document.is_modified());
        assert!(document.undo(&mut view));
        assert_eq!(document.text(), "hello");
    }

    #[test]
    fn standalone_documents_do_not_have_distinct_ids() {
        let first = document("first");
        let second = document("second");

        // Editor::new_document normally assigns IDs. Eggplant must therefore own
        // distinct BufferIds when using standalone Helix documents.
        assert_eq!(first.id(), DocumentId::default());
        assert_eq!(first.id(), second.id());
    }

    #[test]
    fn view_updates_scroll_to_keep_cursor_visible() {
        let text = (0..=8)
            .map(|line| line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut document = document(&text);
        let mut view = View::new(document.id(), GutterConfig::default());
        view.area = Rect::new(0, 0, 20, 4);
        document.ensure_view_init(view.id);

        let cursor = document.text().line_to_char(6);
        document.set_selection(view.id, Selection::point(cursor));
        view.ensure_cursor_in_view(&mut document, 0);

        assert!(document.view_offset(view.id).anchor > 0);
        assert!(view.is_cursor_in_view(&document, 0));
    }

    #[tokio::test]
    async fn standalone_document_supports_open_edit_and_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("note.txt");
        std::fs::write(&path, "hello").unwrap();

        let mut document = Document::open(
            &path,
            None,
            false,
            Arc::new(ArcSwap::new(Arc::new(Config::default()))),
            Arc::new(ArcSwap::from_pointee(syntax::Loader::default())),
        )
        .unwrap();
        let mut view = View::new(document.id(), GutterConfig::default());
        document.ensure_view_init(view.id);
        document.set_selection(view.id, Selection::point(5));

        let transaction = Transaction::insert(
            document.text(),
            document.selection(view.id),
            ", world".into(),
        );
        assert!(document.apply(&transaction, view.id));
        document.append_changes_to_history(&mut view);
        assert!(document.is_modified());

        let saved = document.save(None::<&Path>, false).unwrap().await.unwrap();
        document.set_last_saved_revision(saved.revision, saved.save_time);

        assert_eq!(std::fs::read_to_string(path).unwrap(), "hello, world");
        assert!(!document.is_modified());
    }
}
