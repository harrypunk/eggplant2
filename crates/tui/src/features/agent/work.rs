use std::{future::Future, sync::Arc};

use eggplant_agent::{AgentEvent, AgentProvider, AgentRequest, CancellationToken, RunId};

use super::{AgentContextProvider, EditorContext};
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
        return Some(Effect::feature(ProviderAgentRun {
            run_id,
            prompt,
            cancellation,
            provider,
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

struct ProviderAgentRun {
    run_id: RunId,
    prompt: String,
    cancellation: CancellationToken,
    provider: Arc<dyn AgentProvider>,
}

impl ProviderAgentRun {
    fn perform_async(self) -> impl Future<Output = Vec<AgentEvent>> {
        let events = Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = events.clone();
        let provider = self.provider.clone();
        let request = AgentRequest {
            prompt: self.prompt,
            cancellation: self.cancellation,
        };
        async move {
            let result = provider
                .stream(
                    request,
                    Box::new(move |event| {
                        if let Ok(mut current) = output.lock() {
                            current.push(event);
                        }
                    }),
                )
                .await;
            let mut events = events.lock().unwrap().clone();
            if let Err(error) = result {
                events.push(AgentEvent::Failed(error.0));
            }
            events
        }
    }
}

impl FeatureEffect for ProviderAgentRun {
    fn perform(self: Box<Self>) -> crate::data_object::Message {
        let run_id = self.run_id;
        let events = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("agent runtime must build")
            .block_on(self.perform_async());
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
            let request = AgentRequest {
                prompt: self.prompt,
                cancellation: self.cancellation,
            };
            let result = self
                .provider
                .stream(
                    request,
                    Box::new(move |event| {
                        let _ = event_sender.send(crate::data_object::Message::FeatureCompletion(
                            FeatureCompletion::new(AgentCompletion {
                                run_id,
                                events: vec![event],
                            }),
                        ));
                    }),
                )
                .await;
            if let Err(error) = result {
                let _ = sender.send(crate::data_object::Message::FeatureCompletion(
                    FeatureCompletion::new(AgentCompletion {
                        run_id,
                        events: vec![AgentEvent::Failed(error.0)],
                    }),
                ));
            }
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
}
