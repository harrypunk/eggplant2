use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::{
    AgentContinuation, AgentEvent, AgentProvider, AgentRequest, CancellationToken, ProviderMessage,
    RunId, ToolApproval, ToolResult, ToolResultStatus,
};

use super::tools::ToolContext;
use crate::manager::effect::{FeatureCompletion, FeatureEffect};

pub(crate) struct FakeAgentRun {
    pub(crate) run_id: RunId,
    pub(crate) prompt: String,
}

impl FakeAgentRun {
    fn events(&self) -> Vec<AgentEvent> {
        if self.prompt.eq_ignore_ascii_case("fail") {
            return vec![AgentEvent::Failed("simulated provider failure".to_string())];
        }
        vec![
            AgentEvent::TextDelta("Simulated response to: ".to_string()),
            AgentEvent::TextDelta(self.prompt.clone()),
            AgentEvent::Complete,
        ]
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
                AgentEvent::ToolCalls(calls) => (
                    Self {
                        calls: self.calls.into_iter().chain(calls).collect(),
                        ..self
                    },
                    None,
                ),
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

impl FeatureEffect for FakeAgentRun {
    fn perform(self: Box<Self>) -> crate::data_object::Message {
        crate::data_object::Message::FeatureCompletion(FeatureCompletion::new(AgentCompletion {
            run_id: self.run_id,
            events: self.events(),
        }))
    }

    fn execute(
        self: Box<Self>,
        sender: tokio::sync::mpsc::UnboundedSender<crate::data_object::Message>,
    ) {
        tokio::spawn(async move {
            for event in self.events() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                let run_id = self.run_id;
                let completion = AgentCompletion {
                    run_id,
                    events: vec![event],
                };
                if sender
                    .send(crate::data_object::Message::FeatureCompletion(
                        FeatureCompletion::new(completion),
                    ))
                    .is_err()
                {
                    break;
                }
            }
        });
    }
}
