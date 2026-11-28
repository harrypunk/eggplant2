use crate::RunId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolId(String);

impl ToolId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for ToolId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolConcurrency {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolDescriptor {
    pub id: ToolId,
    pub description: String,
    pub parameters: serde_json::Value,
    pub concurrency: ToolConcurrency,
}

impl ToolDescriptor {
    pub fn new(
        id: impl Into<ToolId>,
        description: impl Into<String>,
        parameters: serde_json::Value,
    ) -> Self {
        Self {
            id: id.into(),
            description: description.into(),
            parameters,
            concurrency: ToolConcurrency::Shared,
        }
    }

    pub fn exclusive(mut self) -> Self {
        self.concurrency = ToolConcurrency::Exclusive;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolResultStatus {
    Success,
    Error,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolResult {
    pub call_id: String,
    pub status: ToolResultStatus,
    pub content: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    pub run_id: RunId,
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub result: Option<ToolResult>,
}
