use crate::{AgentContinuation, ToolApproval, ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCalls(Vec<ToolCall>),
    ToolResults(Vec<ToolResult>),
    ApprovalRequired {
        approval: ToolApproval,
        continuation: AgentContinuation,
    },
    Complete,
    Failed(String),
}
