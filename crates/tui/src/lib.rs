pub mod component;
pub mod manager;
pub mod page;
pub mod terminal;

pub mod data_object {

    #[derive(Debug, Default)]
    pub struct Model {
        pub running_state: RunningState,
        pub edit_mode: EditMode,
        pub current_focus: FocusTarget,
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

    #[derive(PartialEq)]
    pub enum Message {
        Quit,
        EnterInsertMode,
        ExitInsertMode,
    }
}
