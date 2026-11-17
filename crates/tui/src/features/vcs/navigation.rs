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
        if self.state.history_pane == HistoryPane::Detail {
            return match &self.state.history_detail {
                CommitDetailsState::Ready(details) => details.files.len(),
                _ => 0,
            };
        }
        match &self.state.history {
            HistoryState::Ready(commits) => commits.len(),
            _ => 0,
        }
    }

    fn move_to(&mut self, index: usize) {
        if self.state.history_pane == HistoryPane::Detail {
            if let CommitDetailsState::Ready(details) = &self.state.history_detail {
                self.state.history_detail_selected_file = details
                    .files
                    .len()
                    .checked_sub(1)
                    .map(|last| index.min(last));
            }
            return;
        }
        let HistoryState::Ready(commits) = &self.state.history else {
            return;
        };
        self.state.history_selected = commits.len().checked_sub(1).map(|last| index.min(last));
    }
}
