//! Provider-neutral agent session, message, and run-event contracts.
//!
//! This crate intentionally has no editor, terminal, filesystem, or provider SDK
//! dependencies. Frontends own presentation and effect scheduling; providers own
//! model I/O.

mod cancellation;
mod event;
mod message;
mod provider;
mod request;
mod run;
mod session;
mod thinking;
mod tool;
mod usage;

pub use cancellation::CancellationToken;
pub use event::AgentEvent;
pub use message::{ChatMessage, MessagePart, MessageRole, ProviderMessage, ProviderMessageRole};
pub use provider::{AgentError, AgentEventSink, AgentProvider, AgentStreamFuture};
pub use request::AgentRequest;
pub use run::{AgentContinuation, AgentRun, RunId, RunStatus, SessionId};
pub use session::{AgentSession, SessionRestoreError, SessionSnapshot};
pub use thinking::{ThinkingId, ThinkingOption};
pub use tool::{ToolApproval, ToolCall, ToolEvent, ToolResult, ToolResultStatus};
pub use usage::TokenUsage;
