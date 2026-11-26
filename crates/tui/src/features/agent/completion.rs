use eggplant_agent::{AgentContinuation, AgentEvent, RunId};

use super::{AgentState, run::AgentCompletion};
use crate::{
    data_object::Model,
    manager::effect::{Completion, Effect},
};

/// Semantic result of applying one provider event to Agent state.
pub(crate) enum AgentApplyOutcome {
    None,
    ApprovalRequired(AgentContinuation),
}

impl AgentState {
    pub(crate) fn apply_event(&mut self, run_id: RunId, event: AgentEvent) -> AgentApplyOutcome {
        match event {
            AgentEvent::TextDelta(delta) => {
                self.session_mut().append_delta(run_id, &delta);
                AgentApplyOutcome::None
            }
            AgentEvent::Usage(usage) => {
                self.session_mut().record_usage(run_id, usage);
                AgentApplyOutcome::None
            }
            AgentEvent::Complete => {
                self.session_mut().complete(run_id);
                AgentApplyOutcome::None
            }
            AgentEvent::ToolCalls(calls) => {
                calls.into_iter().for_each(|call| {
                    self.session_mut().record_tool_call(run_id, &call);
                });
                AgentApplyOutcome::None
            }
            AgentEvent::ToolResults(results) => {
                results.into_iter().for_each(|result| {
                    self.session_mut().complete_tool_call(run_id, result);
                });
                AgentApplyOutcome::None
            }
            AgentEvent::ApprovalRequired {
                approval,
                continuation,
            } => {
                if self.session_mut().request_approval(run_id, approval) {
                    self.suspend(run_id, continuation.clone());
                    AgentApplyOutcome::ApprovalRequired(continuation)
                } else {
                    AgentApplyOutcome::None
                }
            }
            AgentEvent::Failed(error) => {
                self.session_mut().fail(run_id, error);
                AgentApplyOutcome::None
            }
        }
    }
}

impl Completion for AgentCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let approval = self.events.into_iter().find_map(|event| {
            match model.agent.apply_event(self.run_id, event) {
                AgentApplyOutcome::ApprovalRequired(continuation) => Some(continuation),
                AgentApplyOutcome::None => None,
            }
        });
        if approval.is_some() {
            super::approval::transition_to_approval(model);
        }
        super::storage::persist_finalized(model)
    }
}
