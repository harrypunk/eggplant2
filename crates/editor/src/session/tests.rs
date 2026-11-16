use helix_core::{Selection, Transaction};
use helix_view::graphics::Rect;

use super::*;
use crate::{DocumentFactory, DocumentOpenRequest, LineChange, OpenDocumentError};

fn create_buffer(session: &mut EditorSession, text: &str) -> BufferId {
    session.insert_document(DocumentFactory::new().create(text))
}

fn open_file(session: &mut EditorSession, path: &Path) -> BufferId {
    let document = DocumentFactory::new().open(path).unwrap();
    session.insert_document(document)
}

fn complete_external_reload(
    session: &mut EditorSession,
    id: BufferId,
) -> Result<bool, EditorError> {
    let path = session.buffer_path(id).unwrap().to_path_buf();
    let expected_version = session.buffer_version(id).unwrap();
    let result = DocumentOpenRequest::new(DocumentFactory::new(), &path)
        .perform()
        .into_result();
    session.complete_external_reload(id, expected_version, result)
}

fn active_parts(session: &mut EditorSession) -> (&mut Document, &mut View) {
    session.active_parts().unwrap()
}

fn active_cursor(session: &mut EditorSession) -> usize {
    let (document, view) = active_parts(session);
    document
        .selection(view.id)
        .primary()
        .cursor(document.text().slice(..))
}

fn append_text(session: &mut EditorSession, text: &str) {
    session.enter_insert_mode();
    let (document, view) = active_parts(session);
    let end = document.text().len_chars();
    document.set_selection(view.id, Selection::point(end));
    session.active_editor().unwrap().insert_text(text).unwrap();
    session.enter_normal_mode();
}

#[test]
fn starts_in_empty_state() {
    let mut session = EditorSession::new();
    session.resize_active_view(80, 24);

    assert!(session.is_empty());
    assert!(session.active_snapshot().is_none());
    assert_eq!(session.active_buffer(), None);
    assert_eq!(session.mode(), EditMode::Normal);
}

#[test]
fn creates_distinct_buffers_and_activates_by_id() {
    let mut session = EditorSession::new();
    let first = create_buffer(&mut session, "first");
    let second = create_buffer(&mut session, "second");

    assert_ne!(first, second);
    assert_eq!(session.buffer_count(), 2);
    assert_eq!(session.active_buffer(), Some(second));

    session.activate_buffer(first).unwrap();
    assert_eq!(session.active_buffer(), Some(first));
}

#[test]
fn cycles_buffers_in_both_directions() {
    let mut session = EditorSession::new();
    let first = create_buffer(&mut session, "first");
    let second = create_buffer(&mut session, "second");
    let third = create_buffer(&mut session, "third");
    assert_eq!(session.active_buffer(), Some(third));

    session.activate_next_buffer().unwrap();
    assert_eq!(session.active_buffer(), Some(first));
    session.activate_next_buffer().unwrap();
    assert_eq!(session.active_buffer(), Some(second));
    session.activate_previous_buffer().unwrap();
    assert_eq!(session.active_buffer(), Some(first));
    session.activate_previous_buffer().unwrap();
    assert_eq!(session.active_buffer(), Some(third));
}

#[test]
fn rejects_unknown_buffer() {
    let mut session = EditorSession::new();
    let unknown = BufferId(999);

    assert!(matches!(
        session.activate_buffer(unknown),
        Err(EditorError::UnknownBuffer(id)) if id == unknown
    ));
    assert!(matches!(
        session.close_buffer(unknown),
        Err(EditorError::UnknownBuffer(id)) if id == unknown
    ));
}

#[test]
fn closing_active_buffer_selects_next_or_empty_state() {
    let mut session = EditorSession::new();
    let first = create_buffer(&mut session, "first");
    let second = create_buffer(&mut session, "second");

    session.close_buffer(second).unwrap();
    assert_eq!(session.active_buffer(), Some(first));

    session.close_buffer(first).unwrap();
    assert!(session.is_empty());
    assert_eq!(session.active_buffer(), None);
}

#[test]
fn closing_multiple_buffers_is_atomic() {
    let mut session = EditorSession::new();
    let first = create_buffer(&mut session, "first");
    let second = create_buffer(&mut session, "second");
    let third = create_buffer(&mut session, "third");
    session.activate_buffer(second).unwrap();
    session.close_buffers(&[first, third]).unwrap();
    assert_eq!(session.buffer_count(), 1);
    assert_eq!(session.active_buffer(), Some(second));

    let mut dirty = EditorSession::new();
    let first = create_buffer(&mut dirty, "first");
    let second = create_buffer(&mut dirty, "second");
    dirty.active_editor().unwrap().insert_text("!").unwrap();
    assert!(dirty.close_buffers(&[first, second]).is_err());
    assert_eq!(dirty.buffer_count(), 2);
}

#[test]
fn rejects_closing_modified_buffer() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "hello");
    let (document, view) = active_parts(&mut session);
    document.set_selection(view.id, Selection::point(5));
    let transaction = Transaction::insert(document.text(), document.selection(view.id), "!".into());
    document.apply(&transaction, view.id);

    assert!(matches!(
        session.close_buffer(id),
        Err(EditorError::BufferModified(closed_id)) if closed_id == id
    ));
    assert_eq!(session.active_buffer(), Some(id));
}

#[test]
fn editing_without_active_buffer_is_rejected() {
    let mut session = EditorSession::new();

    assert!(matches!(
        session.active_editor(),
        Err(EditorError::NoActiveBuffer)
    ));
}

#[test]
fn buffer_text_snapshots_are_owned_and_revisioned() {
    let mut session = EditorSession::new();
    assert!(session.active_text_snapshot().is_none());
    let id = create_buffer(&mut session, "hello");
    let first = session.active_text_snapshot().unwrap();
    assert_eq!(first.buffer_id, id);
    assert_eq!(first.text, "hello");

    session.active_editor().unwrap().insert_text("!").unwrap();
    let second = session.active_text_snapshot().unwrap();
    assert_eq!(second.text, "!hello");
    assert_ne!(second.revision, first.revision);
    assert_eq!(session.active_revision(), Some((id, second.revision)));
    assert_eq!(first.text, "hello");
}

#[test]
fn active_previews_center_lines_and_retain_current_syntax() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\nprintln!(\"hello\");").unwrap();
    let mut session = EditorSession::new();
    open_file(&mut session, &path);

    let preview = session.active_preview_at_line(0, 2).unwrap();

    assert_eq!(preview.first_line, 0);
    assert_eq!(preview.lines.len(), 2);
    assert!(
        preview.lines[0].highlights.iter().any(|span| {
            !span.highlights.is_empty() && span.start_byte == 0 && span.end_byte == 2
        })
    );
    assert!(
        session
            .active_preview_at_line(0, 0)
            .unwrap()
            .lines
            .is_empty()
    );
}

#[test]
fn moves_horizontally_within_document_boundaries() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "ab");
    assert_eq!(active_cursor(&mut session), 0);

    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 1);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 2);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 2);

    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Left)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 1);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Left)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Left)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    assert!(!session.active_is_modified());
}

#[test]
fn absolute_cursor_movement_uses_character_offsets_and_document_boundaries() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "hello\na界🙂b\nlast");
    session.resize_active_view(20, 2);
    assert_eq!(session.active_view_width(), Some(20));

    session
        .active_editor()
        .unwrap()
        .move_cursor_to(DocumentPosition {
            line: 1,
            character: 3,
        })
        .unwrap();
    assert_eq!(active_cursor(&mut session), 9);
    let snapshot = session.active_snapshot().unwrap();
    assert!(snapshot.text.first_line <= 1);
    assert!(snapshot.cursor.is_some());

    session
        .active_editor()
        .unwrap()
        .move_cursor_to(DocumentPosition {
            line: 100,
            character: 100,
        })
        .unwrap();
    let document_end = {
        let (document, _) = active_parts(&mut session);
        document.text().len_chars()
    };
    assert_eq!(active_cursor(&mut session), document_end);
    assert!(session.active_snapshot().unwrap().cursor.is_some());
}

#[test]
fn insert_positions_respect_graphemes_and_line_boundaries() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "  hé");
    session.resize_active_view(20, 3);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to(DocumentPosition {
            line: 0,
            character: 2,
        })
        .unwrap();

    session
        .active_editor()
        .unwrap()
        .move_cursor_to_insert(InsertPosition::AfterCursor)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 3);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_insert(InsertPosition::LineEnd)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 4);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_insert(InsertPosition::FirstNonBlank)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 2);
    assert!(!session.active_is_modified());

    let mut grapheme = EditorSession::new();
    create_buffer(&mut grapheme, "👩\u{200d}💻x");
    grapheme
        .active_editor()
        .unwrap()
        .move_cursor_to_insert(InsertPosition::AfterCursor)
        .unwrap();
    assert_eq!(active_cursor(&mut grapheme), 3);
}

#[test]
fn line_boundaries_target_column_zero_and_the_final_grapheme() {
    let mut simple = EditorSession::new();
    create_buffer(&mut simple, " hello");
    simple.resize_active_view(20, 4);
    simple
        .active_editor()
        .unwrap()
        .move_cursor_to_line_boundary(LineBoundary::End)
        .unwrap();
    assert_eq!(active_cursor(&mut simple), 5);
    assert_eq!(simple.active_snapshot().unwrap().cursor.unwrap().column, 5);

    let mut session = EditorSession::new();
    create_buffer(&mut session, "  hé\n\n👩\u{200d}💻x");
    session.resize_active_view(20, 4);

    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line_boundary(LineBoundary::End)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 3);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line_boundary(LineBoundary::Start)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line_boundary(LineBoundary::End)
        .unwrap();
    assert_eq!(session.active_snapshot().unwrap().cursor.unwrap().column, 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(2)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line_boundary(LineBoundary::End)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 9);
    assert!(!session.active_is_modified());

    let mut empty = EditorSession::new();
    create_buffer(&mut empty, "");
    for boundary in [LineBoundary::Start, LineBoundary::End] {
        empty
            .active_editor()
            .unwrap()
            .move_cursor_to_line_boundary(boundary)
            .unwrap();
        assert_eq!(active_cursor(&mut empty), 0);
    }
}

#[test]
fn open_line_preserves_indentation_and_positions_the_new_cursor() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "    first\nsecond");
    session.resize_active_view(20, 4);

    session
        .active_editor()
        .unwrap()
        .open_line(OpenLineDirection::Below)
        .unwrap();
    assert_eq!(session.active_snapshot().unwrap().text.lines[1].text, "\t");
    assert_eq!(session.active_snapshot().unwrap().cursor.unwrap().column, 4);

    let mut above = EditorSession::new();
    create_buffer(&mut above, "first\n    second");
    above.resize_active_view(20, 4);
    above
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    above
        .active_editor()
        .unwrap()
        .open_line(OpenLineDirection::Above)
        .unwrap();
    let snapshot = above.active_snapshot().unwrap();
    assert_eq!(snapshot.text.lines[1].text, "\t");
    assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 1);
    assert_eq!(snapshot.cursor.unwrap().column, 4);

    let mut empty = EditorSession::new();
    create_buffer(&mut empty, "");
    empty.resize_active_view(20, 4);
    empty
        .active_editor()
        .unwrap()
        .open_line(OpenLineDirection::Below)
        .unwrap();
    let snapshot = empty.active_snapshot().unwrap();
    assert_eq!(snapshot.text.line_count, 2);
    assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 1);

    let mut empty_above = EditorSession::new();
    create_buffer(&mut empty_above, "");
    empty_above.resize_active_view(20, 4);
    empty_above
        .active_editor()
        .unwrap()
        .open_line(OpenLineDirection::Above)
        .unwrap();
    let snapshot = empty_above.active_snapshot().unwrap();
    assert_eq!(snapshot.text.line_count, 2);
    assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 0);
}

#[test]
fn line_navigation_targets_first_nonblank_and_clamps_boundaries() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "  first\n\n\tsecond");
    session.resize_active_view(20, 4);

    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(0)
        .unwrap();
    assert_eq!(session.active_snapshot().unwrap().cursor.unwrap().column, 2);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 1);
    assert_eq!(snapshot.cursor.unwrap().column, 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(100)
        .unwrap();
    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.first_line + snapshot.cursor.unwrap().row, 2);
    assert_eq!(snapshot.cursor.unwrap().column, 4);
    assert!(!session.active_is_modified());
}

#[test]
fn cursor_alignment_moves_only_the_viewport() {
    let text = (0..30)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, &text);
    session.resize_active_view(30, 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(10)
        .unwrap();
    let revision = session.buffer_version(id);

    session
        .active_editor()
        .unwrap()
        .align_cursor_line(ViewAlignment::Top)
        .unwrap();
    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.first_line, 10);
    assert_eq!(snapshot.cursor.unwrap().row, 0);
    session
        .active_editor()
        .unwrap()
        .align_cursor_line(ViewAlignment::Bottom)
        .unwrap();
    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.first_line, 5);
    assert_eq!(snapshot.cursor.unwrap().row, 5);
    session
        .active_editor()
        .unwrap()
        .align_cursor_line(ViewAlignment::Center)
        .unwrap();
    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.first_line, 8);
    assert_eq!(snapshot.cursor.unwrap().row, 2);
    assert_eq!(session.buffer_version(id), revision);
    assert!(!session.active_is_modified());

    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(0)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .align_cursor_line(ViewAlignment::Top)
        .unwrap();
    assert_eq!(session.active_snapshot().unwrap().text.first_line, 0);
}

#[test]
fn word_motions_cover_unicode_and_document_boundaries() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "hello world élan test");
    session.resize_active_view(30, 4);

    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::NextEnd)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 4);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::PreviousStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::NextStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::NextStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 12);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::NextEnd)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 15);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::PreviousStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 12);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::PreviousStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::PreviousStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_word(WordMovement::PreviousStart)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    assert!(!session.active_is_modified());

    let mut empty = EditorSession::new();
    create_buffer(&mut empty, "");
    for movement in [
        WordMovement::NextStart,
        WordMovement::PreviousStart,
        WordMovement::NextEnd,
    ] {
        empty
            .active_editor()
            .unwrap()
            .move_cursor_word(movement)
            .unwrap();
        assert_eq!(active_cursor(&mut empty), 0);
    }
}

#[test]
fn vertical_movement_remembers_the_desired_column() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "abcd\nx\nwxyz");
    let (document, view) = active_parts(&mut session);
    document.set_selection(view.id, Selection::point(3));
    assert_eq!(active_cursor(&mut session), 3);

    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    // Helix uses the end-of-line position when the target line is shorter.
    assert_eq!(active_cursor(&mut session), 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 10);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 10);

    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Up)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Up)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 3);
}

#[test]
fn counted_vertical_movement_uses_the_same_boundaries_and_viewport() {
    let text = (0..=10)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = EditorSession::new();
    create_buffer(&mut session, &text);
    session.resize_active_view(20, 4);
    assert_eq!(session.active_view_height(), Some(4));

    session
        .active_editor()
        .unwrap()
        .move_cursor_lines(VerticalDirection::Down, 5)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 10);
    session
        .active_editor()
        .unwrap()
        .move_cursor_lines(VerticalDirection::Up, 2)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 6);
    session
        .active_editor()
        .unwrap()
        .move_cursor_lines(VerticalDirection::Up, 100)
        .unwrap();
    assert_eq!(active_cursor(&mut session), 0);
    session
        .active_editor()
        .unwrap()
        .move_cursor_lines(VerticalDirection::Down, 100)
        .unwrap();
    assert_eq!(active_cursor(&mut session), text.len() - 2);
    assert!(session.active_snapshot().unwrap().cursor.is_some());
}

#[test]
fn movement_scrolls_cursor_into_view() {
    let text = (0..=8)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = EditorSession::new();
    create_buffer(&mut session, &text);
    let (_, view) = active_parts(&mut session);
    view.area = Rect::new(0, 0, 20, 4);

    for _ in 0..6 {
        session
            .active_editor()
            .unwrap()
            .move_cursor(CursorMovement::Down)
            .unwrap();
    }

    let (document, view) = active_parts(&mut session);
    assert!(document.view_offset(view.id).anchor > 0);
    assert!(view.is_cursor_in_view(document, 0));
}

#[test]
fn inserts_text_and_deletes_previous_character() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "hello");
    session.enter_insert_mode();
    let (document, view) = active_parts(&mut session);
    document.set_selection(view.id, Selection::point(5));

    session
        .active_editor()
        .unwrap()
        .insert_text(", world")
        .unwrap();
    session.active_editor().unwrap().backspace().unwrap();

    assert_eq!(session.buffers[&id].document.text(), "hello, worl");
    assert!(session.active_is_modified());
}

#[test]
fn backspace_removes_the_whole_grapheme() {
    for text in ["e\u{301}", "👩\u{200d}💻", "🇫🇮"] {
        let mut session = EditorSession::new();
        let id = create_buffer(&mut session, text);
        let (document, view) = active_parts(&mut session);
        let end = document.text().len_chars();
        document.set_selection(view.id, Selection::point(end));

        session.active_editor().unwrap().backspace().unwrap();
        assert_eq!(session.buffers[&id].document.text(), "");

        session.active_editor().unwrap().backspace().unwrap();
        assert_eq!(session.buffers[&id].document.text(), "");
    }
}

#[test]
fn delete_grapheme_and_visual_selection_use_single_transactions() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "a👩\u{200d}💻b");
    session
        .active_editor()
        .unwrap()
        .move_cursor_to(DocumentPosition {
            line: 0,
            character: 1,
        })
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .delete_grapheme_forward()
        .unwrap();
    assert_eq!(
        session.buffers[&session.active_buffer().unwrap()]
            .document
            .text(),
        "ab"
    );

    let mut visual = EditorSession::new();
    let id = create_buffer(&mut visual, "hello world");
    visual.enter_visual_mode();
    visual
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    visual
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    visual.active_editor().unwrap().delete_selection().unwrap();
    assert_eq!(visual.buffers[&id].document.text(), "lo world");
    assert!(visual.active_editor().unwrap().undo().unwrap());
    assert_eq!(visual.buffers[&id].document.text(), "hello world");
}

#[test]
fn delete_and_change_word_use_the_next_word_start_and_one_undo_step() {
    let mut delete = EditorSession::new();
    let delete_id = create_buffer(&mut delete, "héllo world");
    assert_eq!(
        delete.active_editor().unwrap().current_word_text(),
        "héllo "
    );
    delete.active_editor().unwrap().delete_word().unwrap();
    assert_eq!(delete.buffers[&delete_id].document.text(), "world");
    assert_eq!(active_cursor(&mut delete), 0);
    assert!(delete.active_editor().unwrap().undo().unwrap());
    assert_eq!(delete.buffers[&delete_id].document.text(), "héllo world");

    let mut change = EditorSession::new();
    let change_id = create_buffer(&mut change, "one two");
    change.active_editor().unwrap().change_word().unwrap();
    assert_eq!(change.buffers[&change_id].document.text(), "two");
    change.enter_insert_mode();
    change
        .active_editor()
        .unwrap()
        .insert_text("three ")
        .unwrap();
    change.enter_normal_mode();
    assert_eq!(change.buffers[&change_id].document.text(), "three two");
    assert!(change.active_editor().unwrap().undo().unwrap());
    assert_eq!(change.buffers[&change_id].document.text(), "one two");
}

#[test]
fn word_deletion_handles_final_words_and_empty_or_exhausted_documents() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "first\nlast");
    session.active_editor().unwrap().delete_word().unwrap();
    assert_eq!(session.buffers[&id].document.text(), "\nlast");
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    session.active_editor().unwrap().delete_word().unwrap();
    assert_eq!(session.buffers[&id].document.text(), "\n");
    session.active_editor().unwrap().delete_word().unwrap();
    assert_eq!(session.buffers[&id].document.text(), "\n");

    let mut empty = EditorSession::new();
    let empty_id = create_buffer(&mut empty, "");
    assert_eq!(empty.active_editor().unwrap().current_word_text(), "");
    empty.active_editor().unwrap().change_word().unwrap();
    assert_eq!(empty.buffers[&empty_id].document.text(), "");
    assert!(!empty.active_is_modified());
}

#[test]
fn delete_current_line_handles_first_last_and_only_lines() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "first\nsecond\nthird");
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .delete_current_line()
        .unwrap();
    assert_eq!(session.buffers[&id].document.text(), "first\nthird");
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(1)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .delete_current_line()
        .unwrap();
    assert_eq!(session.buffers[&id].document.text(), "first");
    session
        .active_editor()
        .unwrap()
        .delete_current_line()
        .unwrap();
    assert_eq!(session.buffers[&id].document.text(), "");
    assert!(session.active_editor().unwrap().undo().unwrap());
    assert_eq!(session.buffers[&id].document.text(), "first");
}

#[test]
fn change_current_line_preserves_indentation_and_is_undoable() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "    first\nsecond");
    session.resize_active_view(20, 4);

    session
        .active_editor()
        .unwrap()
        .change_current_line()
        .unwrap();
    assert_eq!(session.buffers[&id].document.text(), "    \nsecond");
    assert_eq!(session.active_snapshot().unwrap().cursor.unwrap().column, 4);
    session.enter_insert_mode();
    session.enter_normal_mode();
    assert!(session.active_editor().unwrap().undo().unwrap());
    assert_eq!(session.buffers[&id].document.text(), "    first\nsecond");
}

#[test]
fn undo_and_redo_restore_an_insert_session() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "a");
    session.enter_insert_mode();
    let (document, view) = active_parts(&mut session);
    document.set_selection(view.id, Selection::point(1));
    session.active_editor().unwrap().insert_text("b").unwrap();
    session.active_editor().unwrap().insert_text("c").unwrap();
    session.enter_normal_mode();

    assert_eq!(session.buffers[&id].document.text(), "abc");
    assert!(session.active_editor().unwrap().undo().unwrap());
    assert_eq!(session.buffers[&id].document.text(), "a");
    assert!(!session.active_is_modified());
    assert!(session.active_editor().unwrap().redo().unwrap());
    assert_eq!(session.buffers[&id].document.text(), "abc");
    assert!(session.active_is_modified());
}

#[tokio::test]
async fn diff_base_projects_line_changes_and_rejects_stale_revisions() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "hello\nworld");
    session.resize_active_view(20, 4);
    let version = session.buffer_version(id).unwrap();
    assert!(
        session
            .set_buffer_diff_base(id, version, b"hello\n".to_vec())
            .unwrap()
    );

    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let snapshot = session.active_snapshot().unwrap();
            if snapshot.text.lines[1].change == Some(LineChange::Modified) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    session.enter_insert_mode();
    session.active_editor().unwrap().insert_text("!").unwrap();
    session.enter_normal_mode();
    assert!(
        !session
            .set_buffer_diff_base(id, version, b"stale".to_vec())
            .unwrap()
    );
}

#[test]
fn visual_selection_extends_reverses_and_collapses_without_editing() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "hello\nworld");
    session.resize_active_view(20, 4);

    session.enter_visual_mode();
    assert_eq!(session.mode(), EditMode::Visual);
    assert_eq!(session.active_editor().unwrap().selected_text(), "h");
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    assert_eq!(session.active_editor().unwrap().selected_text(), "hel");
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Left)
        .unwrap();
    assert_eq!(session.active_editor().unwrap().selected_text(), "he");
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    assert!(
        session
            .active_editor()
            .unwrap()
            .selected_text()
            .contains("hello\n")
    );

    session.enter_normal_mode();
    assert_eq!(session.mode(), EditMode::Normal);
    assert_eq!(session.active_editor().unwrap().selected_text(), "");
    assert!(!session.buffers[&id].document.is_modified());
}

#[test]
fn visual_selection_is_safe_for_empty_or_missing_buffers() {
    let mut session = EditorSession::new();
    session.enter_visual_mode();
    assert_eq!(session.mode(), EditMode::Normal);

    create_buffer(&mut session, "");
    session.resize_active_view(20, 4);
    session.enter_visual_mode();
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Right)
        .unwrap();
    assert_eq!(session.active_editor().unwrap().selected_text(), "");
    session.enter_normal_mode();
}

#[test]
fn visual_line_selection_extends_and_reverses_whole_lines() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "first\nsecond\nthird");
    session.resize_active_view(20, 4);

    session.enter_visual_line_mode();
    assert_eq!(session.mode(), EditMode::VisualLine);
    assert_eq!(session.active_editor().unwrap().selected_text(), "first\n");
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    assert_eq!(
        session.active_editor().unwrap().selected_text(),
        "first\nsecond\n"
    );
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Up)
        .unwrap();
    assert_eq!(session.active_editor().unwrap().selected_text(), "first\n");
    session
        .active_editor()
        .unwrap()
        .move_cursor_to_line(2)
        .unwrap();
    assert_eq!(
        session.active_editor().unwrap().selected_text(),
        "first\nsecond\nthird"
    );

    session.enter_normal_mode();
    assert!(!session.buffers[&id].document.is_modified());
}

#[test]
fn visual_line_selection_is_safe_for_an_empty_document() {
    let mut session = EditorSession::new();
    create_buffer(&mut session, "");
    session.resize_active_view(20, 4);
    session.enter_visual_line_mode();
    session
        .active_editor()
        .unwrap()
        .move_cursor(CursorMovement::Down)
        .unwrap();
    assert_eq!(session.active_editor().unwrap().selected_text(), "");
    session.enter_normal_mode();
}

#[test]
fn first_buffer_uses_size_received_while_empty() {
    let mut session = EditorSession::new();
    session.resize_active_view(20, 4);
    create_buffer(&mut session, "visible");

    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.lines[0].text, "visible");
    assert!(snapshot.cursor.is_some());
}

#[test]
fn snapshot_contains_visible_lines_and_cursor() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "zero\none\ntwo");
    session.resize_active_view(20, 3);

    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.buffer_id, id);
    assert_eq!(snapshot.text.first_line, 0);
    assert_eq!(snapshot.text.line_count, 3);
    assert_eq!(snapshot.text.lines[0].text, "zero");
    assert_eq!(snapshot.text.lines[1].text, "one");
    assert_eq!(snapshot.cursor, Some(CursorPosition { row: 0, column: 0 }));
    assert_eq!(snapshot.text.horizontal_offset, 0);
    assert_eq!(snapshot.mode, EditMode::Normal);
    assert_eq!(snapshot.modified, DirtyState::Clean);
}

#[test]
fn snapshot_follows_the_scrolled_cursor() {
    let text = (0..=8)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = EditorSession::new();
    create_buffer(&mut session, &text);
    session.resize_active_view(20, 4);

    for _ in 0..6 {
        session
            .active_editor()
            .unwrap()
            .move_cursor(CursorMovement::Down)
            .unwrap();
    }

    let snapshot = session.active_snapshot().unwrap();
    let cursor = snapshot.cursor.unwrap();
    assert_eq!(snapshot.text.lines.len(), 4);
    assert!(snapshot.text.lines.iter().any(|line| line.text == "6"));
    assert!(cursor.row < snapshot.text.lines.len());
}

#[test]
fn external_reload_updates_a_clean_buffer_and_keeps_cursor_valid() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "zero\none").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    session.resize_active_view(20, 2);
    session
        .active_editor()
        .unwrap()
        .move_cursor_to(DocumentPosition {
            line: 1,
            character: 1,
        })
        .unwrap();
    std::fs::write(&path, "zero\none changed\nthree").unwrap();

    complete_external_reload(&mut session, id).unwrap();

    assert_eq!(
        session.buffers[&id].document.text(),
        "zero\none changed\nthree"
    );
    assert!(!session.active_is_modified());
    assert!(session.active_snapshot().unwrap().cursor.is_some());
    complete_external_reload(&mut session, id).unwrap();
    assert!(!session.active_is_modified());
}

#[test]
fn external_reload_preserves_dirty_or_deleted_local_buffers() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "original").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    append_text(&mut session, " local");
    std::fs::write(&path, "external").unwrap();

    assert!(matches!(
        complete_external_reload(&mut session, id),
        Err(EditorError::BufferModified(buffer_id)) if buffer_id == id
    ));
    assert_eq!(session.buffers[&id].document.text(), "original local");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");

    let mut clean_session = EditorSession::new();
    let clean_id = open_file(&mut clean_session, &path);
    std::fs::remove_file(&path).unwrap();
    assert!(complete_external_reload(&mut clean_session, clean_id).is_err());
    assert_eq!(clean_session.buffers[&clean_id].document.text(), "external");
    assert_eq!(session.buffers[&id].document.text(), "original local");
}

#[test]
fn stale_external_reload_does_not_overwrite_a_newer_edit() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "original").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    let expected_version = session.buffer_version(id).unwrap();
    session.active_editor().unwrap().insert_text("!").unwrap();
    std::fs::write(&path, "external").unwrap();
    let result = DocumentOpenRequest::new(DocumentFactory::new(), &path)
        .perform()
        .into_result();

    assert!(matches!(
        session.complete_external_reload(id, expected_version, result),
        Ok(false)
    ));
    assert_eq!(session.buffers[&id].document.text(), "!original");
}

#[tokio::test]
async fn buffer_cannot_close_during_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "hello").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    let request = session.save_buffer(id).unwrap();

    assert!(session.has_saving_buffers());
    assert!(matches!(
        session.close_buffer(id),
        Err(EditorError::SaveInProgress(closed_id)) if closed_id == id
    ));

    let completion = request.perform().await;
    session.complete_save(completion).unwrap();
    assert!(!session.has_saving_buffers());
    session.close_buffer(id).unwrap();
    assert!(session.is_empty());
}

#[tokio::test]
async fn saves_buffer_and_clears_its_dirty_state() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "hello").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    append_text(&mut session, ", world");

    let request = session.save_buffer(id).unwrap();
    let completion = request.perform().await;
    session.complete_save(completion).unwrap();

    assert!(!session.active_is_modified());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "hello, world");
}

#[tokio::test]
async fn save_completion_does_not_clear_newer_edits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "hello").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    append_text(&mut session, " one");

    let request = session.save_buffer(id).unwrap();
    append_text(&mut session, " two");
    let completion = request.perform().await;
    session.complete_save(completion).unwrap();

    assert!(session.active_is_modified());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "hello one");
    assert_eq!(session.buffers[&id].document.text(), "hello one two");
}

#[tokio::test]
async fn large_file_viewport_edit_and_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.txt");
    let text = (0..10_000)
        .map(|line| format!("line-{line:05}"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(&path, text).unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    session.resize_active_view(40, 8);
    let (document, view) = active_parts(&mut session);
    let end = document.text().len_chars();
    document.set_selection(view.id, Selection::point(end));
    view.ensure_cursor_in_view(document, 0);

    let snapshot = session.active_snapshot().unwrap();
    assert_eq!(snapshot.text.line_count, 10_000);
    assert!(snapshot.text.first_line > 9_000);
    assert!(snapshot.text.lines.len() <= 8);
    assert!(
        snapshot
            .text
            .lines
            .iter()
            .any(|line| line.text == "line-09999")
    );

    append_text(&mut session, "\ntail");
    let completion = session.save_buffer(id).unwrap().perform().await;
    session.complete_save(completion).unwrap();
    assert!(!session.active_is_modified());
    assert!(std::fs::read_to_string(path).unwrap().ends_with("\ntail"));
}

#[tokio::test]
async fn missing_file_is_created_by_the_first_save() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("new.txt");
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    session.enter_insert_mode();
    session.active_editor().unwrap().insert_text("new").unwrap();
    session.enter_normal_mode();

    let completion = session.save_buffer(id).unwrap().perform().await;
    session.complete_save(completion).unwrap();

    assert!(!session.active_is_modified());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "new\n");
}

#[tokio::test]
async fn readonly_save_failure_preserves_local_and_on_disk_text() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("readonly.txt");
    std::fs::write(&path, "original").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    append_text(&mut session, " local");

    let completion = session.save_buffer(id).unwrap().perform().await;
    assert!(completion.result.is_err());
    let error = session.complete_save(completion).unwrap_err();
    assert!(matches!(
        error,
        EditorError::SaveDocument { buffer_id, .. } if buffer_id == id
    ));
    assert!(session.active_is_modified());
    assert!(!session.has_saving_buffers());
    assert_eq!(session.buffers[&id].document.text(), "original local");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

#[tokio::test]
async fn external_modification_save_failure_does_not_overwrite_either_version() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("external.txt");
    std::fs::write(&path, "original").unwrap();
    let mut session = EditorSession::new();
    let id = open_file(&mut session, &path);
    append_text(&mut session, " local");
    let request = session.save_buffer(id).unwrap();
    std::fs::write(&path, "external").unwrap();
    let external = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    external
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(2))
        .unwrap();

    let completion = request.perform().await;
    assert!(completion.result.is_err());
    let error = session.complete_save(completion).unwrap_err();
    assert!(matches!(
        error,
        EditorError::SaveDocument { buffer_id, .. } if buffer_id == id
    ));
    assert!(session.active_is_modified());
    assert_eq!(session.buffers[&id].document.text(), "original local");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
}

#[test]
fn first_dirty_file_buffer_skips_untitled_documents() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.txt");
    std::fs::write(&path, "original").unwrap();
    let mut session = EditorSession::new();
    create_buffer(&mut session, "scratch");
    append_text(&mut session, " dirty");
    assert_eq!(session.first_dirty_file_buffer(), None);

    let file = open_file(&mut session, &path);
    append_text(&mut session, " dirty");

    assert_eq!(session.first_dirty_file_buffer(), Some(file));
}

#[test]
fn untitled_buffer_cannot_save_without_a_path() {
    let mut session = EditorSession::new();
    let id = create_buffer(&mut session, "scratch");

    match session.save_buffer(id) {
        Err(EditorError::SaveDocument { buffer_id, .. }) => assert_eq!(buffer_id, id),
        _ => panic!("untitled save unexpectedly created a save request"),
    }
}

#[test]
fn opens_files_and_deduplicates_canonical_paths() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("first.txt");
    let second_path = directory.path().join("second.txt");
    std::fs::write(&first_path, "first").unwrap();
    std::fs::write(&second_path, "second").unwrap();

    let mut session = EditorSession::new();
    let first = open_file(&mut session, &first_path);
    let second = open_file(&mut session, &second_path);
    assert_ne!(first, second);
    assert_eq!(session.active_buffer(), Some(second));

    let aliased_path = directory.path().join(".").join("first.txt");
    let reopened = open_file(&mut session, &aliased_path);
    assert_eq!(reopened, first);
    assert_eq!(session.active_buffer(), Some(first));
    assert_eq!(session.buffer_count(), 2);
    assert_eq!(session.buffers[&first].document.text(), "first");
}

#[test]
fn rejects_directory_without_creating_buffer() {
    let directory = tempfile::tempdir().unwrap();
    let session = EditorSession::new();

    let error = DocumentFactory::new().open(directory.path());
    assert!(matches!(error, Err(OpenDocumentError::NotTextFile)));
    assert!(session.is_empty());
    assert_eq!(session.active_buffer(), None);
}

#[test]
fn switching_buffers_preserves_cursor_and_scroll() {
    let text = (0..=8)
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let mut session = EditorSession::new();
    let first = create_buffer(&mut session, &text);

    let (expected_offset, expected_selection) = {
        let (document, view) = active_parts(&mut session);
        view.area = Rect::new(0, 0, 20, 4);
        let cursor = document.text().line_to_char(6);
        document.set_selection(view.id, Selection::point(cursor));
        view.ensure_cursor_in_view(document, 0);
        (
            document.view_offset(view.id),
            document.selection(view.id).clone(),
        )
    };

    let second = create_buffer(&mut session, "second");
    assert_eq!(session.active_buffer(), Some(second));
    session.activate_buffer(first).unwrap();

    let (document, view) = active_parts(&mut session);
    assert_eq!(document.selection(view.id), &expected_selection);
    assert_eq!(document.view_offset(view.id), expected_offset);
    assert!(view.is_cursor_in_view(document, 0));
}
