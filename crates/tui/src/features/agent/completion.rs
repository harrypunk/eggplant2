use super::{mutation::MutationExecutor, run::AgentCompletion, runtime::AgentApplyOutcome};
use crate::{
    data_object::Model,
    manager::effect::{Completion, Effect},
};

impl Completion for AgentCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let run_id = self.run_id;
        let mutation = self.events.into_iter().find_map(|event| {
            match model.agent.runtime_mut().apply_event(run_id, event) {
                AgentApplyOutcome::MutationRequested(call) => Some(call),
                AgentApplyOutcome::None => None,
            }
        });
        let mutation =
            mutation.and_then(|call| MutationExecutor::new(model, run_id, call).execute());
        Effect::combine(mutation, super::storage::persist_finalized(model))
    }
}
