use crate::data_object::Context;

/// Local visibility and parent-dialog state for the provider-login workflow.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ProviderLoginState {
    open: bool,
    return_context: Option<Context>,
}

impl ProviderLoginState {
    pub fn is_open(self) -> bool {
        self.open
    }

    pub fn open(&mut self, return_context: Option<Context>) {
        self.open = true;
        self.return_context = return_context;
    }

    pub fn close(&mut self) -> Option<Context> {
        self.open = false;
        self.return_context.take()
    }
}
