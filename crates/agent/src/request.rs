use crate::{CancellationToken, ProviderMessage, ThinkingId};

#[derive(Debug)]
pub struct AgentRequest {
    pub prompt: String,
    pub history: Vec<ProviderMessage>,
    pub cancellation: CancellationToken,
    /// Provider-specific reasoning option; absent preserves the provider default.
    pub thinking_id: Option<ThinkingId>,
}

impl AgentRequest {
    pub fn new(prompt: String, cancellation: CancellationToken) -> Self {
        Self {
            prompt,
            history: Vec::new(),
            cancellation,
            thinking_id: None,
        }
    }

    pub fn follow_up(history: Vec<ProviderMessage>, cancellation: CancellationToken) -> Self {
        Self {
            prompt: String::new(),
            history,
            cancellation,
            thinking_id: None,
        }
    }

    pub fn with_thinking_id(mut self, id: ThinkingId) -> Self {
        self.thinking_id = Some(id);
        self
    }
}
