use std::{borrow::Borrow, collections::HashMap};

use color_eyre::eyre::eyre;
use editor::CursorMovement;

use crate::data_object::{FocusDirection, Message};

/// Stable semantic command identity shared by keymaps and leader menus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandId(pub &'static str);

impl CommandId {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl Borrow<str> for CommandId {
    fn borrow(&self) -> &str {
        self.0
    }
}

/// A registered command. Message resolution is the M6.1 compatibility boundary;
/// later slices replace this with narrower feature-owned handlers.
#[derive(Debug, Clone)]
pub struct Command {
    pub id: CommandId,
    pub description: &'static str,
    message: Message,
}

impl Command {
    pub fn new(id: CommandId, description: &'static str, message: Message) -> Self {
        Self {
            id,
            description,
            message,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CommandRegistry {
    commands: HashMap<CommandId, Command>,
}

impl CommandRegistry {
    pub fn empty() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, command: Command) -> color_eyre::Result<()> {
        let id = command.id;
        if self.commands.contains_key(&id) {
            return Err(eyre!("duplicate command `{}`", id.as_str()));
        }
        self.commands.insert(id, command);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Command> {
        self.commands.get(id)
    }

    pub fn message_for(&self, id: &str) -> Option<Message> {
        self.get(id).map(|command| command.message.clone())
    }

    pub fn builtin() -> Self {
        let mut registry = Self::empty();
        for (id, description, message) in builtin_commands() {
            registry
                .register(Command::new(CommandId(id), description, message))
                .expect("built-in command IDs must be unique");
        }
        registry
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_commands() -> Vec<(&'static str, &'static str, Message)> {
    vec![
        ("quit", "quit", Message::Quit),
        (
            "enter_insert_mode",
            "enter Insert mode",
            Message::EnterInsertMode,
        ),
        (
            "exit_insert_mode",
            "leave Insert mode",
            Message::ExitInsertMode,
        ),
        (
            "move_left",
            "move left",
            Message::MoveCursor(CursorMovement::Left),
        ),
        (
            "move_right",
            "move right",
            Message::MoveCursor(CursorMovement::Right),
        ),
        (
            "move_up",
            "move up",
            Message::MoveCursor(CursorMovement::Up),
        ),
        (
            "move_down",
            "move down",
            Message::MoveCursor(CursorMovement::Down),
        ),
        (
            "focus_left",
            "focus left",
            Message::MoveFocus(FocusDirection::Left),
        ),
        (
            "focus_down",
            "focus down",
            Message::MoveFocus(FocusDirection::Down),
        ),
        (
            "focus_up",
            "focus up",
            Message::MoveFocus(FocusDirection::Up),
        ),
        (
            "focus_right",
            "focus right",
            Message::MoveFocus(FocusDirection::Right),
        ),
        ("backspace", "backspace", Message::Backspace),
        ("insert_newline", "insert newline", Message::InsertNewline),
        ("undo", "undo", Message::Undo),
        ("redo", "redo", Message::Redo),
        ("close_buffer", "close buffer", Message::CloseActive),
        ("next_buffer", "next buffer", Message::NextBuffer),
        (
            "previous_buffer",
            "previous buffer",
            Message::PreviousBuffer,
        ),
        ("explorer_next", "next entry", Message::ExplorerNext),
        (
            "explorer_previous",
            "previous entry",
            Message::ExplorerPrevious,
        ),
        ("open_selected", "open selected", Message::OpenSelected),
        ("explorer_fold", "fold folder", Message::ExplorerFold),
        ("explorer_unfold", "unfold folder", Message::ExplorerUnfold),
        (
            "refresh_explorer",
            "refresh explorer",
            Message::RefreshExplorer,
        ),
        (
            "toggle_ignored_files",
            "toggle ignored files",
            Message::ToggleIgnoredFiles,
        ),
        (
            "toggle_file_explorer",
            "toggle files",
            Message::ToggleFileExplorer,
        ),
        ("open_file_picker", "find file", Message::OpenFilePicker),
        (
            "close_file_picker",
            "close file picker",
            Message::CloseFilePicker,
        ),
        (
            "file_picker_backspace",
            "delete query character",
            Message::FilePickerBackspace,
        ),
        ("open_picker_file", "open file", Message::OpenPickerFile),
        ("file_picker_next", "next file", Message::FilePickerNext),
        (
            "file_picker_previous",
            "previous file",
            Message::FilePickerPrevious,
        ),
        ("open_theme_picker", "theme", Message::OpenThemePicker),
        (
            "close_theme_picker",
            "close theme picker",
            Message::CloseThemePicker,
        ),
        ("theme_picker_next", "next theme", Message::ThemePickerNext),
        (
            "theme_picker_previous",
            "previous theme",
            Message::ThemePickerPrevious,
        ),
        ("apply_theme", "apply theme", Message::ApplyTheme),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_command_ids() {
        let mut registry = CommandRegistry::empty();
        registry
            .register(Command::new(
                CommandId("test.command"),
                "first",
                Message::Undo,
            ))
            .unwrap();

        let error = registry
            .register(Command::new(
                CommandId("test.command"),
                "second",
                Message::Redo,
            ))
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate command `test.command`")
        );
    }

    #[test]
    fn builtin_registry_contains_metadata_and_semantic_handlers() {
        let registry = CommandRegistry::builtin();
        let command = registry.get("open_theme_picker").unwrap();
        assert_eq!(command.description, "theme");
        assert!(matches!(
            registry.message_for("open_theme_picker"),
            Some(Message::OpenThemePicker)
        ));
        assert!(registry.message_for("missing.command").is_none());
    }
}
