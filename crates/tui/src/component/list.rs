/// Minimal selectable-list state shared by pickers, menus, and prompt commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListState<T> {
    items: Vec<T>,
    selected: Option<usize>,
}

impl<T> Default for ListState<T> {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<T> ListState<T> {
    pub fn new(items: Vec<T>) -> Self {
        Self {
            items,
            selected: None,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_item(&self) -> Option<&T> {
        self.selected.and_then(|index| self.items.get(index))
    }

    pub fn select(&mut self, index: Option<usize>) {
        self.selected = index.filter(|index| *index < self.items.len());
    }

    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    pub fn select_current(&mut self) -> Option<&T> {
        if self.items.is_empty() {
            self.selected = None;
            return None;
        }
        let index = self.selected.unwrap_or(0).min(self.items.len() - 1);
        self.selected = Some(index);
        self.selected_item()
    }

    pub fn select_next(&mut self) -> Option<&T> {
        if self.items.is_empty() {
            self.selected = None;
            return None;
        }
        self.selected = Some(
            self.selected
                .map_or(0, |index| (index + 1) % self.items.len()),
        );
        self.selected_item()
    }

    pub fn select_previous(&mut self) -> Option<&T> {
        if self.items.is_empty() {
            self.selected = None;
            return None;
        }
        self.selected = Some(self.selected.map_or(self.items.len() - 1, |index| {
            (index + self.items.len() - 1) % self.items.len()
        }));
        self.selected_item()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_is_optional_and_navigation_wraps() {
        let mut list = ListState::new(vec!["a", "b", "c"]);
        assert_eq!(list.selected_item(), None);
        assert_eq!(list.select_next(), Some(&"a"));
        assert_eq!(list.select_next(), Some(&"b"));
        assert_eq!(list.select_previous(), Some(&"a"));
        assert_eq!(list.select_previous(), Some(&"c"));
        assert_eq!(list.select_next(), Some(&"a"));
        list.clear_selection();
        assert_eq!(list.selected_item(), None);
        assert_eq!(list.select_current(), Some(&"a"));
    }

    #[test]
    fn empty_lists_do_not_select() {
        let mut list = ListState::<usize>::default();
        assert_eq!(list.select_next(), None);
        assert_eq!(list.select_previous(), None);
        assert_eq!(list.select_current(), None);
    }
}
