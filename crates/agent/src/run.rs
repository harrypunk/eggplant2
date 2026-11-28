use crate::ProviderMessage;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(String);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for SessionId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for SessionId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunStatus {
    Waiting,
    Streaming,
    Complete,
    Failed(String),
    Cancelled,
}

impl RunStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Waiting | Self::Streaming)
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Waiting => "Waiting for assistant",
            Self::Streaming => "Streaming",
            Self::Complete => "Complete",
            Self::Failed(_) => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRun {
    pub id: RunId,
    pub status: RunStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContinuation {
    pub step: usize,
    pub history: Vec<ProviderMessage>,
    pub tool_calls: usize,
    pub tool_output: usize,
}
