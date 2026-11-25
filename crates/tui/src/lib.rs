pub mod capabilities;
pub mod component;
pub mod composition;
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
    agent, buffer_grep, buffer_jump, buffer_navigation, change, code_navigation, cursor_line,
    diagnostics, editing, explorer, external_sync, file_picker, line_jump, model_selector,
    navigation, notifications, page_scroll, project_grep, provider_login, theme_picker, vcs,
    view_align, welcome, yank,
};

#[cfg(test)]
mod test_utils;

pub mod data_object {

    use std::{
        borrow::Borrow,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use app::language::{LanguageServerRegistry, LanguageServers};
    use editor::{BufferId, DocumentPosition, EditorSession};
    use eggplant_agent_storage::{ModelCatalogStore, ProviderCredentialStore};

    use crate::manager::effect::FeatureCompletion;

    use crate::{
        agent::AgentState,
        buffer_grep::BufferGrepState,
        buffer_jump::BufferJumpState,
        buffer_navigation::BufferNavigationState,
        capabilities::document::{DocumentLifecycle, DocumentOpening},
        capabilities::notification::NotificationCenter,
        capabilities::openai::OpenAiProviderFactory,
        capabilities::page_scroll::PageScroll,
        change::ChangeState,
        code_navigation::CodeNavigationState,
        command_menu::CommandMenuState,
        component::{editor::EditorViewOptions, welcome::WelcomeState},
        cursor_line::CursorLineState,
        diagnostics::DiagnosticsState,
        explorer::ExplorerState,
        file_picker::FilePickerState,
        line_jump::LineJumpState,
        model_selector::ModelSelectorState,
        project_grep::ProjectGrepState,
        provider_login::ProviderLoginState,
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
    pub enum GeneralAction {
        Next,
        Previous,
        Activate,
        Back,
    }

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
        pub editor_view: EditorViewOptions,
        pub cursor_line: CursorLineState,
        pub welcome: WelcomeState,
        pub language_servers: Arc<dyn LanguageServerRegistry>,

        // Fixed workspace and its derived, visible filesystem tree.
        pub workspace: Option<Workspace>,
        pub explorer: ExplorerState,

        // Shared shell for independent search, agent, and info tools.
        pub tools: ToolsPanelState,

        // Focus/input routing. Panel visibility lives in each feature's state.
        pub current_focus: FocusTarget,
        pub window_area: ratatui::layout::Rect,
        /// Transient workflows that own input; the shell context is derived when empty.
        pub context_stack: Vec<Context>,
        pub theme: Theme,
        pub theme_loading: ThemeLoadState,
        pub theme_persistence: ThemePersistenceState,

        // Transient feedback, popup, and save-dependent workflows.
        pub status: Option<StatusMessage>,
        pub notifications: NotificationCenter,
        pub pending_input: Option<PendingInput>,
        pub document_open: DocumentOpening,
        pub document_lifecycle: DocumentLifecycle,
        pub agent: AgentState,
        pub provider_login: ProviderLoginState,
        pub model_selector: ModelSelectorState,
        /// Optional durable credential capability shared by login and later model selection.
        pub provider_credentials: Option<Arc<dyn ProviderCredentialStore>>,
        /// Persistent provider model catalogs, independent of credential records.
        pub model_catalogs: Option<Arc<dyn ModelCatalogStore>>,
        /// OpenAI-compatible catalog and provider construction used by login/model selection.
        pub openai_factory: Option<Arc<dyn OpenAiProviderFactory>>,
        pub deferred_action: Option<DeferredAction>,
        pub command_menu: CommandMenuState,
        pub code_navigation: CodeNavigationState,
        pub diagnostics: DiagnosticsState,
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
                editor_view: EditorViewOptions::default(),
                cursor_line: CursorLineState::default(),
                welcome: WelcomeState::default(),
                language_servers: Arc::new(LanguageServers::default()),
                workspace: None,
                explorer: ExplorerState::default(),
                tools: ToolsPanelState::default(),
                current_focus: FocusTarget::Editor,
                window_area: ratatui::layout::Rect::default(),
                context_stack: Vec::new(),
                theme: Theme::default(),
                theme_loading: ThemeLoadState::default(),
                theme_persistence: ThemePersistenceState::default(),
                status: None,
                notifications: NotificationCenter::default(),
                pending_input: None,
                document_open: DocumentOpening::default(),
                document_lifecycle: DocumentLifecycle::default(),
                agent: AgentState::default(),
                provider_login: ProviderLoginState::default(),
                model_selector: ModelSelectorState::default(),
                provider_credentials: None,
                model_catalogs: None,
                openai_factory: None,
                deferred_action: None,
                command_menu: CommandMenuState::default(),
                code_navigation: CodeNavigationState::default(),
                diagnostics: DiagnosticsState::default(),
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
        CommandMenuStart,
        CommandMenuKey(crate::manager::context::KeyChord),
        InvokeCommand(CommandId),
        GeneralAction(GeneralAction),
        FeatureCompletion(FeatureCompletion),
        TextInput(crate::component::text_input::TextInputEvent),
        FocusLost,
        ExternalFileChanged {
            path: PathBuf,
            kind: crate::external_sync::ExternalChangeKind,
        },
        Resize(u16, u16),
        GotoBuffer(usize),
    }

    impl Message {
        /// Route a typed worker result to its owning feature's completion handler.
        pub fn completion(completion: impl crate::manager::effect::Completion) -> Self {
            Self::FeatureCompletion(FeatureCompletion::new(completion))
        }
    }

    /// Input-routing identity for a transient workflow or derived shell surface.
    #[derive(Debug, Default, Hash, Clone, Copy, PartialEq, Eq)]
    pub enum Context {
        #[default]
        AppNormal,
        Welcome,
        AgentChat,
        AgentChatInsert,
        AgentChatHelp,
        AgentChatDialog,
        AgentApproval,
        ProviderLogin,
        ModelSelector,
        VCS,
        VcsHistory,
        VcsCommit,
        VcsCommitInput,
        VcsMerge,
        VcsResetMenu,
        VcsReset,
        VcsBranchMenu,
        VcsBranch,
        EditorInput,
        EditorVisual,
        EditorChange,
        EditorYank,
        Explorer,
        ExplorerHelp,
        ExplorerAdd,
        ExplorerDelete,
        ExplorerTransfer,
        CodeNavigation,
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
