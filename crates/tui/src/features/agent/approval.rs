use super::{
    context::tool_context,
    run::{ProviderAgentRun, RunProgress},
};
use crate::{
    data_object::{Context, Model},
    manager::effect::Effect,
};

pub(crate) fn approve_write(model: &mut Model) -> Option<Effect> {
    resolve_write(model, true)
}

pub(crate) fn reject_write(model: &mut Model) -> Option<Effect> {
    resolve_write(model, false)
}

fn resolve_write(model: &mut Model, approved: bool) -> Option<Effect> {
    let run_id = model.agent.session().run().map(|run| run.id)?;
    let approval = model.agent.session().pending_approval()?.clone();
    let provider = model.agent.provider()?;
    let root = tool_context(model)?.root;
    let continuation = model.agent.take_suspension(run_id)?;
    let outcome = match approved {
        true => match approval.call.name.as_str() {
            super::create::TOOL_NAME => {
                super::create::apply(&mut model.editor, &root, &approval.call)
            }
            _ => super::write::apply(&mut model.editor, &root, &approval.call),
        },
        false => match approval.call.name.as_str() {
            super::create::TOOL_NAME => super::create::rejected(&approval.call),
            _ => super::write::rejected(&approval.call),
        },
    };
    let super::write::WriteOutcome { result, edited } = outcome;
    let context = tool_context(model)?;
    model
        .agent
        .session_mut()
        .resolve_approval(run_id, &approval.call.id);
    model
        .agent
        .session_mut()
        .complete_tool_call(run_id, result.clone());
    let after_edit = edited.and_then(|buffer_id| {
        Effect::combine(
            crate::buffer::save_if_dirty(model, buffer_id),
            crate::code_navigation::synchronize_buffer(model, buffer_id),
        )
    });
    let cancellation = model.agent.begin_cancellation();
    let thinking_id = model.agent.runtime().thinking_id();
    let resume = Effect::feature(ProviderAgentRun {
        run_id,
        prompt: String::new(),
        cancellation,
        provider,
        context,
        progress: RunProgress {
            thinking_id,
            ..RunProgress::from(continuation).with_results(vec![result])
        },
    });
    transition_to_chat(model);
    Effect::combine(after_edit, Some(resume))
}

pub(crate) fn transition_to_approval(model: &mut Model) {
    transition_modal(model, Context::AgentChatDialog, Context::AgentApproval);
}

pub(crate) fn transition_to_chat(model: &mut Model) {
    transition_modal(model, Context::AgentApproval, Context::AgentChatDialog);
}

fn transition_modal(model: &mut Model, from: Context, to: Context) {
    if model.agent.dialog_open() {
        model.modal.replace(from, to);
    }
}
