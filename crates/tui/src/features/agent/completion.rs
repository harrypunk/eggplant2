use super::{execution::ToolExecutor, run::AgentCompletion, runtime::AgentApplyOutcome};
use crate::{
    data_object::Model,
    manager::effect::{Completion, Effect},
};

impl Completion for AgentCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let run_id = self.run_id;
        let tools = model.agent.runtime().tools();
        let execution = self.events.into_iter().find_map(|event| {
            match model.agent.runtime_mut().apply_event(run_id, event) {
                AgentApplyOutcome::MainThreadToolRequested(call) => Some(call),
                AgentApplyOutcome::None => None,
            }
        });
        let execution =
            execution.and_then(|call| ToolExecutor::new(model, tools, run_id, call).execute());
        Effect::combine(execution, super::storage::persist_finalized(model))
    }
}
