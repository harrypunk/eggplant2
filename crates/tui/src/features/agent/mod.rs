mod adapter;
mod approval;
mod commands;
mod completion;
mod context;
mod dialog;
mod navigation;
mod panel;
mod presentation;
mod prompt;
mod prompt_command;
mod references;
mod run;
mod runtime;
mod sessions;
mod state;
mod storage;
mod surface;
mod thinking_level;
mod tools;
mod ui;
mod ui_state;
mod work;
mod write;

pub(crate) use commands::handle_input;
pub use commands::{commands, general_action_subscriptions, handle_general_action};
pub(crate) use dialog::{resume_overlay, suspend_for_overlay};
pub(crate) use panel::{close_panel_input, focus_panel_input};
pub use presentation::{dialog::AgentDialogState, panel::AgentInputMode};
pub(crate) use presentation::{dialog::DialogAction, panel::PanelAction};
pub(crate) use prompt_command::{PromptCommand, PromptCommandMovement, PromptCommandState};
pub use references::{FileReferenceStatus, FileReferenceView};
pub use runtime::{AgentApplyOutcome, AgentRuntime, RunStarted, SessionReset, SessionRestored};
pub use sessions::{SessionSelectorNotice, SessionSelectorPage, SessionSelectorState};
pub use state::AgentState;
pub use storage::{LoadedSession, load_session};
pub use thinking_level::ThinkingSelectorState;
pub use ui::register_ui;
pub(crate) use ui::render_panel;
pub(crate) use ui_state::AgentUiAction;

pub const KEYMAPS: &[ContextKeymap] = &[
    ContextKeymap::new("agent_chat_dialog", Context::AgentChatDialog)
        .with_commands(&[
            ("agent_interrupt", "ctrl-f7"),
            ("agent_command_quick_fill", "tab"),
            ("agent_scroll_up", "pageup"),
            ("agent_scroll_down", "pagedown"),
        ])
        .with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ]),
    ContextKeymap::new("agent_chat", Context::AgentChat)
        .with_commands(&[
            ("agent_panel_normal", "esc"),
            ("agent_insert", "i"),
            ("agent_help", "?"),
            ("focus_left", "ctrl-h"),
            ("focus_right", "ctrl-l"),
            ("agent_interrupt", "ctrl-f7"),
            ("agent_cursor_up", "k"),
            ("agent_cursor_down", "j"),
            ("agent_scroll_up", "pageup"),
            ("agent_scroll_down", "pagedown"),
        ])
        .with_capabilities(&[KeymapCapability::LeaderMenu]),
    ContextKeymap::new("agent_chat_insert", Context::AgentChatInsert)
        .with_commands(&[
            ("agent_interrupt", "ctrl-f7"),
            ("agent_command_quick_fill", "tab"),
            ("agent_scroll_up", "pageup"),
            ("agent_scroll_down", "pagedown"),
        ])
        .with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ]),
    ContextKeymap::new("agent_thinking_selector", Context::AgentThinkingSelector)
        .with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ]),
    ContextKeymap::new("agent_session_selector", Context::AgentSessionSelector).with_capabilities(
        &[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ],
    ),
    ContextKeymap::new("agent_chat_help", Context::AgentChatHelp)
        .with_commands(&[("agent_help", "?"), ("agent_panel_normal", "esc")]),
    ContextKeymap::new("agent_approval", Context::AgentApproval).with_commands(&[
        ("agent_close", "esc"),
        ("agent_approve", "y"),
        ("agent_reject", "n"),
        ("agent_interrupt", "ctrl-f7"),
    ]),
];

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};
use editor::BufferId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorContext {
    pub buffer: BufferId,
    pub path: std::path::PathBuf,
    pub revision: i32,
    pub line: usize,
}
