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

#[derive(Debug, Clone, Copy)]
enum LeaderTarget {
    Submenu(&'static LeaderContext),
    Action(LeaderAction),
}

#[derive(Debug, Clone, Copy)]
pub struct LeaderItem {
    pub key: char,
    pub description: &'static str,
    target: LeaderTarget,
}

#[derive(Debug)]
struct LeaderContext {
    title: &'static str,
    items: &'static [LeaderItem],
}

const QUIT_CONTEXT: LeaderContext = LeaderContext {
    title: "quit",
    items: &[LeaderItem {
        key: 'q',
        description: "quit",
        target: LeaderTarget::Action(LeaderAction::Quit),
    }],
};

const BUFFER_CONTEXT: LeaderContext = LeaderContext {
    title: "buffer",
    items: &[
        LeaderItem {
            key: 'n',
            description: "next buffer",
            target: LeaderTarget::Action(LeaderAction::NextBuffer),
        },
        LeaderItem {
            key: 'p',
            description: "previous buffer",
            target: LeaderTarget::Action(LeaderAction::PreviousBuffer),
        },
    ],
};

const ROOT_CONTEXT: LeaderContext = LeaderContext {
    title: "Leader",
    items: &[
        LeaderItem {
            key: 'q',
            description: "quit",
            target: LeaderTarget::Submenu(&QUIT_CONTEXT),
        },
        LeaderItem {
            key: 'b',
            description: "buffers",
            target: LeaderTarget::Submenu(&BUFFER_CONTEXT),
        },
        LeaderItem {
            key: 'e',
            description: "toggle files",
            target: LeaderTarget::Action(LeaderAction::ToggleFileExplorer),
        },
    ],
};

/// Active context stack for built-in Space-prefixed common commands.
#[derive(Debug, Default)]
pub struct LeaderState {
    contexts: Vec<&'static LeaderContext>,
}

impl LeaderState {
    pub fn is_active(&self) -> bool {
        !self.contexts.is_empty()
    }

    pub fn context_title(&self) -> String {
        self.contexts
            .iter()
            .map(|context| context.title)
            .collect::<Vec<_>>()
            .join(" > ")
    }

    pub fn items(&self) -> &'static [LeaderItem] {
        self.current().map_or(&[], |context| context.items)
    }

    pub fn handle_key(&mut self, key: KeyCode) -> LeaderOutcome {
        if key == KeyCode::Esc {
            self.contexts.clear();
            return LeaderOutcome::Cancelled;
        }

        let Some(context) = self.current() else {
            if key == KeyCode::Char(' ') {
                self.contexts.push(&ROOT_CONTEXT);
                return LeaderOutcome::Pending;
            }
            return LeaderOutcome::Cancelled;
        };
        let Some(item) = context
            .items
            .iter()
            .find(|item| matches!(key, KeyCode::Char(character) if character == item.key))
        else {
            return self.cancel();
        };

        match item.target {
            LeaderTarget::Submenu(submenu) => {
                self.contexts.push(submenu);
                LeaderOutcome::Pending
            }
            LeaderTarget::Action(action) => self.finish(action),
        }
    }

    fn current(&self) -> Option<&'static LeaderContext> {
        self.contexts.last().copied()
    }

    fn finish(&mut self, action: LeaderAction) -> LeaderOutcome {
        self.contexts.clear();
        LeaderOutcome::Action(action)
    }

    fn cancel(&mut self) -> LeaderOutcome {
        self.contexts.clear();
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
        assert_eq!(leader.context_title(), "Leader");
        assert_eq!(
            leader.handle_key(KeyCode::Char('b')),
            LeaderOutcome::Pending
        );
        assert_eq!(leader.context_title(), "Leader > buffer");
        assert_eq!(
            leader.handle_key(KeyCode::Char('n')),
            LeaderOutcome::Action(LeaderAction::NextBuffer)
        );
        assert!(!leader.is_active());

        leader.handle_key(KeyCode::Char(' '));
        leader.handle_key(KeyCode::Char('q'));
        assert_eq!(leader.context_title(), "Leader > quit");
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
