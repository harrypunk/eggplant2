use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::{
    AgentContinuation, AgentEvent, AgentProvider, AgentRequest, CancellationToken, ProviderMessage,
    RunId, ToolApproval, ToolResult, ToolResultStatus,
};

use super::{EditorContext, tools::ToolContext};
use crate::manager::effect::{FeatureCompletion, FeatureEffect};

/// Identity and lifecycle data owned by one foreground provider run.
#[derive(Debug)]
pub(crate) struct AgentRunState {
    id: RunId,
    started_at: i64,
    editor_context: Option<EditorContext>,
    cancellation: CancellationToken,
    suspension: Option<AgentContinuation>,
}

impl AgentRunState {
    pub(crate) fn new(id: RunId, started_at: i64, editor_context: Option<EditorContext>) -> Self {
        Self {
            id,
            started_at,
            editor_context,
            cancellation: CancellationToken::default(),
            suspension: None,
        }
    }

    fn matches(&self, run_id: RunId) -> bool {
        self.id == run_id
    }

    pub(crate) fn started_at(&self, run_id: RunId) -> Option<i64> {
        self.matches(run_id).then_some(self.started_at)
    }

    pub(crate) fn editor_context(&self, run_id: RunId) -> Option<&EditorContext> {
        self.matches(run_id)
            .then_some(self.editor_context.as_ref())
            .flatten()
    }

    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub(crate) fn cancel(&self) {
        self.cancellation.cancel();
    }

    pub(crate) fn suspend(&mut self, run_id: RunId, continuation: AgentContinuation) {
        if self.matches(run_id) {
            self.suspension = Some(continuation);
        }
    }

    pub(crate) fn take_suspension(&mut self, run_id: RunId) -> Option<AgentContinuation> {
        self.matches(run_id)
            .then_some(self.suspension.take())
            .flatten()
    }

    pub(crate) fn clear_suspension(&mut self) {
        self.suspension = None;
    }
}

const MAX_TOOL_STEPS: usize = 3;
const MAX_TOOL_CALLS: usize = 8;
pub(crate) const MAX_TOOL_OUTPUT: usize = 100 * 1024;

pub(crate) struct ProviderAgentRun {
    pub(crate) run_id: RunId,
    pub(crate) prompt: String,
    pub(crate) cancellation: CancellationToken,
    pub(crate) provider: Arc<dyn AgentProvider>,
    pub(crate) context: ToolContext,
    pub(crate) progress: RunProgress,
}

#[derive(Clone, Default)]
pub(crate) struct RunProgress {
    pub(crate) step: usize,
    pub(crate) history: Vec<ProviderMessage>,
    pub(crate) tool_calls: usize,
    pub(crate) tool_output: usize,
}

#[derive(Default)]
pub(crate) struct ProviderRound {
    pub(crate) text: String,
    pub(crate) calls: Vec<eggplant_agent::ToolCall>,
    pub(crate) terminal: Option<RoundTerminal>,
}

pub(crate) enum RoundTerminal {
    Complete,
    Failed(String),
}

pub(crate) enum RoundAction {
    Stop(Option<AgentEvent>),
    Tools(RunProgress, Vec<eggplant_agent::ToolCall>),
}

enum ToolExecution {
    Results(Vec<ToolResult>),
    Approval(ToolApproval),
}

impl ProviderRound {
    fn apply(self, event: AgentEvent) -> (Self, Option<AgentEvent>) {
        match self.terminal.as_ref() {
            Some(_) => (self, None),
            None => match event {
                AgentEvent::TextDelta(delta) => (
                    Self {
                        text: self.text + delta.as_str(),
                        ..self
                    },
                    Some(AgentEvent::TextDelta(delta)),
                ),
                AgentEvent::ThinkingDelta(delta) => (self, Some(AgentEvent::ThinkingDelta(delta))),
                AgentEvent::ToolCalls(calls) => (
                    Self {
                        calls: self.calls.into_iter().chain(calls).collect(),
                        ..self
                    },
                    None,
                ),
                AgentEvent::Usage(usage) => (self, Some(AgentEvent::Usage(usage))),
                AgentEvent::Complete => (
                    Self {
                        terminal: Some(RoundTerminal::Complete),
                        ..self
                    },
                    None,
                ),
                AgentEvent::Failed(error) => (
                    Self {
                        terminal: Some(RoundTerminal::Failed(error)),
                        ..self
                    },
                    None,
                ),
                AgentEvent::ToolResults(_) | AgentEvent::ApprovalRequired { .. } => (self, None),
            },
        }
    }
}

impl RunProgress {
    fn request(&self, prompt: &str, cancellation: CancellationToken) -> AgentRequest {
        match self.history.is_empty() {
            true => AgentRequest::new(prompt.to_string(), cancellation),
            false => AgentRequest::follow_up(self.history.clone(), cancellation),
        }
    }

    pub(crate) fn prepare(
        self,
        prompt: &str,
        round: ProviderRound,
        result: Result<(), eggplant_agent::AgentError>,
        cancelled: bool,
    ) -> RoundAction {
        let ProviderRound {
            text,
            calls,
            terminal,
        } = round;
        let failure = result.err().map(|error| error.0).or(match terminal {
            Some(RoundTerminal::Failed(error)) => Some(error),
            Some(RoundTerminal::Complete) | None => None,
        });
        match (failure, cancelled, calls.is_empty()) {
            (Some(error), _, _) => RoundAction::Stop(Some(AgentEvent::Failed(error))),
            (None, true, _) => RoundAction::Stop(None),
            (None, false, true) => RoundAction::Stop(Some(AgentEvent::Complete)),
            (None, false, false) => match self.with_tool_round(prompt, text, calls.clone()) {
                Ok(progress) => RoundAction::Tools(progress, calls),
                Err(error) => RoundAction::Stop(Some(AgentEvent::Failed(error))),
            },
        }
    }

    fn with_tool_round(
        self,
        prompt: &str,
        text: String,
        calls: Vec<eggplant_agent::ToolCall>,
    ) -> Result<Self, String> {
        let Self {
            step,
            history,
            tool_calls,
            tool_output,
        } = self;
        require(step < MAX_TOOL_STEPS, "tool step limit reached")?;
        require(
            tool_calls + calls.len() <= MAX_TOOL_CALLS,
            "tool call limit reached",
        )?;
        require(tool_output < MAX_TOOL_OUTPUT, "tool output limit reached")?;
        let call_ids = calls
            .iter()
            .map(|call| call.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let valid = call_ids.len() == calls.len()
            && calls.iter().all(|call| {
                !call.id.is_empty()
                    && !call.name.is_empty()
                    && call.arguments.len() <= super::tools::MAX_ARGUMENT_BYTES
            });
        require(valid, "provider emitted an invalid tool call")?;
        let writes = calls.iter().filter(|call| call.name == "write").count();
        require(
            writes == 0 || (writes == 1 && calls.len() == 1),
            "write must be the only tool call in its round",
        )?;
        let history = match history.is_empty() {
            true => vec![ProviderMessage::user(prompt.to_string())],
            false => history,
        };
        let tool_calls = tool_calls + calls.len();
        Ok(Self {
            step: step + 1,
            history: history
                .into_iter()
                .chain([ProviderMessage::assistant(text, calls)])
                .collect(),
            tool_calls,
            tool_output,
        })
    }

    pub(crate) fn with_results(self, results: Vec<ToolResult>) -> Self {
        let Self {
            step,
            history,
            tool_calls,
            tool_output,
        } = self;
        let output = results
            .iter()
            .map(|result| result.content.len())
            .sum::<usize>();
        Self {
            step,
            history: history
                .into_iter()
                .chain(
                    results
                        .into_iter()
                        .map(|result| ProviderMessage::tool(result.call_id, result.content)),
                )
                .collect(),
            tool_calls,
            tool_output: tool_output.saturating_add(output),
        }
    }
}

impl From<RunProgress> for AgentContinuation {
    fn from(progress: RunProgress) -> Self {
        Self {
            step: progress.step,
            history: progress.history,
            tool_calls: progress.tool_calls,
            tool_output: progress.tool_output,
        }
    }
}

impl From<AgentContinuation> for RunProgress {
    fn from(continuation: AgentContinuation) -> Self {
        Self {
            step: continuation.step,
            history: continuation.history,
            tool_calls: continuation.tool_calls,
            tool_output: continuation.tool_output,
        }
    }
}

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

impl ProviderAgentRun {
    async fn drive(self, mut emit: impl FnMut(AgentEvent) + Send + 'static) {
        self.drive_round(self.progress.clone(), &mut emit).await;
    }

    fn drive_round<'a>(
        &'a self,
        progress: RunProgress,
        emit: &'a mut (impl FnMut(AgentEvent) + Send + 'static),
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            let request = progress.request(&self.prompt, self.cancellation.clone());
            let (result, round) = self.stream_round(request, emit).await;
            match progress.prepare(
                &self.prompt,
                round,
                result,
                self.cancellation.is_cancelled(),
            ) {
                RoundAction::Stop(event) => event.into_iter().for_each(&mut *emit),
                RoundAction::Tools(progress, calls) => {
                    emit(AgentEvent::ToolCalls(calls.clone()));
                    match self.execute_calls(&calls, progress.tool_output) {
                        ToolExecution::Results(results) => {
                            emit(AgentEvent::ToolResults(results.clone()));
                            match self.cancellation.is_cancelled() {
                                true => {}
                                false => {
                                    self.drive_round(progress.with_results(results), emit).await;
                                }
                            }
                        }
                        ToolExecution::Approval(approval) => {
                            emit(AgentEvent::ApprovalRequired {
                                approval,
                                continuation: progress.into(),
                            });
                        }
                    }
                }
            }
        })
    }

    async fn stream_round(
        &self,
        request: AgentRequest,
        emit: &mut (impl FnMut(AgentEvent) + Send + 'static),
    ) -> (Result<(), eggplant_agent::AgentError>, ProviderRound) {
        let mut round = ProviderRound::default();
        let result = self
            .provider
            .stream(
                request,
                Box::new(|event| {
                    let (next, outgoing) = std::mem::take(&mut round).apply(event);
                    round = next;
                    outgoing.into_iter().for_each(&mut *emit);
                }),
            )
            .await;
        (result, round)
    }

    fn execute_calls(
        &self,
        calls: &[eggplant_agent::ToolCall],
        output_used: usize,
    ) -> ToolExecution {
        match calls {
            [call] if call.name == "write" && !self.cancellation.is_cancelled() => {
                match super::write::prepare(call, &self.context) {
                    Ok(approval) => ToolExecution::Approval(approval),
                    Err(result) => ToolExecution::Results(vec![super::tools::bound_result(
                        result,
                        MAX_TOOL_OUTPUT.saturating_sub(output_used),
                    )]),
                }
            }
            _ => ToolExecution::Results(
                calls
                    .iter()
                    .map(|call| match self.cancellation.is_cancelled() {
                        true => ToolResult {
                            call_id: call.id.clone(),
                            status: ToolResultStatus::Cancelled,
                            content: "cancelled".to_string(),
                            truncated: false,
                        },
                        false => super::tools::execute(call, &self.context),
                    })
                    .scan(output_used, |used, result| {
                        let result = super::tools::bound_result(
                            result,
                            MAX_TOOL_OUTPUT.saturating_sub(*used),
                        );
                        *used = used.saturating_add(result.content.len());
                        Some(result)
                    })
                    .collect(),
            ),
        }
    }
}

impl FeatureEffect for ProviderAgentRun {
    fn perform(self: Box<Self>) -> crate::data_object::Message {
        let run_id = self.run_id;
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = events.clone();
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("agent runtime must build")
            .block_on(self.drive(move |event| {
                if let Ok(mut current) = output.lock() {
                    current.push(event);
                }
            }));
        let events = events.lock().unwrap().clone();
        crate::data_object::Message::FeatureCompletion(FeatureCompletion::new(AgentCompletion {
            run_id,
            events,
        }))
    }

    fn execute(
        self: Box<Self>,
        sender: tokio::sync::mpsc::UnboundedSender<crate::data_object::Message>,
    ) {
        tokio::spawn(async move {
            let run_id = self.run_id;
            let event_sender = sender.clone();
            self.drive(move |event| {
                let _ = event_sender.send(crate::data_object::Message::FeatureCompletion(
                    FeatureCompletion::new(AgentCompletion {
                        run_id,
                        events: vec![event],
                    }),
                ));
            })
            .await;
        });
    }
}

pub(crate) struct AgentCompletion {
    pub(crate) run_id: RunId,
    pub(crate) events: Vec<AgentEvent>,
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use eggplant_agent::RunStatus;
    use tokio::sync::Notify;

    use super::*;
    use crate::{data_object::Model, manager::message_manager::MessageManager};

    #[derive(Debug)]
    struct ControlledProvider {
        resume: Arc<Notify>,
    }

    impl AgentProvider for ControlledProvider {
        fn stream<'a>(
            &'a self,
            _request: AgentRequest,
            mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
        ) -> Pin<Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>>
        {
            Box::pin(async move {
                on_event(AgentEvent::TextDelta("hello".to_string()));
                self.resume.notified().await;
                on_event(AgentEvent::TextDelta(" world".to_string()));
                on_event(AgentEvent::Complete);
                Ok(())
            })
        }
    }

    async fn receive(
        receiver: &mut tokio::sync::mpsc::UnboundedReceiver<crate::data_object::Message>,
    ) -> crate::data_object::Message {
        tokio::time::timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect("provider event should arrive")
            .expect("provider channel should remain open")
    }

    #[tokio::test]
    async fn async_execution_publishes_text_before_the_run_completes() {
        let resume = Arc::new(Notify::new());
        let mut model = Model::default();
        let started = model
            .agent
            .runtime_mut()
            .begin("question".to_string(), None, 1)
            .unwrap();
        let cancellation = model.agent.runtime_mut().begin_cancellation();
        let run = ProviderAgentRun {
            run_id: started.id,
            prompt: "question".to_string(),
            cancellation,
            provider: Arc::new(ControlledProvider {
                resume: resume.clone(),
            }),
            context: ToolContext::default(),
            progress: RunProgress::default(),
        };
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        Box::new(run).execute(sender);
        let manager = MessageManager::default();

        manager.update(&mut model, receive(&mut receiver).await);
        assert_eq!(model.agent.session().messages()[1].text, "hello");
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Streaming
        );
        assert!(receiver.try_recv().is_err());

        resume.notify_one();
        manager.update(&mut model, receive(&mut receiver).await);
        assert_eq!(model.agent.session().messages()[1].text, "hello world");
        manager.update(&mut model, receive(&mut receiver).await);
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
    }
}
