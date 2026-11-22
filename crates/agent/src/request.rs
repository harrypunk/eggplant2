use crate::{CancellationToken, ProviderMessage};

#[derive(Debug)]
pub struct AgentRequest {
    pub prompt: String,
    pub history: Vec<ProviderMessage>,
    pub cancellation: CancellationToken,
}

impl AgentRequest {
    pub fn new(prompt: String, cancellation: CancellationToken) -> Self {
        Self {
            prompt,
            history: Vec::new(),
            cancellation,
        }
    }

    pub fn follow_up(history: Vec<ProviderMessage>, cancellation: CancellationToken) -> Self {
        Self {
            prompt: String::new(),
            history,
            cancellation,
        }
    }
}
