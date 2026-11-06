use crossterm::event::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderAction {
    Quit,
    NextBuffer,
    PreviousBuffer,
    ToggleFileExplorer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderOutcome {
    Pending,
    Cancelled,
    Action(LeaderAction),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaderItem {
    pub key: &'static str,
    pub description: &'static str,
}

const ROOT_ITEMS: &[LeaderItem] = &[
    LeaderItem {
        key: "q q",
        description: "quit",
    },
    LeaderItem {
        key: "b",
        description: "buffers",
    },
    LeaderItem {
        key: "e",
        description: "toggle files",
    },
];
const BUFFER_ITEMS: &[LeaderItem] = &[
    LeaderItem {
        key: "n",
        description: "next buffer",
    },
    LeaderItem {
        key: "p",
        description: "previous buffer",
    },
];
const QUIT_ITEMS: &[LeaderItem] = &[LeaderItem {
    key: "q",
    description: "quit",
}];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LeaderMenu {
    Root,
    Buffer,
    Quit,
}

/// State for built-in Space-prefixed common commands.
#[derive(Debug, Default)]
pub struct LeaderState {
    menu: Option<LeaderMenu>,
}

impl LeaderState {
    pub fn is_active(&self) -> bool {
        self.menu.is_some()
    }

    pub fn items(&self) -> &'static [LeaderItem] {
        match self.menu {
            Some(LeaderMenu::Root) => ROOT_ITEMS,
            Some(LeaderMenu::Buffer) => BUFFER_ITEMS,
            Some(LeaderMenu::Quit) => QUIT_ITEMS,
            None => &[],
        }
    }

    pub fn handle_key(&mut self, key: KeyCode) -> LeaderOutcome {
        if key == KeyCode::Esc {
            self.menu = None;
            return LeaderOutcome::Cancelled;
        }

        match self.menu {
            None if key == KeyCode::Char(' ') => {
                self.menu = Some(LeaderMenu::Root);
                LeaderOutcome::Pending
            }
            None => LeaderOutcome::Cancelled,
            Some(LeaderMenu::Root) => match key {
                KeyCode::Char('q') => {
                    self.menu = Some(LeaderMenu::Quit);
                    LeaderOutcome::Pending
                }
                KeyCode::Char('b') => {
                    self.menu = Some(LeaderMenu::Buffer);
                    LeaderOutcome::Pending
                }
                KeyCode::Char('e') => self.finish(LeaderAction::ToggleFileExplorer),
                _ => self.cancel(),
            },
            Some(LeaderMenu::Buffer) => match key {
                KeyCode::Char('n') => self.finish(LeaderAction::NextBuffer),
                KeyCode::Char('p') => self.finish(LeaderAction::PreviousBuffer),
                _ => self.cancel(),
            },
            Some(LeaderMenu::Quit) => match key {
                KeyCode::Char('q') => self.finish(LeaderAction::Quit),
                _ => self.cancel(),
            },
        }
    }

    fn finish(&mut self, action: LeaderAction) -> LeaderOutcome {
        self.menu = None;
        LeaderOutcome::Action(action)
    }

    fn cancel(&mut self) -> LeaderOutcome {
        self.menu = None;
        LeaderOutcome::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leader_sequences_emit_expected_actions() {
        let mut leader = LeaderState::default();
        assert_eq!(
            leader.handle_key(KeyCode::Char(' ')),
            LeaderOutcome::Pending
        );
        assert!(leader.is_active());
        assert_eq!(
            leader.handle_key(KeyCode::Char('b')),
            LeaderOutcome::Pending
        );
        assert_eq!(
            leader.handle_key(KeyCode::Char('n')),
            LeaderOutcome::Action(LeaderAction::NextBuffer)
        );
        assert!(!leader.is_active());

        leader.handle_key(KeyCode::Char(' '));
        leader.handle_key(KeyCode::Char('q'));
        assert_eq!(
            leader.handle_key(KeyCode::Char('q')),
            LeaderOutcome::Action(LeaderAction::Quit)
        );

        leader.handle_key(KeyCode::Char(' '));
        assert_eq!(
            leader.handle_key(KeyCode::Char('e')),
            LeaderOutcome::Action(LeaderAction::ToggleFileExplorer)
        );
    }

    #[test]
    fn escape_cancels_pending_sequence() {
        let mut leader = LeaderState::default();
        leader.handle_key(KeyCode::Char(' '));
        assert_eq!(leader.handle_key(KeyCode::Esc), LeaderOutcome::Cancelled);
        assert!(!leader.is_active());
    }
}
