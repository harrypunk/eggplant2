use std::{future::Future, pin::Pin, sync::Arc};

use eggplant_agent::{
    AgentEvent, AgentProvider, AgentRequest, CancellationToken, ProviderMessage, RunId, ToolResult,
    ToolResultStatus,
};

use super::{AgentContextProvider, EditorContext, tools::ToolContext};
use crate::{
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
            CommandId("agent_toggle_panel"),
            "toggle agent tool panel",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn register_text_input(input: &mut TextInputRegistry) -> color_eyre::Result<()> {
    input.register(Context::AgentChatDialog, insert)
}

pub fn register_ui(ui: &mut UiRegistry) -> color_eyre::Result<()> {
    ui.register_overlay("agent", super::AgentDialog::overlay)
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "agent_open" => open(model),
        "agent_close" => close(model),
        "agent_submit" => submit(model),
        "agent_backspace" => backspace(model),
        "agent_interrupt" => interrupt(model),
        "agent_toggle_panel" => toggle_panel(model),
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
    let relative = model
        .workspace
        .as_ref()
        .and_then(|workspace| context.path.strip_prefix(workspace.root()).ok())
        .unwrap_or(&context.path);
    model.agent.open_dialog(
        context.clone(),
        format!("{}:{}", relative.display(), context.line),
    );
    if !model.modal.open(Context::AgentChatDialog) {
        return None;
    }
    model.agent.open_dialog(
        context.clone(),
        format!("{}:{}", relative.display(), context.line),
    );
    model.context_stack.push(Context::AgentChatDialog);
    None
}

fn close(model: &mut Model) -> Option<Effect> {
    if !model.agent.dialog_open() {
        return None;
    }
    model.agent.close_dialog();
    model.modal.close(Context::AgentChatDialog);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatDialog);
    None
}

fn submit(model: &mut Model) -> Option<Effect> {
    if !model.agent.dialog_open() {
        return None;
    }
    let prompt = model.agent.prompt().trim().to_string();
    if prompt.is_empty() {
        return notify(model, "Enter a prompt".to_string());
    }
    if model.agent.session().is_running() {
        return notify(model, "Agent is already running".to_string());
    }
    if let Err(message) = model.agent_context() {
        return notify(model, message);
    }
    let run_id = model.agent.begin_prompt()?;
    if let Some(provider) = model.agent.provider() {
        let cancellation = model.agent.begin_cancellation();
        let context = ToolContext {
            root: model.workspace.as_ref()?.root().to_path_buf(),
            active_path: model
                .editor
                .active_buffer()
                .and_then(|buffer| model.editor.buffer_path(buffer))
                .map(std::path::Path::to_path_buf),
            active_snapshot: model.editor.active_text_snapshot(),
        };
        return Some(Effect::feature(ProviderAgentRun {
            run_id,
            prompt,
            cancellation,
            provider,
            context,
        }));
    }
    Some(Effect::feature(FakeAgentRun { run_id, prompt }))
}

fn insert(model: &mut Model, character: char) -> Option<Effect> {
    model.agent.insert(character);
    None
}

fn backspace(model: &mut Model) -> Option<Effect> {
    model.agent.backspace();
    None
}

fn interrupt(model: &mut Model) -> Option<Effect> {
    model.agent.cancel_active();
    model.agent.session_mut().cancel();
    None
}

fn toggle_panel(model: &mut Model) -> Option<Effect> {
    if model.tools.visibility == Visibility::Displayed
        && model.tools.active_tool == Some(ToolKind::Agent)
    {
        model.tools.visibility = Visibility::Hidden;
    } else {
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
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
}

#[derive(Default)]
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
                AgentEvent::ToolResults(_) => (self, None),
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

fn require(condition: bool, message: &str) -> Result<(), String> {
    condition.then_some(()).ok_or_else(|| message.to_string())
}

impl ProviderAgentRun {
    async fn drive(self, mut emit: impl FnMut(AgentEvent) + Send + 'static) {
        self.drive_round(RunProgress::default(), &mut emit).await;
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
                    let results = self.execute_calls(&calls, progress.tool_output);
                    emit(AgentEvent::ToolResults(results.clone()));
                    match self.cancellation.is_cancelled() {
                        true => {}
                        false => {
                            self.drive_round(progress.with_results(results), emit).await;
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
    ) -> Vec<ToolResult> {
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
                let result =
                    super::tools::bound_result(result, MAX_TOOL_OUTPUT.saturating_sub(*used));
                *used = used.saturating_add(result.content.len());
                Some(result)
            })
            .collect()
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
        for event in self.events {
            match event {
                AgentEvent::TextDelta(delta) => {
                    model.agent.session_mut().append_delta(self.run_id, &delta);
                }
                AgentEvent::Complete => {
                    model.agent.session_mut().complete(self.run_id);
                }
                AgentEvent::ToolCalls(calls) => {
                    for call in calls {
                        model
                            .agent
                            .session_mut()
                            .record_tool_call(self.run_id, &call);
                    }
                }
                AgentEvent::ToolResults(results) => {
                    for result in results {
                        model
                            .agent
                            .session_mut()
                            .complete_tool_call(self.run_id, result);
                    }
                }
                AgentEvent::Failed(error) => {
                    model.agent.session_mut().fail(self.run_id, error);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use eggplant_agent::{MessageRole, RunStatus};

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
        insert(&mut model, '?');
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
        let focus = model.current_focus;
        toggle_panel(&mut model);
        assert_eq!(model.tools.active_tool, Some(ToolKind::Agent));
        assert_eq!(model.current_focus, focus);

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
            model.agent.insert(character);
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
