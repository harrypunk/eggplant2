use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::{
    AgentContinuation, AgentEvent, AgentProvider, AgentRequest, CancellationToken, ProviderMessage,
    RunId, ToolApproval, ToolResult, ToolResultStatus,
};

use super::{
    AgentContextProvider, EditorContext,
    tools::{BufferContext, ToolContext},
};
use crate::{
    component::text_input::TextInputEvent,
    data_object::{CommandId, Context, Model, StatusMessage, Visibility},
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
        text_input_registry::TextInputRegistry,
    },
    tools::ToolKind,
    ui::UiRegistry,
};

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(CommandId("agent_open"), "open AI agent", handle_command),
        Command::feature(CommandId("agent_close"), "hide AI agent", handle_command),
        Command::feature(
            CommandId("agent_submit"),
            "submit agent prompt",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_backspace"),
            "delete agent prompt character",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_interrupt"),
            "interrupt agent run",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_approve"),
            "approve agent write",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_reject"),
            "reject agent write",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_toggle_panel"),
            "toggle agent tool panel",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_panel_close"),
            "leave agent panel input",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_insert"),
            "enter agent insert mode",
            handle_command,
        ),
        Command::feature(
            CommandId("agent_panel_normal"),
            "return to agent normal mode",
            handle_command,
        ),
        Command::feature(CommandId("agent_help"), "toggle agent help", handle_command),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register_with_cursor(Context::AgentChatDialog, handle_input)?;
    input.register_with_cursor(Context::AgentChat, handle_input)?;
    input.register_with_cursor(Context::AgentChatInsert, handle_input)
}

fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    if model.context_stack.last() == Some(&Context::AgentChat)
        && model.agent.panel_mode() == super::AgentInputMode::Normal
    {
        return None;
    }
    model.agent.handle_input_event(event);
    None
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("agent", super::AgentDialog::overlay)?;
    ui.register_overlay("agent_help", super::help::overlay)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "agent_open" => open(model),
        "agent_close" => close(model),
        "agent_submit" => submit(model),
        "agent_backspace" => backspace(model),
        "agent_interrupt" => interrupt(model),
        "agent_approve" => approve_write(model),
        "agent_reject" => reject_write(model),
        "agent_toggle_panel" => toggle_panel(model),
        "agent_panel_close" => {
            close_panel_input(model);
            None
        }
        "agent_insert" => enter_insert_mode(model),
        "agent_panel_normal" => {
            if model.agent.help_visible() {
                close_help(model)
            } else {
                enter_normal_mode(model)
            }
        }
        "agent_help" => toggle_help(model),
        _ => None,
    }
}

fn open(model: &mut Model) -> Option<Effect> {
    if model.agent.dialog_open() {
        return None;
    }
    let context = match model.agent_context() {
        Ok(context) => context,
        Err(message) => return notify(model, message),
    };
    let input_context = match model.agent.session().pending_approval() {
        Some(_) => Context::AgentApproval,
        None => Context::AgentChatDialog,
    };
    if !model.modal.open(input_context) {
        return None;
    }
    let label = prompt_label(model, &context);
    model.agent.open_dialog(context, label);
    model.context_stack.push(input_context);
    None
}

fn prompt_label(model: &Model, context: &EditorContext) -> String {
    let relative = model
        .workspace
        .as_ref()
        .and_then(|workspace| context.path.strip_prefix(workspace.root()).ok())
        .unwrap_or(&context.path);
    format!("{}:{}", relative.display(), context.line)
}

pub(crate) fn focus_panel_input(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility != Visibility::Displayed
        || model.tools.active_tool != Some(ToolKind::Agent)
    {
        return None;
    }
    if model.workspace.is_none() {
        return notify(model, "Open a project to use the agent".to_string());
    }
    close_help(model);
    model.agent.prepare_panel_prompt();
    model.current_focus = crate::data_object::FocusTarget::ToolsPanel;
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatInsert);
    if model.context_stack.last() != Some(&Context::AgentChat) {
        model.context_stack.push(Context::AgentChat);
    }
    None
}

fn enter_insert_mode(model: &mut Model) -> Option<Effect> {
    if model.context_stack.last() == Some(&Context::AgentChat) {
        model.agent.enter_insert_mode();
        model.context_stack.push(Context::AgentChatInsert);
    }
    None
}

fn enter_normal_mode(model: &mut Model) -> Option<Effect> {
    model.agent.enter_normal_mode();
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatInsert);
    None
}

fn toggle_help(model: &mut Model) -> Option<Effect> {
    if model.agent.help_visible() {
        return close_help(model);
    }
    if model.context_stack.last() != Some(&Context::AgentChat)
        || model.agent.panel_mode() != super::AgentInputMode::Normal
        || !model.modal.open(Context::AgentChatHelp)
    {
        return None;
    }
    model.agent.toggle_help();
    model.context_stack.push(Context::AgentChatHelp);
    None
}

fn close_help(model: &mut Model) -> Option<Effect> {
    model.agent.close_help();
    model.modal.close(Context::AgentChatHelp);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatHelp);
    None
}

pub(crate) fn close_panel_input(model: &mut Model) {
    close_help(model);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatInsert);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChat);
    model.agent.prepare_panel_prompt();
    crate::focus::focus_editor(model);
}

fn close(model: &mut Model) -> Option<Effect> {
    if !model.agent.dialog_open() {
        return None;
    }
    model.agent.close_dialog();
    [Context::AgentChatDialog, Context::AgentApproval]
        .into_iter()
        .for_each(|context| {
            model.modal.close(context);
            model.context_stack.pop_if(|active| active == &context);
        });
    None
}

fn transition_to_approval(model: &mut Model) {
    transition_modal(model, Context::AgentChatDialog, Context::AgentApproval);
}

fn transition_to_chat(model: &mut Model) {
    transition_modal(model, Context::AgentApproval, Context::AgentChatDialog);
}

fn transition_modal(model: &mut Model, from: Context, to: Context) {
    if !model.agent.dialog_open() || !model.modal.is_active(from) {
        return;
    }
    model.modal.close(from);
    model.context_stack.pop_if(|context| context == &from);
    if model.modal.open(to) {
        model.context_stack.push(to);
    }
}

fn submit(model: &mut Model) -> Option<Effect> {
    let panel_input = matches!(
        model.context_stack.last(),
        Some(Context::AgentChat | Context::AgentChatInsert)
    ) && model.tools.active_tool == Some(ToolKind::Agent);
    if !model.agent.dialog_open() && !panel_input {
        return None;
    }
    let prompt = model.agent.prompt().trim().to_string();
    if prompt.is_empty() {
        return notify(model, "Enter a prompt".to_string());
    }
    if model.agent.session().is_running() {
        return notify(model, "Agent is already running".to_string());
    }
    if panel_input && model.workspace.is_none() {
        return notify(model, "Open a project to use the agent".to_string());
    }
    if !panel_input && let Err(message) = model.agent_context() {
        return notify(model, message);
    }
    let run_id = model.agent.begin_prompt()?;
    if let Some(provider) = model.agent.provider() {
        let cancellation = model.agent.begin_cancellation();
        return Some(Effect::feature(ProviderAgentRun {
            run_id,
            prompt,
            cancellation,
            provider,
            context: tool_context(model)?,
            progress: RunProgress::default(),
        }));
    }
    Some(Effect::feature(FakeAgentRun { run_id, prompt }))
}

fn approve_write(model: &mut Model) -> Option<Effect> {
    resolve_write(model, true)
}

fn reject_write(model: &mut Model) -> Option<Effect> {
    resolve_write(model, false)
}

fn resolve_write(model: &mut Model, approved: bool) -> Option<Effect> {
    let run_id = model.agent.session().run().map(|run| run.id)?;
    let approval = model.agent.session().pending_approval()?.clone();
    let provider = model.agent.provider()?;
    let root = tool_context(model)?.root;
    let continuation = model.agent.take_suspension(run_id)?;
    let outcome = match approved {
        true => super::write::apply(&mut model.editor, &root, &approval.call),
        false => super::write::rejected(&approval.call),
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
    let resume = Effect::feature(ProviderAgentRun {
        run_id,
        prompt: String::new(),
        cancellation,
        provider,
        context,
        progress: RunProgress::from(continuation).with_results(vec![result]),
    });
    transition_to_chat(model);
    Effect::combine(after_edit, Some(resume))
}

fn backspace(model: &mut Model) -> Option<Effect> {
    model.agent.backspace();
    None
}

fn interrupt(model: &mut Model) -> Option<Effect> {
    let pending = model.agent.session().pending_approval().cloned();
    let run_id = model.agent.session().run().map(|run| run.id);
    model.agent.cancel_active();
    model.agent.session_mut().cancel();
    run_id
        .zip(pending)
        .map(|(run_id, approval)| {
            (
                run_id,
                ToolResult {
                    call_id: approval.call.id,
                    status: ToolResultStatus::Cancelled,
                    content: "Write approval cancelled; no changes were applied.".to_string(),
                    truncated: false,
                },
            )
        })
        .into_iter()
        .for_each(|(run_id, result)| {
            model.agent.session_mut().complete_tool_call(run_id, result);
        });
    model.agent.clear_suspension();
    transition_to_chat(model);
    super::storage::persist_finalized(model)
}

fn toggle_panel(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility == Visibility::Displayed
        && model.tools.active_tool == Some(ToolKind::Agent)
    {
        model.tools.visibility = Visibility::Hidden;
        if model.current_focus == crate::data_object::FocusTarget::ToolsPanel {
            close_panel_input(model);
        }
    } else {
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        return focus_panel_input(model);
    }
    None
}

impl AgentContextProvider for Model {
    fn agent_context(&self) -> Result<EditorContext, String> {
        let Some(buffer) = self.editor.active_buffer() else {
            return Err("Open a workspace file to use the agent".to_string());
        };
        if self.workspace.is_none()
            || crate::capabilities::readonly::active_buffer_is_read_only(self)
        {
            return Err("Agent is available only for workspace files".to_string());
        }
        let path = self
            .editor
            .buffer_path(buffer)
            .ok_or_else(|| "Open a workspace file to use the agent".to_string())?
            .to_path_buf();
        let (_, revision) = self
            .editor
            .active_revision()
            .ok_or_else(|| "Open a workspace file to use the agent".to_string())?;
        let line = self
            .editor
            .active_position()
            .map(|position| position.line + 1)
            .unwrap_or(1);
        Ok(EditorContext {
            buffer,
            path,
            revision,
            line,
        })
    }
}

fn tool_context(model: &Model) -> Option<ToolContext> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    let buffers = model
        .editor
        .file_buffer_ids()
        .into_iter()
        .filter_map(|buffer_id| {
            let path = model.editor.buffer_path(buffer_id)?.to_path_buf();
            let snapshot = model.editor.buffer_text_snapshot(buffer_id)?;
            path.starts_with(&root)
                .then_some(BufferContext { path, snapshot })
        })
        .collect();
    Some(ToolContext { root, buffers })
}

fn notify(model: &mut Model, message: String) -> Option<Effect> {
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(message),
        Some("Agent"),
    ))
}

struct FakeAgentRun {
    run_id: RunId,
    prompt: String,
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
const MAX_TOOL_OUTPUT: usize = 100 * 1024;

struct ProviderAgentRun {
    run_id: RunId,
    prompt: String,
    cancellation: CancellationToken,
    provider: Arc<dyn AgentProvider>,
    context: ToolContext,
    progress: RunProgress,
}

#[derive(Clone, Default)]
struct RunProgress {
    step: usize,
    history: Vec<ProviderMessage>,
    tool_calls: usize,
    tool_output: usize,
}

#[derive(Default)]
struct ProviderRound {
    text: String,
    calls: Vec<eggplant_agent::ToolCall>,
    terminal: Option<RoundTerminal>,
}

enum RoundTerminal {
    Complete,
    Failed(String),
}

enum RoundAction {
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

    fn prepare(
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

    fn with_results(self, results: Vec<ToolResult>) -> Self {
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

struct AgentCompletion {
    run_id: RunId,
    events: Vec<AgentEvent>,
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

impl Completion for AgentCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        self.events.into_iter().for_each(|event| match event {
            AgentEvent::TextDelta(delta) => {
                model.agent.session_mut().append_delta(self.run_id, &delta);
            }
            AgentEvent::Complete => {
                model.agent.session_mut().complete(self.run_id);
            }
            AgentEvent::ToolCalls(calls) => calls.into_iter().for_each(|call| {
                model
                    .agent
                    .session_mut()
                    .record_tool_call(self.run_id, &call);
            }),
            AgentEvent::ToolResults(results) => results.into_iter().for_each(|result| {
                model
                    .agent
                    .session_mut()
                    .complete_tool_call(self.run_id, result);
            }),
            AgentEvent::ApprovalRequired {
                approval,
                continuation,
            } => {
                if model
                    .agent
                    .session_mut()
                    .request_approval(self.run_id, approval)
                {
                    model.agent.suspend(self.run_id, continuation);
                    transition_to_approval(model);
                }
            }
            AgentEvent::Failed(error) => {
                model.agent.session_mut().fail(self.run_id, error);
            }
        });
        super::storage::persist_finalized(model)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use eggplant_agent::{MessageRole, RunStatus};
    use eggplant_agent_storage::SessionStore as _;

    use super::*;
    use crate::{
        manager::message_manager::MessageManager, startup::initial_model, test_utils::open_file,
    };

    fn perform(manager: &MessageManager, model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected agent effect");
        };
        manager.update(model, request.perform())
    }

    #[test]
    fn open_requires_a_workspace_file() {
        let mut empty = Model::default();
        assert!(open(&mut empty).is_some());
        assert!(!empty.agent.dialog_open());

        let workspace = tempfile::tempdir().unwrap();
        let external = tempfile::tempdir().unwrap();
        let path = external.path().join("dependency.rs");
        std::fs::write(&path, "fn dependency() {}\n").unwrap();
        let mut model = initial_model(Some(workspace.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        assert!(open(&mut model).is_some());
        assert!(!model.agent.dialog_open());

        let standalone = tempfile::tempdir().unwrap();
        let path = standalone.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(path)).unwrap();
        assert!(open(&mut model).is_some());
        assert!(!model.agent.dialog_open());
    }

    #[test]
    fn panel_input_shares_prompt_and_submits_through_the_same_run_workflow() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;

        focus_panel_input(&mut model);
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChat));
        assert_eq!(model.agent.prompt(), "");
        enter_insert_mode(&mut model);
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChatInsert));
        handle_input(&mut model, TextInputEvent::Character('?'));
        assert_eq!(model.agent.prompt(), "?");

        let effect = submit(&mut model).unwrap();
        let manager = MessageManager::default();
        let storage = perform(&manager, &mut model, effect).unwrap();
        perform(&manager, &mut model, storage);
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert!(
            model.agent.session().messages()[1]
                .text
                .contains("Simulated response to: ?")
        );

        enter_normal_mode(&mut model);
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChat));
        close_panel_input(&mut model);
        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);
        assert_ne!(model.context_stack.last(), Some(&Context::AgentChat));
    }

    #[test]
    fn panel_help_owns_input_and_closes_without_moving_focus() {
        let mut model = Model {
            current_focus: crate::data_object::FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.context_stack.push(Context::AgentChat);

        toggle_help(&mut model);
        assert!(model.agent.help_visible());
        assert!(model.modal.is_active(Context::AgentChatHelp));
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChatHelp));
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );

        toggle_help(&mut model);
        assert!(!model.agent.help_visible());
        assert!(!model.modal.is_active(Context::AgentChatHelp));
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChat));
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );

        toggle_help(&mut model);
        close_panel_input(&mut model);
        assert!(!model.agent.help_visible());
        assert!(!model.modal.is_active(Context::AgentChatHelp));
        assert_ne!(model.context_stack.last(), Some(&Context::AgentChatHelp));
    }

    #[test]
    fn panel_chat_requires_a_project_but_not_an_open_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        assert!(model.editor.active_buffer().is_none());
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;

        focus_panel_input(&mut model);
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChat));
        handle_input(&mut model, TextInputEvent::Character('x'));
        assert_eq!(model.agent.prompt(), "");
        enter_insert_mode(&mut model);
        for character in "explain @main.rs".chars() {
            handle_input(&mut model, TextInputEvent::Character(character));
        }
        let effect = submit(&mut model).unwrap();
        let manager = MessageManager::default();
        let storage = perform(&manager, &mut model, effect).unwrap();
        perform(&manager, &mut model, storage);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert!(
            model.agent.session().messages()[1]
                .text
                .contains("explain @main.rs")
        );
        assert!(
            model
                .agent
                .run_context(model.agent.session().run().unwrap().id)
                .is_none()
        );
    }

    #[test]
    fn prefill_is_editable_and_draft_state_survives_close_and_reopen() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "first\nsecond").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        model
            .editor
            .active_editor()
            .unwrap()
            .move_cursor_to(editor::DocumentPosition {
                line: 1,
                character: 0,
            })
            .unwrap();

        open(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:2");
        handle_input(&mut model, TextInputEvent::Character('?'));
        assert_eq!(model.agent.prompt(), "main.rs:2?");
        backspace(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:2");
        close(&mut model);
        open(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:2");

        for _ in "main.rs:2".chars() {
            backspace(&mut model);
        }
        close(&mut model);
        open(&mut model);
        assert_eq!(model.agent.prompt(), "");
    }

    #[test]
    fn prompt_cursor_moves_and_inserts_at_unicode_boundaries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "first\nsecond").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        open(&mut model);

        handle_input(
            &mut model,
            TextInputEvent::Cursor(crate::component::text_input::TextCursorMovement::Left),
        );
        handle_input(
            &mut model,
            TextInputEvent::Cursor(crate::component::text_input::TextCursorMovement::Left),
        );
        handle_input(&mut model, TextInputEvent::Character('界'));
        assert_eq!(model.agent.prompt(), "main.rs界:1");
        backspace(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:1");
        handle_input(&mut model, TextInputEvent::Character('X'));
        assert_eq!(model.agent.prompt(), "main.rsX:1");
        handle_input(
            &mut model,
            TextInputEvent::Cursor(crate::component::text_input::TextCursorMovement::Right),
        );
        handle_input(&mut model, TextInputEvent::Character('Y'));
        assert_eq!(model.agent.prompt(), "main.rsX:Y1");
    }

    #[test]
    fn streaming_continues_while_hidden_and_panel_toggle_preserves_focus() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        assert!(model.agent.session().is_running());
        close(&mut model);
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(model.agent.session().messages().len(), 2);
        assert_eq!(
            model.agent.session().messages()[1].text,
            "Simulated response to: main.rs:1"
        );
        toggle_panel(&mut model);
        assert_eq!(model.tools.active_tool, Some(ToolKind::Agent));
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChat));
        toggle_panel(&mut model);
        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);
        assert_ne!(model.context_stack.last(), Some(&Context::AgentChat));

        open(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:1");
        assert_eq!(model.agent.session().messages().len(), 2);
    }

    #[test]
    fn provider_failure_is_recorded_as_session_data() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        for _ in 0..model.agent.prompt().chars().count() {
            model.agent.backspace();
        }
        for character in "fail".chars() {
            model
                .agent
                .handle_input_event(TextInputEvent::Character(character));
        }
        let effect = submit(&mut model).unwrap();
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Failed("simulated provider failure".to_string())
        );
        assert_eq!(model.agent.session().messages().len(), 1);
    }

    #[test]
    fn finalized_fake_run_is_queryable_through_the_in_memory_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let store = Arc::new(eggplant_agent_storage::InMemoryStore::new());
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_session_store(store.clone());
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();

        let storage = perform(&manager, &mut model, effect).unwrap();
        perform(&manager, &mut model, storage);

        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(async {
                let recent = store
                    .recent_sessions(eggplant_agent_storage::RecentQuery::default())
                    .await
                    .unwrap();
                assert_eq!(recent.sessions.len(), 1);
                assert_eq!(recent.sessions[0].id, model.agent.session().id().clone());
                let transcript = store
                    .transcript(
                        model.agent.session().id().clone(),
                        eggplant_agent_storage::TranscriptQuery::default(),
                    )
                    .await
                    .unwrap();
                assert_eq!(transcript.messages.len(), 2);
                assert_eq!(transcript.messages[0].role, MessageRole::User);
                assert_eq!(transcript.messages[1].role, MessageRole::Assistant);
                assert!(transcript.messages[1].text.contains("Simulated response"));
                assert_eq!(transcript.runs[0].provider, "fake");
                assert!(
                    transcript.runs[0]
                        .editor_context
                        .as_ref()
                        .is_some_and(|context| context.contains("buffer_id"))
                );
            });
    }

    #[derive(Debug)]
    struct FailingStore;

    impl eggplant_agent_storage::SessionStore for FailingStore {
        fn finalize_run<'a>(
            &'a self,
            _run: eggplant_agent_storage::FinalizedRun,
            _cancellation: CancellationToken,
        ) -> eggplant_agent_storage::StoreFuture<'a, ()> {
            Box::pin(async {
                Err(eggplant_agent_storage::StorageError::Backend(
                    "disk full".to_string(),
                ))
            })
        }

        fn recent_sessions<'a>(
            &'a self,
            _query: eggplant_agent_storage::RecentQuery,
        ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::RecentPage> {
            Box::pin(async {
                Err(eggplant_agent_storage::StorageError::Backend(
                    "disk full".to_string(),
                ))
            })
        }

        fn transcript<'a>(
            &'a self,
            _session_id: eggplant_agent::SessionId,
            _query: eggplant_agent_storage::TranscriptQuery,
        ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::TranscriptPage>
        {
            Box::pin(async {
                Err(eggplant_agent_storage::StorageError::Backend(
                    "disk full".to_string(),
                ))
            })
        }

        fn search<'a>(
            &'a self,
            _query: eggplant_agent_storage::SearchQuery,
        ) -> eggplant_agent_storage::StoreFuture<'a, eggplant_agent_storage::SearchPage> {
            Box::pin(async {
                Err(eggplant_agent_storage::StorageError::Backend(
                    "disk full".to_string(),
                ))
            })
        }
    }

    #[test]
    fn finalization_failure_is_a_warning_not_a_provider_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_session_store(Arc::new(FailingStore));
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();

        let storage = perform(&manager, &mut model, effect).unwrap();
        perform(&manager, &mut model, storage);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert!(
            model
                .agent
                .storage_error()
                .is_some_and(|error| error.contains("disk full"))
        );
    }

    #[test]
    fn interrupt_preserves_partial_state_and_rejects_late_deltas() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        close(&mut model);
        open(&mut model);
        interrupt(&mut model);
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Cancelled
        );
        assert_eq!(model.agent.session().messages().len(), 1);
        assert_eq!(model.agent.session().messages()[0].role, MessageRole::User);
    }

    #[test]
    fn injected_provider_uses_the_shared_session_and_cancellation_token() {
        #[derive(Debug)]
        struct ScriptedProvider;

        impl AgentProvider for ScriptedProvider {
            fn stream<'a>(
                &'a self,
                request: AgentRequest,
                mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
            ) -> std::pin::Pin<
                Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
            > {
                Box::pin(async move {
                    if !request.cancellation.is_cancelled() {
                        on_event(AgentEvent::TextDelta("provider: ".to_string()));
                        on_event(AgentEvent::TextDelta(request.prompt));
                        on_event(AgentEvent::Complete);
                    }
                    Ok(())
                })
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_provider(Arc::new(ScriptedProvider));
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        perform(&manager, &mut model, effect);
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(model.agent.session().messages().len(), 2);
        assert!(
            model.agent.session().messages()[1]
                .text
                .starts_with("provider: ")
        );

        close(&mut model);
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        interrupt(&mut model);
        perform(&manager, &mut model, effect);
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Cancelled
        );
        assert_eq!(model.agent.session().messages().len(), 3);
    }

    #[test]
    fn provider_tool_calls_execute_and_continue_with_tool_history() {
        #[derive(Debug)]
        struct ToolCallingProvider {
            seen: Arc<std::sync::Mutex<Vec<ProviderMessage>>>,
        }

        impl AgentProvider for ToolCallingProvider {
            fn stream<'a>(
                &'a self,
                request: AgentRequest,
                mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
            ) -> std::pin::Pin<
                Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
            > {
                Box::pin(async move {
                    if request.history.is_empty() {
                        on_event(AgentEvent::ToolCalls(vec![eggplant_agent::ToolCall {
                            id: "read-1".to_string(),
                            name: "read".to_string(),
                            arguments: r#"{"path":"main.rs","limit":1}"#.to_string(),
                        }]));
                        on_event(AgentEvent::Complete);
                    } else {
                        self.seen.lock().unwrap().extend(request.history.clone());
                        on_event(AgentEvent::TextDelta("read complete".to_string()));
                        on_event(AgentEvent::Complete);
                    }
                    Ok(())
                })
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "hello").unwrap();
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model
            .agent
            .set_provider(Arc::new(ToolCallingProvider { seen: seen.clone() }));
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(model.agent.session().tool_events().len(), 1);
        let event = &model.agent.session().tool_events()[0];
        assert_eq!(event.name, "read");
        let result = event.result.as_ref().unwrap();
        assert_eq!(result.status, ToolResultStatus::Success);
        assert!(result.content.contains("hello"));
        assert_eq!(model.agent.session().messages()[1].text, "read complete");
        assert!(
            seen.lock()
                .unwrap()
                .iter()
                .any(|message| message.role == eggplant_agent::ProviderMessageRole::Tool)
        );
    }

    #[derive(Debug)]
    struct WriteProvider {
        call: eggplant_agent::ToolCall,
        read_after: bool,
        requests: Arc<std::sync::atomic::AtomicUsize>,
        seen: Arc<std::sync::Mutex<Vec<ProviderMessage>>>,
    }

    impl WriteProvider {
        fn new(
            call: eggplant_agent::ToolCall,
            read_after: bool,
        ) -> (
            Self,
            Arc<std::sync::Mutex<Vec<ProviderMessage>>>,
            Arc<std::sync::atomic::AtomicUsize>,
        ) {
            let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
            (
                Self {
                    call,
                    read_after,
                    requests: requests.clone(),
                    seen: seen.clone(),
                },
                seen,
                requests,
            )
        }
    }

    impl AgentProvider for WriteProvider {
        fn stream<'a>(
            &'a self,
            request: AgentRequest,
            mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
        > {
            Box::pin(async move {
                let round = self
                    .requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if round == 0 {
                    on_event(AgentEvent::ToolCalls(vec![self.call.clone()]));
                } else {
                    self.seen.lock().unwrap().extend(request.history.clone());
                    if round == 1 && self.read_after {
                        let path = serde_json::from_str::<serde_json::Value>(&self.call.arguments)
                            .ok()
                            .and_then(|value| value["path"].as_str().map(str::to_string))
                            .unwrap_or_default();
                        on_event(AgentEvent::ToolCalls(vec![eggplant_agent::ToolCall {
                            id: "read-after-write".to_string(),
                            name: "read".to_string(),
                            arguments: serde_json::json!({"path": path}).to_string(),
                        }]));
                    } else {
                        on_event(AgentEvent::TextDelta("write resolved".to_string()));
                    }
                }
                on_event(AgentEvent::Complete);
                Ok(())
            })
        }
    }

    fn write_call(model: &Model, buffer_id: editor::BufferId) -> eggplant_agent::ToolCall {
        let snapshot = model.editor.buffer_text_snapshot(buffer_id).unwrap();
        let root = model.workspace.as_ref().unwrap().root();
        let path = model.editor.buffer_path(buffer_id).unwrap();
        let relative = path.strip_prefix(root).unwrap();
        eggplant_agent::ToolCall {
            id: "write-1".to_string(),
            name: "write".to_string(),
            arguments: serde_json::json!({
                "buffer_id": buffer_id.as_raw(),
                "path": relative,
                "expected_revision": snapshot.revision,
                "old_text": "before",
                "new_text": "after",
            })
            .to_string(),
        }
    }

    fn begin_write(model: &mut Model) {
        let manager = MessageManager::default();
        open(model);
        let effect = submit(model).unwrap();
        perform(&manager, model, effect);
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::WaitingForApproval
        );
    }

    fn perform_resolution(model: &mut Model, effect: Effect) -> Vec<editor::BufferId> {
        match effect {
            Effect::Save(request) => vec![request.buffer_id()],
            Effect::Feature(request) => {
                MessageManager::default().update(model, request.perform());
                Vec::new()
            }
            Effect::Batch(effects) => effects
                .into_iter()
                .flat_map(|effect| perform_resolution(model, effect))
                .collect(),
            Effect::OpenDocument(_) => Vec::new(),
        }
    }

    #[test]
    fn approval_is_required_once_before_an_undoable_write_and_resume() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "before").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = open_file(&mut model, &path);
        let (provider, seen, requests) = WriteProvider::new(write_call(&model, buffer), false);
        model.agent.set_provider(Arc::new(provider));
        begin_write(&mut model);

        assert!(
            model
                .modal
                .is_active(crate::data_object::Context::AgentApproval)
        );
        assert_eq!(
            model.editor.buffer_text_snapshot(buffer).unwrap().text,
            "before"
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);

        let effect = approve_write(&mut model).unwrap();
        let saves = perform_resolution(&mut model, effect);

        assert_eq!(saves, vec![buffer]);
        assert_eq!(
            model.editor.buffer_text_snapshot(buffer).unwrap().text,
            "after"
        );
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert!(model.agent.session().pending_approval().is_none());
        assert!(
            model
                .modal
                .is_active(crate::data_object::Context::AgentChatDialog)
        );
        assert!(approve_write(&mut model).is_none());
        assert!(seen.lock().unwrap().iter().any(|message| {
            message.role == eggplant_agent::ProviderMessageRole::Tool
                && message.content.contains("not claimed to be saved")
        }));
    }

    #[test]
    fn rejection_and_cancellation_do_not_mutate_or_implicitly_resume() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "before").unwrap();
        let mut rejected = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = open_file(&mut rejected, &path);
        let (provider, seen, requests) = WriteProvider::new(write_call(&rejected, buffer), false);
        rejected.agent.set_provider(Arc::new(provider));
        begin_write(&mut rejected);
        let effect = reject_write(&mut rejected).unwrap();
        assert!(perform_resolution(&mut rejected, effect).is_empty());

        assert_eq!(
            rejected.editor.buffer_text_snapshot(buffer).unwrap().text,
            "before"
        );
        assert_eq!(
            rejected.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert!(seen.lock().unwrap().iter().any(|message| {
            message.role == eggplant_agent::ProviderMessageRole::Tool
                && message.content.contains("rejected")
        }));

        let mut cancelled = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = open_file(&mut cancelled, &path);
        let (provider, _seen, requests) = WriteProvider::new(write_call(&cancelled, buffer), false);
        cancelled.agent.set_provider(Arc::new(provider));
        begin_write(&mut cancelled);
        let storage = interrupt(&mut cancelled).unwrap();
        perform(&MessageManager::default(), &mut cancelled, storage);

        assert_eq!(
            cancelled.editor.buffer_text_snapshot(buffer).unwrap().text,
            "before"
        );
        assert_eq!(
            cancelled.agent.session().run().unwrap().status,
            RunStatus::Cancelled
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert!(approve_write(&mut cancelled).is_none());
        assert_eq!(
            cancelled.agent.session().tool_events()[0]
                .result
                .as_ref()
                .unwrap()
                .status,
            ToolResultStatus::Cancelled
        );
    }

    #[test]
    fn hiding_and_reopening_preserves_the_identical_approval() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "before").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = open_file(&mut model, &path);
        let (provider, _seen, requests) = WriteProvider::new(write_call(&model, buffer), false);
        model.agent.set_provider(Arc::new(provider));
        begin_write(&mut model);
        let approval = model.agent.session().pending_approval().unwrap().clone();

        close(&mut model);
        assert!(!model.agent.dialog_open());
        assert_eq!(model.agent.session().pending_approval(), Some(&approval));
        assert_eq!(
            model.editor.buffer_text_snapshot(buffer).unwrap().text,
            "before"
        );

        open(&mut model);
        assert!(model.agent.dialog_open());
        assert!(
            model
                .modal
                .is_active(crate::data_object::Context::AgentApproval)
        );
        assert_eq!(model.agent.session().pending_approval(), Some(&approval));
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);
        assert_eq!(
            model.editor.buffer_text_snapshot(buffer).unwrap().text,
            "before"
        );
    }

    #[test]
    fn stale_revision_returns_a_conflict_without_overwriting_user_text() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "before").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let buffer = open_file(&mut model, &path);
        let (provider, seen, _requests) = WriteProvider::new(write_call(&model, buffer), false);
        model.agent.set_provider(Arc::new(provider));
        begin_write(&mut model);
        model.editor.enter_insert_mode();
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text("user ")
            .unwrap();
        model.editor.enter_normal_mode();

        let effect = approve_write(&mut model).unwrap();
        assert!(perform_resolution(&mut model, effect).is_empty());

        assert_eq!(
            model.editor.buffer_text_snapshot(buffer).unwrap().text,
            "user before"
        );
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(
            model.agent.session().tool_events()[0]
                .result
                .as_ref()
                .unwrap()
                .status,
            ToolResultStatus::Error
        );
        assert!(seen.lock().unwrap().iter().any(|message| {
            message.role == eggplant_agent::ProviderMessageRole::Tool
                && message.content.contains("revision changed")
        }));
    }

    #[test]
    fn inactive_write_saves_by_id_and_follow_up_reads_fresh_text() {
        let directory = tempfile::tempdir().unwrap();
        let first_path = directory.path().join("first.txt");
        let second_path = directory.path().join("second.txt");
        std::fs::write(&first_path, "before").unwrap();
        std::fs::write(&second_path, "second").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        let first = open_file(&mut model, &first_path);
        let second = open_file(&mut model, &second_path);
        let (provider, seen, requests) = WriteProvider::new(write_call(&model, first), true);
        model.agent.set_provider(Arc::new(provider));
        begin_write(&mut model);

        let effect = approve_write(&mut model).unwrap();
        let saves = perform_resolution(&mut model, effect);

        assert_eq!(saves, vec![first]);
        assert_eq!(model.editor.active_buffer(), Some(second));
        assert_eq!(
            model.editor.buffer_text_snapshot(first).unwrap().text,
            "after"
        );
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 3);
        assert!(seen.lock().unwrap().iter().any(|message| {
            message.role == eggplant_agent::ProviderMessageRole::Tool
                && message.content.contains("1:after")
        }));
    }

    #[derive(Debug)]
    struct QueuedCallProvider {
        fail: bool,
        requests: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl AgentProvider for QueuedCallProvider {
        fn stream<'a>(
            &'a self,
            _request: AgentRequest,
            mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
        > {
            Box::pin(async move {
                self.requests
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                on_event(AgentEvent::ToolCalls(vec![eggplant_agent::ToolCall {
                    id: "read-1".to_string(),
                    name: "read".to_string(),
                    arguments: r#"{"path":"main.rs"}"#.to_string(),
                }]));
                if self.fail {
                    on_event(AgentEvent::Failed(
                        "OpenAI response stopped for length".to_string(),
                    ));
                }
                on_event(AgentEvent::Complete);
                Ok(())
            })
        }
    }

    fn queued_call_model(
        fail: bool,
    ) -> (
        Model,
        Arc<std::sync::atomic::AtomicUsize>,
        tempfile::TempDir,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "hello").unwrap();
        let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_provider(Arc::new(QueuedCallProvider {
            fail,
            requests: requests.clone(),
        }));
        open_file(&mut model, &path);
        open(&mut model);
        (model, requests, directory)
    }

    #[test]
    fn failed_round_never_executes_queued_tool_calls() {
        let (mut model, requests, _directory) = queued_call_model(true);
        let effect = submit(&mut model).unwrap();
        perform(&MessageManager::default(), &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Failed("OpenAI response stopped for length".to_string())
        );
        assert!(model.agent.session().tool_events().is_empty());
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn cancellation_discards_queued_tool_calls() {
        let (mut model, requests, _directory) = queued_call_model(false);
        let effect = submit(&mut model).unwrap();
        interrupt(&mut model);
        perform(&MessageManager::default(), &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Cancelled
        );
        assert!(model.agent.session().tool_events().is_empty());
        assert_eq!(requests.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn exhausted_output_budget_rejects_another_tool_round() {
        let progress = RunProgress {
            tool_output: MAX_TOOL_OUTPUT,
            ..RunProgress::default()
        };
        let round = ProviderRound {
            calls: vec![eggplant_agent::ToolCall {
                id: "read-1".to_string(),
                name: "read".to_string(),
                arguments: r#"{"path":"main.rs"}"#.to_string(),
            }],
            ..ProviderRound::default()
        };
        let action = progress.prepare("read", round, Ok(()), false);

        assert!(matches!(
            action,
            RoundAction::Stop(Some(AgentEvent::Failed(message)))
                if message == "tool output limit reached"
        ));
    }

    #[test]
    fn repeated_tool_requests_stop_at_the_step_limit() {
        #[derive(Debug)]
        struct RepeatingProvider;

        impl AgentProvider for RepeatingProvider {
            fn stream<'a>(
                &'a self,
                _request: AgentRequest,
                mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
            ) -> std::pin::Pin<
                Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
            > {
                Box::pin(async move {
                    on_event(AgentEvent::ToolCalls(vec![eggplant_agent::ToolCall {
                        id: "read-1".to_string(),
                        name: "read".to_string(),
                        arguments: r#"{"path":"main.rs"}"#.to_string(),
                    }]));
                    Ok(())
                })
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "hello").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_provider(Arc::new(RepeatingProvider));
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Failed("tool step limit reached".to_string())
        );
    }
}
