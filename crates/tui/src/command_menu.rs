use crossterm::event::KeyCode;

use crate::manager::command_registry::CommandId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandMenuOutcome {
    Pending,
    Cancelled,
    Action(CommandId),
}

#[derive(Debug, Clone, Copy)]
enum CommandMenuTarget {
    Submenu(&'static CommandMenuContext),
    Action(CommandId),
}

#[derive(Debug, Clone, Copy)]
pub struct CommandMenuItem {
    pub key: char,
    pub description: &'static str,
    target: CommandMenuTarget,
}

#[derive(Debug)]
struct CommandMenuContext {
    title: &'static str,
    items: &'static [CommandMenuItem],
}

const QUIT_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "quit",
    items: &[CommandMenuItem {
        key: 'q',
        description: "quit",
        target: CommandMenuTarget::Action(CommandId("quit")),
    }],
};

const BUFFER_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "buffer",
    items: &[
        CommandMenuItem {
            key: 'n',
            description: "next buffer",
            target: CommandMenuTarget::Action(CommandId("next_buffer")),
        },
        CommandMenuItem {
            key: 'p',
            description: "previous buffer",
            target: CommandMenuTarget::Action(CommandId("previous_buffer")),
        },
    ],
};

const FILES_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "files",
    items: &[CommandMenuItem {
        key: 'f',
        description: "find file",
        target: CommandMenuTarget::Action(CommandId("open_file_picker")),
    }],
};

const UI_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "ui",
    items: &[CommandMenuItem {
        key: 't',
        description: "theme",
        target: CommandMenuTarget::Action(CommandId("open_theme_picker")),
    }],
};

const ROOT_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "Leader",
    items: &[
        CommandMenuItem {
            key: 'q',
            description: "quit",
            target: CommandMenuTarget::Submenu(&QUIT_CONTEXT),
        },
        CommandMenuItem {
            key: 'b',
            description: "buffers",
            target: CommandMenuTarget::Submenu(&BUFFER_CONTEXT),
        },
        CommandMenuItem {
            key: 'e',
            description: "toggle files",
            target: CommandMenuTarget::Action(CommandId("toggle_file_explorer")),
        },
        CommandMenuItem {
            key: 'f',
            description: "files",
            target: CommandMenuTarget::Submenu(&FILES_CONTEXT),
        },
        CommandMenuItem {
            key: 'u',
            description: "ui",
            target: CommandMenuTarget::Submenu(&UI_CONTEXT),
        },
    ],
};

/// Active context stack for built-in Space-prefixed common commands.
#[derive(Debug, Default)]
pub struct CommandMenuState {
    contexts: Vec<&'static CommandMenuContext>,
}

impl CommandMenuState {
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

    pub fn items(&self) -> &'static [CommandMenuItem] {
        self.current().map_or(&[], |context| context.items)
    }

    pub fn handle_key(&mut self, key: KeyCode) -> CommandMenuOutcome {
        if key == KeyCode::Esc {
            self.contexts.clear();
            return CommandMenuOutcome::Cancelled;
        }

        let Some(context) = self.current() else {
            if key == KeyCode::Char(' ') {
                self.contexts.push(&ROOT_CONTEXT);
                return CommandMenuOutcome::Pending;
            }
            return CommandMenuOutcome::Cancelled;
        };
        let Some(item) = context
            .items
            .iter()
            .find(|item| matches!(key, KeyCode::Char(character) if character == item.key))
        else {
            return self.cancel();
        };

        match item.target {
            CommandMenuTarget::Submenu(submenu) => {
                self.contexts.push(submenu);
                CommandMenuOutcome::Pending
            }
            CommandMenuTarget::Action(action) => self.finish(action),
        }
    }

    fn current(&self) -> Option<&'static CommandMenuContext> {
        self.contexts.last().copied()
    }

    fn finish(&mut self, action: CommandId) -> CommandMenuOutcome {
        self.contexts.clear();
        CommandMenuOutcome::Action(action)
    }

    fn cancel(&mut self) -> CommandMenuOutcome {
        self.contexts.clear();
        CommandMenuOutcome::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager::command_registry::CommandRegistry;

    fn assert_targets_registered(context: &CommandMenuContext, registry: &CommandRegistry) {
        for item in context.items {
            match item.target {
                CommandMenuTarget::Submenu(submenu) => assert_targets_registered(submenu, registry),
                CommandMenuTarget::Action(id) => assert!(
                    registry.get(id.as_str()).is_some(),
                    "leader command `{}` is not registered",
                    id.as_str()
                ),
            }
        }
    }

    #[test]
    fn every_leader_target_is_registered() {
        assert_targets_registered(&ROOT_CONTEXT, &CommandRegistry::builtin());
    }

    #[test]
    fn menu_sequences_emit_expected_actions() {
        let mut menu = CommandMenuState::default();
        assert_eq!(
            menu.handle_key(KeyCode::Char(' ')),
            CommandMenuOutcome::Pending
        );
        assert!(menu.is_active());
        assert_eq!(menu.context_title(), "Leader");
        assert_eq!(
            menu.handle_key(KeyCode::Char('b')),
            CommandMenuOutcome::Pending
        );
        assert_eq!(menu.context_title(), "Leader > buffer");
        assert_eq!(
            menu.handle_key(KeyCode::Char('n')),
            CommandMenuOutcome::Action(CommandId("next_buffer"))
        );
        assert!(!menu.is_active());

        menu.handle_key(KeyCode::Char(' '));
        menu.handle_key(KeyCode::Char('q'));
        assert_eq!(menu.context_title(), "Leader > quit");
        assert_eq!(
            menu.handle_key(KeyCode::Char('q')),
            CommandMenuOutcome::Action(CommandId("quit"))
        );

        menu.handle_key(KeyCode::Char(' '));
        assert_eq!(
            menu.handle_key(KeyCode::Char('e')),
            CommandMenuOutcome::Action(CommandId("toggle_file_explorer"))
        );

        menu.handle_key(KeyCode::Char(' '));
        menu.handle_key(KeyCode::Char('f'));
        assert_eq!(menu.context_title(), "Leader > files");
        assert_eq!(
            menu.handle_key(KeyCode::Char('f')),
            CommandMenuOutcome::Action(CommandId("open_file_picker"))
        );

        menu.handle_key(KeyCode::Char(' '));
        menu.handle_key(KeyCode::Char('u'));
        assert_eq!(menu.context_title(), "Leader > ui");
        assert_eq!(
            menu.handle_key(KeyCode::Char('t')),
            CommandMenuOutcome::Action(CommandId("open_theme_picker"))
        );
    }

    #[test]
    fn escape_cancels_pending_sequence() {
        let mut menu = CommandMenuState::default();
        menu.handle_key(KeyCode::Char(' '));
        assert_eq!(menu.handle_key(KeyCode::Esc), CommandMenuOutcome::Cancelled);
        assert!(!menu.is_active());
    }
}
