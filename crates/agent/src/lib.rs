//! Provider-neutral agent session, message, and run-event contracts.
//!
//! This crate intentionally has no editor, terminal, filesystem, or provider SDK
//! dependencies. Frontends own presentation and effect scheduling; providers own
//! model I/O.

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_MESSAGES: usize = 100;

#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

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
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for AgentError {}

/// Object-safe provider port for ordered assistant events.
pub trait AgentProvider: std::fmt::Debug + Send + Sync {
    fn stream<'a>(
        &'a self,
        request: AgentRequest,
        on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
    ) -> Pin<Box<dyn Future<Output = Result<(), AgentError>> + Send + 'a>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    ToolCalls(Vec<ToolCall>),
    ToolResults(Vec<ToolResult>),
    Complete,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEvent {
    pub call_id: String,
    pub name: String,
    pub arguments: String,
    pub result: Option<ToolResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub run_id: RunId,
    pub text: String,
}

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

#[derive(Debug, Default)]
pub struct AgentSession {
    next_id: u64,
    run: Option<AgentRun>,
    messages: Vec<ChatMessage>,
    tool_events: Vec<ToolEvent>,
}

impl AgentSession {
    pub fn begin(&mut self, prompt: String) -> Option<RunId> {
        if prompt.trim().is_empty() || self.run.as_ref().is_some_and(|run| run.status.is_running())
        {
            return None;
        }
        self.next_id = self.next_id.wrapping_add(1);
        let id = RunId(self.next_id);
        self.push(ChatMessage {
            role: MessageRole::User,
            run_id: id,
            text: prompt,
        });
        self.run = Some(AgentRun {
            id,
            status: RunStatus::Waiting,
        });
        Some(id)
    }

    pub fn append_delta(&mut self, id: RunId, delta: &str) -> bool {
        let Some(run) = self
            .run
            .as_mut()
            .filter(|run| run.id == id && run.status.is_running())
        else {
            return false;
        };
        run.status = RunStatus::Streaming;
        if let Some(message) = self
            .messages
            .iter_mut()
            .rev()
            .find(|message| message.run_id == id && message.role == MessageRole::Assistant)
        {
            message.text.push_str(delta);
        } else {
            self.push(ChatMessage {
                role: MessageRole::Assistant,
                run_id: id,
                text: delta.to_string(),
            });
        }
        true
    }

    pub fn complete(&mut self, id: RunId) -> bool {
        self.finish(id, RunStatus::Complete)
    }

    pub fn fail(&mut self, id: RunId, error: String) -> bool {
        self.finish(id, RunStatus::Failed(error))
    }

    pub fn cancel(&mut self) -> bool {
        let Some(run) = self.run.as_mut().filter(|run| run.status.is_running()) else {
            return false;
        };
        run.status = RunStatus::Cancelled;
        true
    }

    fn finish(&mut self, id: RunId, status: RunStatus) -> bool {
        let Some(run) = self
            .run
            .as_mut()
            .filter(|run| run.id == id && run.status.is_running())
        else {
            return false;
        };
        run.status = status;
        true
    }

    fn push(&mut self, message: ChatMessage) {
        if self.messages.len() == MAX_MESSAGES {
            self.messages.remove(0);
        }
        self.messages.push(message);
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn run(&self) -> Option<&AgentRun> {
        self.run.as_ref()
    }

    pub fn is_running(&self) -> bool {
        self.run.as_ref().is_some_and(|run| run.status.is_running())
    }

    pub fn record_tool_call(&mut self, run_id: RunId, call: &ToolCall) -> bool {
        let Some(run) = self
            .run
            .as_mut()
            .filter(|run| run.id == run_id && run.status.is_running())
        else {
            return false;
        };
        run.status = RunStatus::Streaming;
        if self.tool_events.len() == 50 {
            self.tool_events.remove(0);
        }
        self.tool_events.push(ToolEvent {
            call_id: call.id.clone(),
            name: call.name.clone(),
            arguments: call.arguments.clone(),
            result: None,
        });
        true
    }

    pub fn complete_tool_call(&mut self, run_id: RunId, result: ToolResult) -> bool {
        if !self.run.as_ref().is_some_and(|run| {
            run.id == run_id && (run.status.is_running() || run.status == RunStatus::Cancelled)
        }) {
            return false;
        }
        let Some(event) = self
            .tool_events
            .iter_mut()
            .rev()
            .find(|event| event.call_id == result.call_id)
        else {
            return false;
        };
        event.result = Some(result);
        true
    }

    pub fn tool_events(&self) -> &[ToolEvent] {
        &self.tool_events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_ordered_text_and_rejects_stale_events() {
        let mut session = AgentSession::default();
        let run = session.begin("explain this".to_string()).unwrap();
        assert!(session.append_delta(run, "first"));
        assert!(session.append_delta(run, " second"));
        assert!(session.complete(run));
        assert!(!session.append_delta(run, "late"));

        let messages = session.messages();
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
        assert_eq!(messages[1].text, "first second");
        assert_eq!(session.run().unwrap().status, RunStatus::Complete);
    }

    #[test]
    fn cancel_preserves_partial_text_and_blocks_later_deltas() {
        let mut session = AgentSession::default();
        let run = session.begin("write".to_string()).unwrap();
        session.append_delta(run, "partial");
        assert!(session.cancel());
        assert!(!session.append_delta(run, "ignored"));
        assert!(!session.complete(run));
        assert_eq!(session.run().unwrap().status, RunStatus::Cancelled);
        assert_eq!(session.messages()[1].text, "partial");
    }

    #[test]
    fn empty_prompt_and_concurrent_runs_are_rejected() {
        let mut session = AgentSession::default();
        assert!(session.begin(" ".to_string()).is_none());
        session.begin("one".to_string()).unwrap();
        assert!(session.begin("two".to_string()).is_none());
    }

    #[test]
    fn tool_results_match_their_run_and_survive_cancellation() {
        let mut session = AgentSession::default();
        let first = session.begin("read".to_string()).unwrap();
        let call = ToolCall {
            id: "call-1".to_string(),
            name: "read".to_string(),
            arguments: "{}".to_string(),
        };
        let result = ToolResult {
            call_id: call.id.clone(),
            status: ToolResultStatus::Cancelled,
            content: "cancelled".to_string(),
            truncated: false,
        };
        assert!(session.record_tool_call(first, &call));
        assert!(session.cancel());
        assert!(session.complete_tool_call(first, result.clone()));
        assert_eq!(session.tool_events()[0].result, Some(result.clone()));

        let second = session.begin("read again".to_string()).unwrap();
        assert_ne!(first, second);
        assert!(!session.complete_tool_call(first, result));
        assert_eq!(session.tool_events().len(), 1);
    }
}
