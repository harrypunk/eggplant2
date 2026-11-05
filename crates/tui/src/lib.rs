pub mod component;
pub mod config;
pub mod manager;
pub mod page;
pub mod startup;
pub mod terminal;

pub mod data_object {

    use std::path::PathBuf;

    use editor::CursorMovement;

    #[derive(Debug, Default)]
    pub struct Model {
        pub running_state: RunningState,
        pub editor: editor::EditorSession,
        pub workspace: Option<Workspace>,
        pub current_focus: FocusTarget,
        pub context_stack: Vec<Context>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Workspace {
        pub root: PathBuf,
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

    #[derive(Debug, Clone, Copy, PartialEq)]
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
        Resize(u16, u16),
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
    }
}
