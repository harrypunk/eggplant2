use crate::{
    AgentRun, ChatMessage, ConversationContextPolicy, ConversationTurn, FullContext, MessagePart,
    MessageRole, ProviderMessage, RunId, RunStatus, SessionId, TokenUsage, ToolApproval, ToolCall,
    ToolEvent, ToolResult,
};

const MAX_MESSAGES: usize = 100;
const MAX_TOOL_EVENTS: usize = 50;

/// Provider-neutral state needed to rehydrate one finalized session.
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub id: SessionId,
    pub run: Option<AgentRun>,
    pub messages: Vec<ChatMessage>,
    pub tool_events: Vec<ToolEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRestoreError {
    RunInProgress,
}

#[derive(Debug, Default)]
pub struct AgentSession {
    id: SessionId,
    next_id: u64,
    run: Option<AgentRun>,
    messages: Vec<ChatMessage>,
    tool_events: Vec<ToolEvent>,
    pending_approval: Option<ToolApproval>,
    usage: TokenUsage,
    latest_usage: Option<TokenUsage>,
    latest_usage_run: Option<RunId>,
}

impl AgentSession {
    pub fn id(&self) -> &SessionId {
        &self.id
    }

    pub fn restore(snapshot: SessionSnapshot) -> Result<Self, SessionRestoreError> {
        let SessionSnapshot {
            id,
            run,
            messages,
            tool_events,
        } = snapshot;
        if run.as_ref().is_some_and(|run| run.status.is_running()) {
            return Err(SessionRestoreError::RunInProgress);
        }
        let next_id = run
            .iter()
            .map(|run| run.id.0)
            .chain(messages.iter().map(|message| message.run_id.0))
            .chain(tool_events.iter().map(|event| event.run_id.0))
            .max()
            .unwrap_or(0);
        let message_skip = messages.len().saturating_sub(MAX_MESSAGES);
        let tool_skip = tool_events.len().saturating_sub(MAX_TOOL_EVENTS);
        let mut session = Self {
            id,
            next_id,
            run,
            messages: messages.into_iter().skip(message_skip).collect(),
            tool_events: tool_events.into_iter().skip(tool_skip).collect(),
            pending_approval: None,
            usage: TokenUsage::default(),
            latest_usage: None,
            latest_usage_run: None,
        };
        session.restore_failure();
        Ok(session)
    }

    fn restore_failure(&mut self) {
        let Some(AgentRun {
            id,
            status: RunStatus::Failed(error),
        }) = self.run.clone()
        else {
            return;
        };
        let message = self.ensure_assistant_message(id);
        if !message
            .parts()
            .iter()
            .any(|part| matches!(part, MessagePart::Error(_)))
        {
            append_part(message, MessagePart::Error(error));
        }
    }

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
            parts: Vec::new(),
        });
        self.pending_approval = None;
        self.run = Some(AgentRun {
            id,
            status: RunStatus::Waiting,
        });
        Some(id)
    }

    pub fn append_delta(&mut self, id: RunId, delta: &str) -> bool {
        let Some(message) = self.assistant_message(id) else {
            return false;
        };
        message.text.push_str(delta);
        append_part(message, MessagePart::Answer(delta.to_string()));
        true
    }

    pub fn append_thinking_delta(&mut self, id: RunId, delta: &str) -> bool {
        let Some(message) = self.assistant_message(id) else {
            return false;
        };
        append_part(message, MessagePart::Thinking(delta.to_string()));
        true
    }

    fn assistant_message(&mut self, id: RunId) -> Option<&mut ChatMessage> {
        let run = self
            .run
            .as_mut()
            .filter(|run| run.id == id && run.status.is_running())?;
        run.status = RunStatus::Streaming;
        Some(self.ensure_assistant_message(id))
    }

    fn ensure_assistant_message(&mut self, id: RunId) -> &mut ChatMessage {
        if !self
            .messages
            .iter()
            .any(|message| message.run_id == id && message.role == MessageRole::Assistant)
        {
            self.push(ChatMessage {
                role: MessageRole::Assistant,
                run_id: id,
                text: String::new(),
                parts: Vec::new(),
            });
        }
        self.messages
            .iter_mut()
            .rev()
            .find(|message| message.run_id == id && message.role == MessageRole::Assistant)
            .expect("assistant message was just created")
    }

    pub fn complete(&mut self, id: RunId) -> bool {
        self.finish(id, RunStatus::Complete)
    }

    pub fn fail(&mut self, id: RunId, error: String) -> bool {
        let Some(message) = self.assistant_message(id) else {
            return false;
        };
        append_part(message, MessagePart::Error(error.clone()));
        self.finish(id, RunStatus::Failed(error))
    }

    pub fn cancel(&mut self) -> bool {
        let Some(run) = self.run.as_mut().filter(|run| run.status.is_running()) else {
            return false;
        };
        run.status = RunStatus::Cancelled;
        self.pending_approval = None;
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
        self.pending_approval = None;
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

    /// Answered turns without answer text are omitted, while partial answers are retained.
    /// Thinking and earlier tool rounds are not cross-run provider replay data.
    pub fn answered_turns(&self) -> Vec<ConversationTurn> {
        self.messages
            .chunk_by(|left, right| left.run_id == right.run_id)
            .filter_map(|turn| {
                let user = turn
                    .iter()
                    .find(|message| message.role == MessageRole::User)?;
                let answer = turn.iter().find(|message| {
                    message.role == MessageRole::Assistant && !message.text.is_empty()
                })?;
                Some(ConversationTurn {
                    run_id: user.run_id,
                    user: user.text.clone(),
                    answer: answer.text.clone(),
                })
            })
            .collect()
    }

    pub fn provider_history(&self) -> Vec<ProviderMessage> {
        FullContext.project(&self.answered_turns()).history
    }

    pub fn run(&self) -> Option<&AgentRun> {
        self.run.as_ref()
    }

    /// Record one provider round. Cumulative totals and the latest context sample are distinct.
    pub fn record_usage(&mut self, run_id: RunId, usage: TokenUsage) -> bool {
        match self.run.as_ref() {
            Some(run) if run.id == run_id => {
                self.usage.accumulate(usage);
                self.latest_usage = Some(usage);
                self.latest_usage_run = Some(run_id);
                true
            }
            _ => false,
        }
    }

    pub fn usage(&self) -> TokenUsage {
        self.usage
    }

    pub fn latest_usage(&self) -> Option<TokenUsage> {
        self.latest_usage
    }

    pub fn latest_usage_run(&self) -> Option<RunId> {
        self.latest_usage_run
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
        if self.tool_events.len() == MAX_TOOL_EVENTS {
            self.tool_events.remove(0);
        }
        self.tool_events.push(ToolEvent {
            run_id,
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
            .find(|event| event.run_id == run_id && event.call_id == result.call_id)
        else {
            return false;
        };
        event.result = Some(result);
        true
    }

    pub fn request_approval(&mut self, run_id: RunId, approval: ToolApproval) -> bool {
        let call_pending = self.tool_events.iter().any(|event| {
            event.run_id == run_id && event.call_id == approval.call.id && event.result.is_none()
        });
        let run_ready = self
            .run
            .as_ref()
            .is_some_and(|run| run.id == run_id && run.status.is_running());
        if !call_pending || !run_ready || self.pending_approval.is_some() {
            return false;
        }
        self.pending_approval = Some(approval);
        self.run = self.run.take().map(|run| AgentRun {
            status: RunStatus::WaitingForApproval,
            ..run
        });
        true
    }

    pub fn resolve_approval(&mut self, run_id: RunId, call_id: &str) -> Option<ToolApproval> {
        let matches = self
            .run
            .as_ref()
            .is_some_and(|run| run.id == run_id && run.status == RunStatus::WaitingForApproval)
            && self
                .pending_approval
                .as_ref()
                .is_some_and(|approval| approval.call.id == call_id);
        if !matches {
            return None;
        }
        self.run = self.run.take().map(|run| AgentRun {
            status: RunStatus::Streaming,
            ..run
        });
        self.pending_approval.take()
    }

    pub fn pending_approval(&self) -> Option<&ToolApproval> {
        self.pending_approval.as_ref()
    }

    pub fn tool_events(&self) -> &[ToolEvent] {
        &self.tool_events
    }
}

fn append_part(message: &mut ChatMessage, part: MessagePart) {
    let appended = match (message.parts.last_mut(), &part) {
        (Some(MessagePart::Thinking(current)), MessagePart::Thinking(delta))
        | (Some(MessagePart::Answer(current)), MessagePart::Answer(delta))
        | (Some(MessagePart::Error(current)), MessagePart::Error(delta)) => {
            current.push_str(delta);
            true
        }
        _ => false,
    };
    if !appended {
        message.parts.push(part);
    }
}

#[cfg(test)]
mod tests;
