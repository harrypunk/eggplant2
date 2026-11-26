mod openai_compatible_form;
mod openai_compatible_work;
mod paste;
mod providers;
mod state;
mod storage;
mod ui;
mod work;

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] =
    &[
        ContextKeymap::new("provider_login", Context::ProviderLogin).with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
            KeymapCapability::FormActions,
        ]),
    ];

pub use openai_compatible_form::OpenAiField;
pub use providers::kimi_for_coding::DEFAULT_ENDPOINT as DEFAULT_KIMI_FOR_CODING_ENDPOINT;
pub use providers::openai::DEFAULT_OPENAI_ENDPOINT;
pub use providers::qwen_tokenplan_individual_cn::DEFAULT_ENDPOINT as DEFAULT_QWEN_TOKENPLAN_INDIVIDUAL_CN_ENDPOINT;
pub use providers::{ProviderAuthMethod, ProviderSpec};
pub use state::{
    ProviderLoginBack, ProviderLoginOption, ProviderLoginPage, ProviderLoginState,
    ProviderLoginSubmission,
};
pub use ui::register_ui;
pub use work::{
    commands, general_action_subscriptions, handle_command, handle_general_action, handle_input,
    open,
};
