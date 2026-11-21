//! Provider-neutral agent session, message, and run-event contracts.
//!
//! This crate intentionally has no editor, terminal, filesystem, or provider SDK
//! dependencies. Frontends own presentation and effect scheduling; providers own
//! model I/O.

const MAX_MESSAGES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEvent {
    TextDelta(String),
    Complete,
    Failed(String),
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
}
