use crossterm::event::KeyCode;

use crate::{
    manager::{command_registry::CommandId, context::KeyChord},
    theme_picker::ThemePickerCommand,
};

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

impl CommandMenuItem {
    pub const fn action(key: char, description: &'static str, command: CommandId) -> Self {
        Self {
            key,
            description,
            target: CommandMenuTarget::Action(command),
        }
    }

    pub const fn submenu(
        key: char,
        description: &'static str,
        context: &'static CommandMenuContext,
    ) -> Self {
        Self {
            key,
            description,
            target: CommandMenuTarget::Submenu(context),
        }
    }
}

#[derive(Debug)]
pub struct CommandMenuContext {
    title: &'static str,
    items: &'static [CommandMenuItem],
}

impl CommandMenuContext {
    pub const fn new(title: &'static str, items: &'static [CommandMenuItem]) -> Self {
        Self { title, items }
    }
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
            description: "next",
            target: CommandMenuTarget::Action(CommandId("next_buffer")),
        },
        CommandMenuItem {
            key: 'p',
            description: "previous",
            target: CommandMenuTarget::Action(CommandId("previous_buffer")),
        },
        CommandMenuItem {
            key: 'c',
            description: "close",
            target: CommandMenuTarget::Action(CommandId("close_buffer")),
        },
        CommandMenuItem {
            key: 'o',
            description: "close others",
            target: CommandMenuTarget::Action(CommandId("close_other_buffers")),
        },
        CommandMenuItem {
            key: 'r',
            description: "close right",
            target: CommandMenuTarget::Action(CommandId("close_buffers_right")),
        },
        CommandMenuItem {
            key: 'a',
            description: "close all",
            target: CommandMenuTarget::Action(CommandId("close_all_buffers")),
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
    items: &[
        CommandMenuItem {
            key: 't',
            description: "theme",
            target: CommandMenuTarget::Action(ThemePickerCommand::Open.id()),
        },
        CommandMenuItem::action('c', "toggle cursor line", CommandId("cursor_line_toggle")),
        CommandMenuItem {
            key: 'w',
            description: "toggle editor wrap",
            target: CommandMenuTarget::Action(CommandId("view_toggle_wrap")),
        },
    ],
};

const GIT_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "git",
    items: &[CommandMenuItem {
        key: 'g',
        description: "log",
        target: CommandMenuTarget::Action(CommandId("open_vcs_page")),
    }],
};

static AI_CONTEXT: CommandMenuContext = CommandMenuContext::new(
    "ai",
    &[CommandMenuItem::action(
        't',
        "toggle agent panel",
        CommandId("agent_toggle_panel"),
    )],
);

static CODE_CONTEXT: CommandMenuContext = CommandMenuContext::new(
    "code",
    &[
        CommandMenuItem::action('d', "definition", CommandId("code_definition")),
        CommandMenuItem::action('r', "references", CommandId("code_references")),
        CommandMenuItem::action('i', "implementation", CommandId("code_implementation")),
        CommandMenuItem::action('l', "diagnostics", CommandId("code_diagnostics")),
    ],
);

static SEARCH_CONTEXT: CommandMenuContext = CommandMenuContext::new(
    "search",
    &[
        crate::buffer_jump::SEARCH_MENU_ITEM,
        crate::buffer_grep::CURRENT_FILE_MENU_ITEM,
        crate::project_grep::PROJECT_MENU_ITEM,
    ],
);

const ROOT_CONTEXT: CommandMenuContext = CommandMenuContext {
    title: "Leader",
    items: &[
        CommandMenuItem::action(' ', "find file", CommandId("open_file_picker")),
        CommandMenuItem {
            key: 'q',
            description: "quit",
            target: CommandMenuTarget::Submenu(&QUIT_CONTEXT),
        },
        CommandMenuItem::submenu('a', "ai", &AI_CONTEXT),
        CommandMenuItem {
            key: 'b',
            description: "buffers",
            target: CommandMenuTarget::Submenu(&BUFFER_CONTEXT),
        },
        CommandMenuItem::submenu('c', "code", &CODE_CONTEXT),
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
            key: 'g',
            description: "git",
            target: CommandMenuTarget::Submenu(&GIT_CONTEXT),
        },
        CommandMenuItem::submenu('s', "search", &SEARCH_CONTEXT),
        CommandMenuItem {
            key: 'u',
            description: "ui",
            target: CommandMenuTarget::Submenu(&UI_CONTEXT),
        },
    ],
};

/// Active context stack for common commands opened by the configured leader.
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

    pub fn start(&mut self) {
        self.contexts.clear();
        self.contexts.push(&ROOT_CONTEXT);
    }

    pub fn handle_key(&mut self, key: impl Into<KeyChord>) -> CommandMenuOutcome {
        let key = key.into();
        if key.code == KeyCode::Esc {
            self.contexts.clear();
            return CommandMenuOutcome::Cancelled;
        }

        let Some(context) = self.current() else {
            return CommandMenuOutcome::Cancelled;
        };
        let Some(item) = context
            .items
            .iter()
            .find(|item| key.modifiers.is_empty() && key.code == KeyCode::Char(item.key))
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
        menu.start();
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

        menu.start();
        menu.handle_key(KeyCode::Char('b'));
        assert_eq!(
            menu.handle_key(KeyCode::Char('c')),
            CommandMenuOutcome::Action(CommandId("close_buffer"))
        );

        for (key, command) in [
            ('o', "close_other_buffers"),
            ('r', "close_buffers_right"),
            ('a', "close_all_buffers"),
        ] {
            menu.start();
            menu.handle_key(KeyCode::Char('b'));
            assert_eq!(
                menu.handle_key(KeyCode::Char(key)),
                CommandMenuOutcome::Action(CommandId(command))
            );
        }

        menu.start();
        menu.handle_key(KeyCode::Char('c'));
        assert_eq!(menu.context_title(), "Leader > code");
        assert_eq!(
            menu.handle_key(KeyCode::Char('d')),
            CommandMenuOutcome::Action(CommandId("code_definition"))
        );
        for (key, command) in [
            ('r', "code_references"),
            ('i', "code_implementation"),
            ('l', "code_diagnostics"),
        ] {
            menu.start();
            menu.handle_key(KeyCode::Char('c'));
            assert_eq!(
                menu.handle_key(KeyCode::Char(key)),
                CommandMenuOutcome::Action(CommandId(command))
            );
        }

        menu.start();
        menu.handle_key(KeyCode::Char('s'));
        assert_eq!(menu.context_title(), "Leader > search");
        assert_eq!(
            menu.handle_key(KeyCode::Char('j')),
            CommandMenuOutcome::Action(CommandId("buffer_jump_start"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('s'));
        assert_eq!(menu.context_title(), "Leader > search");
        assert_eq!(
            menu.handle_key(KeyCode::Char('c')),
            CommandMenuOutcome::Action(CommandId("buffer_grep_open"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('s'));
        assert_eq!(
            menu.handle_key(KeyCode::Char('p')),
            CommandMenuOutcome::Action(CommandId("project_grep_open"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('a'));
        assert_eq!(menu.context_title(), "Leader > ai");
        assert_eq!(
            menu.handle_key(KeyCode::Char('t')),
            CommandMenuOutcome::Action(CommandId("agent_toggle_panel"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('q'));
        assert_eq!(menu.context_title(), "Leader > quit");
        assert_eq!(
            menu.handle_key(KeyCode::Char('q')),
            CommandMenuOutcome::Action(CommandId("quit"))
        );

        menu.start();
        assert_eq!(
            menu.handle_key(KeyCode::Char('e')),
            CommandMenuOutcome::Action(CommandId("toggle_file_explorer"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('f'));
        assert_eq!(menu.context_title(), "Leader > files");
        assert_eq!(
            menu.handle_key(KeyCode::Char('f')),
            CommandMenuOutcome::Action(CommandId("open_file_picker"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('g'));
        assert_eq!(menu.context_title(), "Leader > git");
        assert_eq!(
            menu.handle_key(KeyCode::Char('g')),
            CommandMenuOutcome::Action(CommandId("open_vcs_page"))
        );

        menu.start();
        assert_eq!(
            menu.handle_key(KeyCode::Char(' ')),
            CommandMenuOutcome::Action(CommandId("open_file_picker"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('u'));
        assert_eq!(menu.context_title(), "Leader > ui");
        assert_eq!(
            menu.handle_key(KeyCode::Char('t')),
            CommandMenuOutcome::Action(ThemePickerCommand::Open.id())
        );

        menu.start();
        menu.handle_key(KeyCode::Char('u'));
        assert_eq!(
            menu.handle_key(KeyCode::Char('c')),
            CommandMenuOutcome::Action(CommandId("cursor_line_toggle"))
        );

        menu.start();
        menu.handle_key(KeyCode::Char('u'));
        assert_eq!(
            menu.handle_key(KeyCode::Char('w')),
            CommandMenuOutcome::Action(CommandId("view_toggle_wrap"))
        );
    }

    #[test]
    fn escape_cancels_pending_sequence() {
        let mut menu = CommandMenuState::default();
        menu.start();
        assert_eq!(menu.handle_key(KeyCode::Esc), CommandMenuOutcome::Cancelled);
        assert!(!menu.is_active());
    }
}
