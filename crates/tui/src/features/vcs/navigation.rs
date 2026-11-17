use crate::navigation::VerticalNavigation;

use super::{CommitDetailsState, HistoryPane, HistoryState, VcsState};

pub(crate) struct HistoryNavigation<'a> {
    state: &'a mut VcsState,
}

impl<'a> HistoryNavigation<'a> {
    pub(crate) fn new(state: &'a mut VcsState) -> Self {
        Self { state }
    }
}

impl VerticalNavigation for HistoryNavigation<'_> {
    fn item_count(&self) -> usize {
        if self.state.history.pane == HistoryPane::Detail {
            return match &self.state.history.detail {
                CommitDetailsState::Ready(details) => details.files.len(),
                _ => 0,
            };
        }
        match self.state.history.state() {
            HistoryState::Ready(commits) => commits.len(),
            _ => 0,
        }
    }

    fn move_to(&mut self, index: usize) {
        if self.state.history.pane == HistoryPane::Detail {
            if let CommitDetailsState::Ready(details) = &self.state.history.detail {
                self.state.history.selected_file = details
                    .files
                    .len()
                    .checked_sub(1)
                    .map(|last| index.min(last));
            }
            return;
        }
        let HistoryState::Ready(commits) = &mut self.state.history.state else {
            return;
        };
        self.state.history.selected = commits.len().checked_sub(1).map(|last| index.min(last));
    }
}
