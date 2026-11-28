use eggplant_agent::RunId;

use crate::{
    capabilities::page_scroll::{PageScroll, ScrollDirection},
    component::agent::{PanelNavigation, ViewportMetrics},
    data_object::Context,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum AgentInputMode {
    #[default]
    Normal,
    Insert,
}

impl AgentInputMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Insert => "Insert",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelAction {
    Prepare,
    EnterInsert,
    EnterNormal,
    ToggleHelp,
    CloseHelp,
    RunStarted(RunId),
    Page {
        page: PageScroll,
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    },
    MoveCursor {
        direction: ScrollDirection,
        metrics: ViewportMetrics,
    },
}

/// Mode, help, and transcript navigation data owned by the Agent panel.
#[derive(Debug, Default)]
pub struct PanelPresentationState {
    mode: AgentInputMode,
    help_visible: bool,
    navigation: PanelNavigation,
}

impl PanelPresentationState {
    pub fn mode(&self) -> AgentInputMode {
        self.mode
    }

    pub fn input_context(&self) -> Context {
        match self.mode {
            AgentInputMode::Normal => Context::AgentChat,
            AgentInputMode::Insert => Context::AgentChatInsert,
        }
    }

    pub fn navigation(&self) -> PanelNavigation {
        self.navigation
    }

    pub fn help_visible(&self) -> bool {
        self.help_visible
    }

    pub fn reduce(&mut self, action: PanelAction) {
        match action {
            PanelAction::Prepare => {
                self.mode = AgentInputMode::Normal;
                self.navigation.enter_normal();
                self.help_visible = false;
            }
            PanelAction::EnterInsert => {
                self.mode = AgentInputMode::Insert;
                self.help_visible = false;
            }
            PanelAction::EnterNormal => {
                self.mode = AgentInputMode::Normal;
                self.navigation.enter_normal();
            }
            PanelAction::ToggleHelp => self.help_visible = !self.help_visible,
            PanelAction::CloseHelp => self.help_visible = false,
            PanelAction::RunStarted(_) => self.navigation.follow_bottom(),
            PanelAction::Page {
                page,
                direction,
                metrics,
            } => match self.mode {
                AgentInputMode::Normal => self.navigation.page(page, direction, metrics),
                AgentInputMode::Insert => self.navigation.scroll(page, direction, metrics),
            },
            PanelAction::MoveCursor { direction, metrics } => {
                if self.mode == AgentInputMode::Normal {
                    self.navigation.move_cursor(direction, metrics);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_drive_mode_help_and_navigation_state() {
        let mut panel = PanelPresentationState::default();
        panel.reduce(PanelAction::ToggleHelp);
        assert!(panel.help_visible());
        panel.reduce(PanelAction::EnterInsert);
        assert_eq!(panel.mode(), AgentInputMode::Insert);
        assert!(!panel.help_visible());
        panel.reduce(PanelAction::EnterNormal);
        assert_eq!(panel.mode(), AgentInputMode::Normal);
        assert_eq!(panel.input_context(), Context::AgentChat);
    }
}
