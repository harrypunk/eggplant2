use crate::{AgentContinuation, TokenUsage, ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCalls(Vec<ToolCall>),
    ToolResults(Vec<ToolResult>),
    Usage(TokenUsage),
    MutationRequested {
        call: ToolCall,
        continuation: AgentContinuation,
    },
    Complete,
    Failed(String),
}
