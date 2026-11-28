use std::{future::Future, pin::Pin};

use crate::{AgentEvent, AgentRequest, EstimatedTokens, ThinkingOption, TokenEstimator};

/// Callback boundary for ordered provider events.
pub type AgentEventSink<'a> = Box<dyn FnMut(AgentEvent) + Send + 'a>;

/// Object-safe future returned by a provider stream.
///
/// These aliases keep the trait readable without adding an async-trait dependency.
pub type AgentStreamFuture<'a> = Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + 'a>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AgentError {}

/// Object-safe provider port for ordered assistant events.
pub trait AgentProvider: std::fmt::Debug + Send + Sync {
    /// Active model context capacity when the frontend configuration supplies it.
    fn context_window(&self) -> Option<u64> {
        None
    }

    /// Pi-style context estimation by default; providers may opt out or specialize it.
    fn token_estimator(&self) -> Option<&dyn TokenEstimator> {
        static ESTIMATOR: EstimatedTokens = EstimatedTokens;
        Some(&ESTIMATOR)
    }

    /// Display options supplied by this provider's reasoning policy.
    fn thinking_options(&self) -> &[ThinkingOption] {
        &[]
    }

    fn stream<'a>(
        &'a self,
        request: AgentRequest,
        on_event: AgentEventSink<'a>,
    ) -> AgentStreamFuture<'a>;
}
