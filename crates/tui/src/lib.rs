pub mod component;
pub mod config;
pub mod manager;
pub mod page;
pub mod terminal;

pub mod data_object {

    #[derive(Debug, Default)]
    pub struct Model {
        pub running_state: RunningState,
        pub edit_mode: EditMode,
        pub current_focus: FocusTarget,
        pub context_stack: Vec<Context>,
    }

    #[derive(Debug, Default, PartialEq, Eq)]
    pub enum RunningState {
        #[default]
        Running,
        Done,
    }

    // vim style edit mode
    #[derive(Debug, Default, PartialEq, Eq)]
    pub enum EditMode {
        #[default]
        Normal,
        Insert,
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
