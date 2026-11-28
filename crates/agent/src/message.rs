use crate::{RunId, ToolCall};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderMessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderMessage {
    pub role: ProviderMessageRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ProviderMessage {
    pub fn user(content: String) -> Self {
        Self {
            role: ProviderMessageRole::User,
            content,
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: String, tool_calls: Vec<ToolCall>) -> Self {
        Self {
            role: ProviderMessageRole::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        }
    }

    pub fn tool(call_id: String, content: String) -> Self {
        Self {
            role: ProviderMessageRole::Tool,
            content,
            tool_calls: Vec::new(),
            tool_call_id: Some(call_id),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessagePart {
    Thinking(String),
    Answer(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub run_id: RunId,
    /// Answer text for assistant messages; original text for user messages.
    pub text: String,
    pub(crate) parts: Vec<MessagePart>,
}

impl ChatMessage {
    pub fn parts(&self) -> &[MessagePart] {
        &self.parts
    }

    /// Rehydrate answer-only display text without inventing reasoning content.
    pub fn restored(role: MessageRole, run_id: RunId, text: String) -> Self {
        let parts = match role {
            MessageRole::Assistant if !text.is_empty() => {
                vec![MessagePart::Answer(text.clone())]
            }
            MessageRole::User | MessageRole::Assistant => Vec::new(),
        };
        Self {
            role,
            run_id,
            text,
            parts,
        }
    }
}
