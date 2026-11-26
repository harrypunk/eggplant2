mod openai;
mod openai_work;
mod paste;
mod state;
mod storage;
mod ui;
mod work;

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] =
    &[ContextKeymap::new("provider_login", Context::ProviderLogin)
        .with_commands(&[
            ("provider_login_focus_next", "tab"),
            ("provider_login_paste", "ctrl-v"),
        ])
        .with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ])];

pub use openai::{DEFAULT_OPENAI_ENDPOINT, OpenAiField};
pub use state::{
    ProviderAuthMethod, ProviderLoginBack, ProviderLoginOption, ProviderLoginPage,
    ProviderLoginState, ProviderLoginSubmission, ProviderSpec,
};
pub use ui::register_ui;
pub use work::{
    commands, general_action_subscriptions, handle_command, handle_general_action, handle_input,
    open,
};
