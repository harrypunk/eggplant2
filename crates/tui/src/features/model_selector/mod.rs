mod state;
mod ui;
mod work;

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] =
    &[
        ContextKeymap::new("model_selector", Context::ModelSelector).with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ]),
    ];

pub use state::{ModelSelectorNotice, ModelSelectorPage, ModelSelectorState, ProviderModel};
pub use ui::register_ui;
pub use work::{close, commands, general_action_subscriptions, handle_command, open};
