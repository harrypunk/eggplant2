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

    pub fn from_commands(commands: impl IntoIterator<Item = Command>) -> color_eyre::Result<Self> {
        commands
            .into_iter()
            .try_fold(Self::empty(), |mut registry, command| {
                registry.register(command)?;
                Ok(registry)
            })
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
        Self::from_commands(builtin_commands()).expect("built-in command IDs must be unique")
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

fn builtin_commands() -> impl IntoIterator<Item = Command> {
    [
        Command::new(CommandId("quit"), "quit", Message::Quit),
        Command::new(
            CommandId("enter_insert_mode"),
            "enter Insert mode",
            Message::EnterInsertMode,
        ),
        Command::new(
            CommandId("exit_insert_mode"),
            "leave Insert mode",
            Message::ExitInsertMode,
        ),
        Command::new(
            CommandId("move_left"),
            "move left",
            Message::MoveCursor(CursorMovement::Left),
        ),
        Command::new(
            CommandId("move_right"),
            "move right",
            Message::MoveCursor(CursorMovement::Right),
        ),
        Command::new(
            CommandId("move_up"),
            "move up",
            Message::MoveCursor(CursorMovement::Up),
        ),
        Command::new(
            CommandId("move_down"),
            "move down",
            Message::MoveCursor(CursorMovement::Down),
        ),
        Command::new(
            CommandId("focus_left"),
            "focus left",
            Message::MoveFocus(FocusDirection::Left),
        ),
        Command::new(
            CommandId("focus_down"),
            "focus down",
            Message::MoveFocus(FocusDirection::Down),
        ),
        Command::new(
            CommandId("focus_up"),
            "focus up",
            Message::MoveFocus(FocusDirection::Up),
        ),
        Command::new(
            CommandId("focus_right"),
            "focus right",
            Message::MoveFocus(FocusDirection::Right),
        ),
        Command::new(CommandId("backspace"), "backspace", Message::Backspace),
        Command::new(
            CommandId("insert_newline"),
            "insert newline",
            Message::InsertNewline,
        ),
        Command::new(CommandId("undo"), "undo", Message::Undo),
        Command::new(CommandId("redo"), "redo", Message::Redo),
        Command::new(
            CommandId("close_buffer"),
            "close buffer",
            Message::CloseActive,
        ),
        Command::new(CommandId("next_buffer"), "next buffer", Message::NextBuffer),
        Command::new(
            CommandId("previous_buffer"),
            "previous buffer",
            Message::PreviousBuffer,
        ),
        Command::new(
            CommandId("explorer_next"),
            "next entry",
            Message::ExplorerNext,
        ),
        Command::new(
            CommandId("explorer_previous"),
            "previous entry",
            Message::ExplorerPrevious,
        ),
        Command::new(
            CommandId("open_selected"),
            "open selected",
            Message::OpenSelected,
        ),
        Command::new(
            CommandId("explorer_fold"),
            "fold folder",
            Message::ExplorerFold,
        ),
        Command::new(
            CommandId("explorer_unfold"),
            "unfold folder",
            Message::ExplorerUnfold,
        ),
        Command::new(
            CommandId("refresh_explorer"),
            "refresh explorer",
            Message::RefreshExplorer,
        ),
        Command::new(
            CommandId("toggle_ignored_files"),
            "toggle ignored files",
            Message::ToggleIgnoredFiles,
        ),
        Command::new(
            CommandId("toggle_file_explorer"),
            "toggle files",
            Message::ToggleFileExplorer,
        ),
        Command::new(
            CommandId("open_file_picker"),
            "find file",
            Message::OpenFilePicker,
        ),
        Command::new(
            CommandId("close_file_picker"),
            "close file picker",
            Message::CloseFilePicker,
        ),
        Command::new(
            CommandId("file_picker_backspace"),
            "delete query character",
            Message::FilePickerBackspace,
        ),
        Command::new(
            CommandId("open_picker_file"),
            "open file",
            Message::OpenPickerFile,
        ),
        Command::new(
            CommandId("file_picker_next"),
            "next file",
            Message::FilePickerNext,
        ),
        Command::new(
            CommandId("file_picker_previous"),
            "previous file",
            Message::FilePickerPrevious,
        ),
        Command::new(
            CommandId("open_theme_picker"),
            "theme",
            Message::OpenThemePicker,
        ),
        Command::new(
            CommandId("close_theme_picker"),
            "close theme picker",
            Message::CloseThemePicker,
        ),
        Command::new(
            CommandId("theme_picker_next"),
            "next theme",
            Message::ThemePickerNext,
        ),
        Command::new(
            CommandId("theme_picker_previous"),
            "previous theme",
            Message::ThemePickerPrevious,
        ),
        Command::new(CommandId("apply_theme"), "apply theme", Message::ApplyTheme),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_command_ids() {
        let error = CommandRegistry::from_commands([
            Command::new(CommandId("test.command"), "first", Message::Undo),
            Command::new(CommandId("test.command"), "second", Message::Redo),
        ])
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
