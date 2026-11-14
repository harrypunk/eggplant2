pub mod buffer;
pub mod buffer_jump;
pub mod command_menu;
pub mod component;
pub mod config;
pub mod document;
pub mod editing;
pub mod explorer;
pub mod file_picker;
mod filesystem;
pub mod focus;
pub mod lifecycle;
pub mod manager;
pub mod page;
pub mod page_scroll;
pub mod runtime;
pub mod startup;
pub mod terminal;
pub mod theme;
pub mod theme_picker;
pub mod tools;
pub mod ui;

#[cfg(test)]
mod test_utils;

pub mod data_object {

    use std::{
        borrow::Borrow,
        path::{Path, PathBuf},
    };

    use editor::{BufferId, DocumentOpenCompletion, EditorSession, SaveCompletion};

    use crate::manager::effect::FeatureCompletion;

    use crate::{
        buffer_jump::BufferJumpState,
        command_menu::CommandMenuState,
        explorer::ExplorerState,
        file_picker::{FileEntry, FilePickerState, PreviewState},
        page_scroll::PageScrollState,
        theme::Theme,
        theme_picker::ThemePickerState,
        tools::ToolsPanelState,
    };

    /// Stable semantic command identity shared by keymaps and feature handlers.
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
    /// ├── FilePickerState        query, ranked files, selection, text preview
    /// ├── UI state               panel visibility, focus, input contexts
    /// └── Transient workflows    status, deferred actions, command menu
    /// ```
    ///
    /// `MessageManager` applies messages to this state and returns effects. Pages
    /// and components only read it (or an editor snapshot) to render the UI.
    #[derive(Debug)]
    pub struct Model {
        // Application lifecycle.
        pub running_state: RunningState,

        // Editor-owned document and editing state.
        pub editor: EditorSession,

        // Fixed workspace and its derived, visible filesystem tree.
        pub workspace: Option<Workspace>,
        pub explorer: ExplorerState,

        // Shared shell for independent search, agent, and info tools.
        pub tools: ToolsPanelState,

        // Focus/input routing. Panel visibility lives in each feature's state.
        pub current_focus: FocusTarget,
        pub context_stack: Vec<Context>,
        pub theme: Theme,

        // Transient feedback, popup, and save-dependent workflows.
        pub status: Option<StatusMessage>,
        pub opening_file: Option<PathBuf>,
        pub deferred_action: Option<DeferredAction>,
        pub command_menu: CommandMenuState,
        pub file_picker: FilePickerState,
        pub theme_picker: ThemePickerState,
        pub page_scroll: PageScrollState,
        pub buffer_jump: BufferJumpState,
        #[cfg(test)]
        pub example_feature: crate::test_utils::ExampleFeatureState,
    }

    impl Default for Model {
        fn default() -> Self {
            Self {
                running_state: RunningState::Running,
                editor: EditorSession::default(),
                workspace: None,
                explorer: ExplorerState::default(),
                tools: ToolsPanelState::default(),
                current_focus: FocusTarget::Editor,
                context_stack: Vec::new(),
                theme: Theme::default(),
                status: None,
                opening_file: None,
                deferred_action: None,
                command_menu: CommandMenuState::default(),
                file_picker: FilePickerState::default(),
                theme_picker: ThemePickerState::default(),
                page_scroll: PageScrollState::default(),
                buffer_jump: BufferJumpState::default(),
                #[cfg(test)]
                example_feature: crate::test_utils::ExampleFeatureState::default(),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeferredAction {
        Quit,
        CloseBuffer(BufferId),
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
        FilePicker,
        AgentChatDialog,
    }

    #[derive(Debug)]
    pub enum Message {
        CommandMenuKey(crossterm::event::KeyCode),
        InvokeCommand(CommandId),
        FeatureCompletion(FeatureCompletion),
        TextInput(char),
        SaveFinished(SaveCompletion),
        FileOpened(DocumentOpenCompletion),
        FocusLost,
        Resize(u16, u16),
        OpenSelected,
        FileScanFinished {
            revision: u64,
            result: Result<Vec<FileEntry>, String>,
        },
        FileFilterFinished {
            revision: u64,
            results: Vec<FileEntry>,
        },
        FilePreviewFinished {
            revision: u64,
            preview: PreviewState,
        },
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
        FilePicker,
        ThemePicker,
        BufferJump,
    }
}
