use editor::EditorSession;

/// Minimal vertical-navigation capability shared by documents and selectable lists.
pub(crate) trait VerticalNavigation {
    fn item_count(&self) -> usize;
    fn move_to(&mut self, index: usize);

    fn accepts_count(&self) -> bool {
        false
    }

    fn move_to_start(&mut self) {
        if self.item_count() > 0 {
            self.move_to(0);
        }
    }

    fn move_to_end(&mut self) {
        if let Some(index) = self.item_count().checked_sub(1) {
            self.move_to(index);
        }
    }
}

impl VerticalNavigation for EditorSession {
    fn item_count(&self) -> usize {
        self.active_snapshot()
            .map(|snapshot| snapshot.text.line_count)
            .unwrap_or(0)
    }

    fn move_to(&mut self, index: usize) {
        if let Ok(mut editor) = self.active_editor() {
            let _ = editor.move_cursor_to_line(index);
        }
    }

    fn accepts_count(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ListTarget {
        len: usize,
        selected: Option<usize>,
    }

    impl VerticalNavigation for ListTarget {
        fn item_count(&self) -> usize {
            self.len
        }

        fn move_to(&mut self, index: usize) {
            self.selected = self.len.checked_sub(1).map(|last| index.min(last));
        }
    }

    #[test]
    fn start_end_and_index_navigation_compose_with_a_small_list() {
        let mut target = ListTarget {
            len: 3,
            selected: None,
        };
        target.move_to_start();
        assert_eq!(target.selected, Some(0));
        target.move_to_end();
        assert_eq!(target.selected, Some(2));
        target.move_to(99);
        assert_eq!(target.selected, Some(2));
        assert!(!target.accepts_count());

        let mut empty = ListTarget {
            len: 0,
            selected: None,
        };
        empty.move_to_start();
        empty.move_to_end();
        assert_eq!(empty.selected, None);
    }
}
