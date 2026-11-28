use std::path::Path;

use editor::BufferId;
use eggplant_agent::{RunId, ToolCall};

use super::{
    run::{ProviderAgentRun, RunProgress},
    runtime::AgentRuntime,
    tools::ToolContext,
    write::{MutationKind, MutationOutcome},
};
use crate::manager::effect::Effect;

/// Editor and effect capabilities required to execute one validated provider mutation.
pub(crate) trait MutationContext {
    fn runtime(&self) -> &AgentRuntime;
    fn runtime_mut(&mut self) -> &mut AgentRuntime;
    fn tool_context(&self) -> Option<ToolContext>;
    fn apply_mutation(
        &mut self,
        kind: MutationKind,
        root: &Path,
        call: &ToolCall,
    ) -> MutationOutcome;
    fn after_mutation(&mut self, edited: Option<BufferId>) -> Option<Effect>;
}

/// Coordinates the main-thread mutation, fresh context capture, and provider continuation.
pub(crate) struct MutationExecutor<'a, C: MutationContext> {
    context: &'a mut C,
    run_id: RunId,
    call: ToolCall,
}

impl<'a, C: MutationContext> MutationExecutor<'a, C> {
    pub(crate) fn new(context: &'a mut C, run_id: RunId, call: ToolCall) -> Self {
        Self {
            context,
            run_id,
            call,
        }
    }

    pub(crate) fn execute(self) -> Option<Effect> {
        let Self {
            context,
            run_id,
            call,
        } = self;
        context
            .runtime()
            .session()
            .tool_call_is_pending(run_id, &call.id)
            .then_some(())?;
        let provider = context.runtime().provider()?;
        let root = context.tool_context()?.root;
        let continuation = context.runtime_mut().take_suspension(run_id)?;
        let kind = MutationKind::from_call(&call)?;
        let MutationOutcome { result, edited } = context.apply_mutation(kind, &root, &call);
        let fresh_context = context.tool_context()?;
        context
            .runtime_mut()
            .session_mut()
            .complete_tool_call(run_id, result.clone());
        let after_edit = context.after_mutation(edited);
        let cancellation = context.runtime_mut().begin_cancellation();
        let thinking_id = context.runtime().thinking_id();
        let resume = Effect::feature(ProviderAgentRun {
            run_id,
            prompt: String::new(),
            cancellation,
            provider,
            context: fresh_context,
            progress: RunProgress {
                thinking_id,
                ..RunProgress::from(continuation).with_results(vec![result])
            },
        });
        Effect::combine(after_edit, Some(resume))
    }
}
