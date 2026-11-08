pub mod component;
pub mod config;
pub mod explorer;
pub mod leader;
pub mod manager;
pub mod page;
pub mod startup;
pub mod terminal;
pub mod tools;

#[cfg(test)]
mod test_utils;

pub mod data_object {

    use std::path::{Path, PathBuf};

    use editor::{CursorMovement, SaveCompletion};

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Visibility {
        Displayed,
        Hidden,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FocusState {
        Focused,
        Unfocused,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BufferMovement {
        Next,
        Previous,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FocusDirection {
        Left,
        Down,
        Up,
        Right,
    }

    /// Central application state used by the Elm-style message/update/view loop.
    ///
    /// `Model` composes state from each domain rather than duplicating its data:
    ///
    /// ```text
    /// Model
    /// ├── EditorSession          buffers, cursor, edit mode, viewport, saves
    /// ├── Workspace + Explorer   fixed root, visible tree, selection, expansion
    /// ├── ToolsPanelState        side-panel visibility and active tool
    /// ├── UI state               panel visibility, focus, input contexts
    /// └── Transient workflows    status, deferred actions, leader sequences
    /// ```
    ///
    /// `MessageManager` applies messages to this state and returns effects. Pages
    /// and components only read it (or an editor snapshot) to render the UI.
    #[derive(Debug)]
    pub struct Model {
        // Application lifecycle.
        pub running_state: RunningState,

        // Editor-owned document and editing state.
        pub editor: editor::EditorSession,

        // Fixed workspace and its derived, visible filesystem tree.
        pub workspace: Option<Workspace>,
        pub explorer: crate::explorer::ExplorerState,

        // Shared shell for independent search, agent, and info tools.
        pub tools: crate::tools::ToolsPanelState,

        // Layout visibility and focus/input routing. Explorer visibility stays
        // separate from filesystem-tree behavior so hiding it retains the tree.
        // A future ExplorerPanelState wrapper can group these if useful.
        pub explorer_visibility: Visibility,
        pub current_focus: FocusTarget,
        pub context_stack: Vec<Context>,

        // Transient feedback and save-dependent workflows.
        pub status: Option<StatusMessage>,
        pub deferred_action: Option<DeferredAction>,
        pub leader: crate::leader::LeaderState,
    }

    impl Default for Model {
        fn default() -> Self {
            Self {
                running_state: RunningState::Running,
                editor: editor::EditorSession::default(),
                workspace: None,
                explorer: crate::explorer::ExplorerState::default(),
                tools: crate::tools::ToolsPanelState::default(),
                explorer_visibility: Visibility::Displayed,
                current_focus: FocusTarget::Editor,
                context_stack: Vec::new(),
                status: None,
                deferred_action: None,
                leader: crate::leader::LeaderState::default(),
            }
        }
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

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum FocusTarget {
        #[default]
        Editor,
        FileExplorer,
        ToolsPanel,
        AgentChatDialog,
    }

    #[derive(Debug, Clone)]
    pub enum Message {
        LeaderKey(crossterm::event::KeyCode),
        Quit,
        EnterInsertMode,
        ExitInsertMode,
        MoveCursor(CursorMovement),
        MoveFocus(FocusDirection),
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
        ToggleIgnoredFiles,
        ToggleFileExplorer,
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
