use std::sync::Arc;

use editor::EditorChange;
use eggplant_agent::{RunId, ToolCall};

use super::{
    host::AgentHost,
    registry::{PreparedTool, ToolRegistry},
    run::{MAX_TOOL_OUTPUT, ProviderAgentRun, RunProgress},
    runtime::AgentRuntime,
    tools::ToolContext,
};
use crate::manager::effect::Effect;

/// Host capabilities required to execute one prepared main-thread tool.
pub(crate) trait ToolExecutionContext {
    fn runtime(&self) -> &AgentRuntime;
    fn runtime_mut(&mut self) -> &mut AgentRuntime;
    fn tool_context(&self) -> Option<ToolContext>;
    fn host(&mut self) -> &mut dyn AgentHost;
    fn after_edit(&mut self, change: Option<EditorChange>) -> Option<Effect>;
}

/// Coordinates main-thread execution, common after-edit effects, and provider continuation.
pub(crate) struct ToolExecutor<'a, C: ToolExecutionContext> {
    context: &'a mut C,
    registry: Arc<ToolRegistry>,
    run_id: RunId,
    call: ToolCall,
}

impl<'a, C: ToolExecutionContext> ToolExecutor<'a, C> {
    pub(crate) fn new(
        context: &'a mut C,
        registry: Arc<ToolRegistry>,
        run_id: RunId,
        call: ToolCall,
    ) -> Self {
        Self {
            context,
            registry,
            run_id,
            call,
        }
    }

    pub(crate) fn execute(self) -> Option<Effect> {
        let Self {
            context,
            registry,
            run_id,
            call,
        } = self;
        registry.validate_round(std::slice::from_ref(&call)).ok()?;
        context
            .runtime()
            .session()
            .tool_call_is_pending(run_id, &call.id)
            .then_some(())?;
        let provider = context.runtime().provider()?;
        let continuation = context.runtime_mut().take_suspension(run_id)?;
        let snapshot = context.tool_context()?;
        let outcome = match registry.prepare(&call, &snapshot) {
            PreparedTool::Immediate(result) => super::registry::ToolOutcome::immediate(result),
            PreparedTool::MainThread(operation) => operation.execute(context.host()),
        };
        let result = super::tools::bound_result(
            outcome.result,
            MAX_TOOL_OUTPUT.saturating_sub(continuation.tool_output),
        );
        let fresh_context = context.tool_context()?;
        context
            .runtime_mut()
            .session_mut()
            .complete_tool_call(run_id, result.clone());
        let after_edit = context.after_edit(outcome.change);
        let cancellation = context.runtime_mut().begin_cancellation();
        let thinking_id = context.runtime().thinking_id();
        let resume = Effect::feature(ProviderAgentRun {
            run_id,
            prompt: String::new(),
            cancellation,
            provider,
            tools: registry,
            context: fresh_context,
            progress: RunProgress {
                thinking_id,
                ..RunProgress::from(continuation).with_results(vec![result])
            },
        });
        Effect::combine(after_edit, Some(resume))
    }
}
