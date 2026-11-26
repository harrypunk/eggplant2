use crate::{
    AgentRun, ChatMessage, MessageRole, RunId, RunStatus, SessionId, TokenUsage, ToolApproval,
    ToolCall, ToolEvent, ToolResult,
};

const MAX_MESSAGES: usize = 100;
const MAX_TOOL_EVENTS: usize = 50;

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
}

impl AgentSession {
    pub fn id(&self) -> &SessionId {
        &self.id
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
        });
        self.pending_approval = None;
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

    pub fn run(&self) -> Option<&AgentRun> {
        self.run.as_ref()
    }

    /// Record one provider round. Cumulative totals and the latest context sample are distinct.
    pub fn record_usage(&mut self, run_id: RunId, usage: TokenUsage) -> bool {
        match self.run.as_ref() {
            Some(run) if run.id == run_id => {
                self.usage.accumulate(usage);
                self.latest_usage = Some(usage);
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

#[cfg(test)]
mod tests;
