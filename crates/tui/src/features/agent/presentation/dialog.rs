use eggplant_agent::RunId;

use crate::{
    capabilities::page_scroll::{PageScroll, ScrollDirection},
    component::agent::{PageScrollState, ViewportMetrics},
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AgentDialogState {
    #[default]
    Closed,
    Open,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DialogAction {
    Open,
    Close,
    Suspend,
    Resume,
    RunStarted(RunId),
    Page {
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    },
}

/// Lifecycle and navigation data owned by the inline Agent dialog.
#[derive(Debug, Default)]
pub struct DialogPresentationState {
    lifecycle: AgentDialogState,
    scroll: PageScrollState,
}

impl DialogPresentationState {
    pub fn lifecycle(&self) -> AgentDialogState {
        self.lifecycle
    }

    pub fn is_open(&self) -> bool {
        self.lifecycle != AgentDialogState::Closed
    }

    pub fn scroll(&self) -> PageScrollState {
        self.scroll
    }

    pub fn reduce(&mut self, action: DialogAction) {
        match action {
            DialogAction::Open => {
                self.scroll.bottom();
                self.lifecycle = AgentDialogState::Open;
            }
            DialogAction::Close => self.lifecycle = AgentDialogState::Closed,
            DialogAction::Suspend => {
                if self.lifecycle == AgentDialogState::Open {
                    self.lifecycle = AgentDialogState::Suspended;
                }
            }
            DialogAction::Resume => {
                if self.lifecycle == AgentDialogState::Suspended {
                    self.lifecycle = AgentDialogState::Open;
                }
            }
            DialogAction::RunStarted(_) => self.scroll.bottom(),
            DialogAction::Page {
                page,
                direction,
                metrics,
            } => self.scroll.scroll(page, direction, metrics),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_drive_lifecycle_and_scroll_state() {
        let mut dialog = DialogPresentationState::default();
        assert!(!dialog.is_open());
        dialog.reduce(DialogAction::Open);
        assert_eq!(dialog.lifecycle(), AgentDialogState::Open);
        dialog.reduce(DialogAction::Suspend);
        assert_eq!(dialog.lifecycle(), AgentDialogState::Suspended);
        dialog.reduce(DialogAction::Resume);
        assert_eq!(dialog.lifecycle(), AgentDialogState::Open);
        dialog.reduce(DialogAction::Close);
        assert!(!dialog.is_open());
    }
}
