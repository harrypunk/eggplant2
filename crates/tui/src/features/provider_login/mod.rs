mod openai;
mod openai_work;
mod paste;
mod state;
mod storage;
mod ui;
mod work;

pub use openai::{DEFAULT_OPENAI_ENDPOINT, OpenAiField};
pub use state::{
    ProviderAuthMethod, ProviderLoginBack, ProviderLoginOption, ProviderLoginPage,
    ProviderLoginState, ProviderLoginSubmission, ProviderSpec,
};
pub use ui::register_ui;
pub use work::{commands, handle_command, handle_general_action, handle_input, open};
