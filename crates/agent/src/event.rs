use crate::{AgentContinuation, TokenUsage, ToolApproval, ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCalls(Vec<ToolCall>),
    ToolResults(Vec<ToolResult>),
    Usage(TokenUsage),
    ApprovalRequired {
        approval: ToolApproval,
        continuation: AgentContinuation,
    },
    Complete,
    Failed(String),
}
