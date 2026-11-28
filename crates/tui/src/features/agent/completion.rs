use super::{run::AgentCompletion, runtime::AgentApplyOutcome};
use crate::{
    data_object::Model,
    manager::effect::{Completion, Effect},
};

impl Completion for AgentCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        let approval = self.events.into_iter().find_map(|event| {
            match model.agent.runtime_mut().apply_event(self.run_id, event) {
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
