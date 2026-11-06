pub mod component;
pub mod config;
pub mod explorer;
pub mod manager;
pub mod page;
pub mod startup;
pub mod terminal;

#[cfg(test)]
mod test_utils;

pub mod data_object {

    use std::path::{Path, PathBuf};

    use editor::{CursorMovement, SaveCompletion};

    #[derive(Debug, Default)]
    pub struct Model {
        pub running_state: RunningState,
        pub editor: editor::EditorSession,
        pub workspace: Option<Workspace>,
        pub explorer: crate::explorer::ExplorerState,
        pub current_focus: FocusTarget,
        pub context_stack: Vec<Context>,
        pub status: Option<StatusMessage>,
        pub deferred_action: Option<DeferredAction>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeferredAction {
        Quit,
        CloseBuffer(editor::BufferId),
    }

    /// A workspace whose root is fixed after startup.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Workspace {
        root: PathBuf,
    }

    impl Workspace {
        pub(crate) fn new(root: PathBuf) -> Self {
            Self { root }
        }

        pub fn root(&self) -> &Path {
            &self.root
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StatusKind {
        Info,
        Error,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct StatusMessage {
        pub kind: StatusKind,
        pub text: String,
    }

    impl StatusMessage {
        pub fn info(text: impl Into<String>) -> Self {
            Self {
                kind: StatusKind::Info,
                text: text.into(),
            }
        }

        pub fn error(text: impl Into<String>) -> Self {
            Self {
                kind: StatusKind::Error,
                text: text.into(),
            }
        }
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    pub enum RunningState {
        #[default]
        Running,
        Done,
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    pub enum FocusTarget {
        #[default]
        Editor,
        FileExplorer,
        ToolsPanel,
        AgentChatDialog,
    }

    #[derive(Debug, Clone)]
    pub enum Message {
        Quit,
        EnterInsertMode,
        ExitInsertMode,
        MoveCursor(CursorMovement),
        InsertChar(char),
        InsertNewline,
        Backspace,
        Undo,
        Redo,
        CloseActive,
        NextBuffer,
        PreviousBuffer,
        SaveFinished(SaveCompletion),
        FocusLost,
        Resize(u16, u16),
        ExplorerNext,
        ExplorerPrevious,
        OpenSelected,
        ExplorerFold,
        ExplorerUnfold,
        RefreshExplorer,
        ToggleExplorerFocus,
        GotoBuffer(usize),
    }

    // context for input events
    #[derive(Debug, Default, Hash, Clone, Copy, PartialEq, Eq)]
    pub enum Context {
        #[default]
        AppNormal,
        AgentChat,
        AgentChatDialog,
        VCS,
        EditorInput,
        Explorer,
    }
}
