mod component;
mod state;
mod ui;
mod work;

use crate::{
    data_object::Context,
    manager::keymap_contribution::{ContextKeymap, KeymapCapability},
};

pub const KEYMAPS: &[ContextKeymap] =
    &[
        ContextKeymap::new("provider_logout", Context::ProviderLogout).with_capabilities(&[
            KeymapCapability::ListNavigation,
            KeymapCapability::ModalActions,
        ]),
    ];

pub use state::{ProviderLogoutLoad, ProviderLogoutPage, ProviderLogoutState};
pub use ui::register_ui;
pub use work::{close, commands, general_action_subscriptions, handle_general_action, open};
