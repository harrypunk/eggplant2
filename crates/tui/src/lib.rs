pub mod capabilities;
pub mod component;
pub mod config;
pub mod features;
pub mod manager;
pub mod page;
pub mod runtime;
pub mod shell;
pub mod startup;
pub mod terminal;
pub mod theme;
pub mod tools;
pub mod ui;

pub(crate) use capabilities::filesystem;
pub use capabilities::{clipboard, copy_hash, document, file_watcher, register, search};
pub use shell::{buffer, command_menu, focus, lifecycle};

pub use features::{
    buffer_grep, buffer_jump, buffer_navigation, change, editing, explorer, external_sync,
    file_picker, line_jump, navigation, notifications, page_scroll, project_grep, theme_picker,
    vcs, view_align, yank,
};

#[cfg(test)]
mod test_utils;

pub mod data_object {

    use std::{
        borrow::Borrow,
        path::{Path, PathBuf},
    };

    use editor::{BufferId, DocumentPosition, EditorSession, SaveCompletion};

    use crate::manager::effect::FeatureCompletion;

    use crate::{
        buffer_grep::BufferGrepState,
        buffer_jump::BufferJumpState,
        buffer_navigation::BufferNavigationState,
        capabilities::document::DocumentOpening,
        capabilities::notification::NotificationCenter,
        capabilities::page_scroll::PageScroll,
        change::ChangeState,
        command_menu::CommandMenuState,
        explorer::ExplorerState,
        file_picker::FilePickerState,
        line_jump::LineJumpState,
        project_grep::ProjectGrepState,
        register::RegisterState,
        theme::{Theme, ThemeLoadState, ThemePersistenceState},
        theme_picker::ThemePickerState,
        tools::ToolsPanelState,
        vcs::VcsState,
        view_align::ViewAlignSession,
        yank::YankState,
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OpenLocation {
        pub path: PathBuf,
        pub position: Option<DocumentPosition>,
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
        pub active_page: AppPage,

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
        pub theme_loading: ThemeLoadState,
        pub theme_persistence: ThemePersistenceState,

        // Transient feedback, popup, and save-dependent workflows.
        pub status: Option<StatusMessage>,
        pub notifications: NotificationCenter,
        pub pending_input: Option<PendingInput>,
        pub document_open: DocumentOpening,
        pub deferred_action: Option<DeferredAction>,
        pub command_menu: CommandMenuState,
        pub modal: ModalController,
        pub file_picker: FilePickerState,
        pub theme_picker: ThemePickerState,
        pub page_scroll: PageScroll,
        pub line_jump: LineJumpState,
        pub change: ChangeState,
        pub register: RegisterState,
        pub yank: YankState,
        pub buffer_jump: BufferJumpState,
        pub buffer_navigation: BufferNavigationState,
        pub buffer_grep: BufferGrepState,
        pub project_grep: ProjectGrepState,
        pub view_align: Option<ViewAlignSession>,
        pub vcs: VcsState,
        #[cfg(test)]
        pub example_feature: crate::test_utils::ExampleFeatureState,
    }

    impl Default for Model {
        fn default() -> Self {
            Self {
                running_state: RunningState::Running,
                active_page: AppPage::Editor,
                editor: EditorSession::default(),
                workspace: None,
                explorer: ExplorerState::default(),
                tools: ToolsPanelState::default(),
                current_focus: FocusTarget::Editor,
                context_stack: Vec::new(),
                theme: Theme::default(),
                theme_loading: ThemeLoadState::default(),
                theme_persistence: ThemePersistenceState::default(),
                status: None,
                notifications: NotificationCenter::default(),
                pending_input: None,
                document_open: DocumentOpening::default(),
                deferred_action: None,
                command_menu: CommandMenuState::default(),
                modal: ModalController::default(),
                file_picker: FilePickerState::default(),
                theme_picker: ThemePickerState::default(),
                page_scroll: PageScroll::default(),
                line_jump: LineJumpState::default(),
                change: ChangeState::default(),
                register: RegisterState::default(),
                yank: YankState::default(),
                buffer_jump: BufferJumpState::default(),
                buffer_navigation: BufferNavigationState::default(),
                buffer_grep: BufferGrepState::default(),
                project_grep: ProjectGrepState::default(),
                view_align: None,
                vcs: VcsState::default(),
                #[cfg(test)]
                example_feature: crate::test_utils::ExampleFeatureState::default(),
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeferredAction {
        Quit,
        CloseBuffer(BufferId),
        CloseOthers(BufferId),
        CloseRight(BufferId),
        CloseAll(BufferId),
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

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct PendingInput {
        owner: Context,
        text: String,
    }

    impl PendingInput {
        pub fn new(owner: Context, text: impl Into<String>) -> Self {
            Self {
                owner,
                text: text.into(),
            }
        }

        pub fn text(&self) -> &str {
            &self.text
        }

        pub fn is_owner(&self, owner: Context) -> bool {
            self.owner == owner
        }

        pub(crate) fn clear(slot: &mut Option<Self>, owner: Context) {
            if slot.as_ref().is_some_and(|pending| pending.is_owner(owner)) {
                *slot = None;
            }
        }
    }

    #[derive(Debug, Default)]
    pub struct ModalController {
        active: Option<Context>,
    }

    impl ModalController {
        pub fn open(&mut self, context: Context) -> bool {
            if self.active.is_some_and(|active| active != context) {
                return false;
            }
            self.active = Some(context);
            true
        }

        pub fn close(&mut self, context: Context) {
            if self.active == Some(context) {
                self.active = None;
            }
        }

        pub fn is_active(&self, context: Context) -> bool {
            self.active == Some(context)
        }
    }

    #[cfg(test)]
    mod modal_tests {
        use super::*;

        #[test]
        fn one_modal_owns_input_until_its_owner_closes() {
            let mut modal = ModalController::default();
            assert!(modal.open(Context::FilePicker));
            assert!(!modal.open(Context::ThemePicker));
            modal.close(Context::ThemePicker);
            assert!(modal.is_active(Context::FilePicker));
            modal.close(Context::FilePicker);
            assert!(modal.open(Context::ThemePicker));
        }
    }

    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub enum AppPage {
        #[default]
        Editor,
        Vcs,
        VcsHistory,
        VcsCommit,
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
        FileOpened(crate::document::OpenFileCompletion),
        FocusLost,
        ExternalFileChanged {
            path: PathBuf,
            kind: crate::external_sync::ExternalChangeKind,
        },
        Resize(u16, u16),
        OpenSelected,
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
        VcsHistory,
        VcsCommit,
        VcsCommitInput,
        VcsMerge,
        VcsBranchMenu,
        VcsBranch,
        EditorInput,
        EditorVisual,
        EditorChange,
        EditorYank,
        Explorer,
        FilePicker,
        ThemePicker,
        BufferJump,
        BufferNavigation,
        BufferGrep,
        ProjectGrep,
        LineJump,
        ViewAlign,
    }
}
