use eggplant_agent::{ToolResult, ToolResultStatus};

use crate::manager::effect::Effect;

use super::{AgentState, tools::ToolContext};

/// Root-supplied capabilities needed to accept and dispatch an Agent submission.
pub(crate) trait SubmissionContext {
    fn agent(&self) -> &AgentState;
    fn agent_mut(&mut self) -> &mut AgentState;
    fn submission_active(&self) -> bool;
    fn project_available(&self) -> bool;
    fn tool_context(&self) -> Option<ToolContext>;
    fn open_provider_login(&mut self) -> Option<Effect>;
    fn open_provider_logout(&mut self) -> Option<Effect>;
    fn open_model_selector(&mut self) -> Option<Effect>;
    fn notify(&mut self, message: String) -> Option<Effect>;
}

/// Root coordination required after interrupting the active Agent run.
pub(crate) trait InterruptionContext {
    fn agent(&self) -> &AgentState;
    fn agent_mut(&mut self) -> &mut AgentState;
    fn transition_to_chat(&mut self);
    fn persist_finalized(&mut self) -> Option<Effect>;
}

enum Submission {
    Inactive,
    Command(super::PromptCommand),
    Rejected(SubmissionRejection),
    Accepted(AcceptedSubmission),
}

enum SubmissionRejection {
    EmptyPrompt,
    AlreadyRunning,
    MissingProject,
    MissingProvider,
}

impl SubmissionRejection {
    fn message(self) -> &'static str {
        match self {
            Self::EmptyPrompt => "Enter a prompt",
            Self::AlreadyRunning => "Agent is already running",
            Self::MissingProject => "Open a project to use the agent",
            Self::MissingProvider => "No model available; use /provider or Space a i to log in",
        }
    }
}

struct AcceptedSubmission {
    prompt: String,
}

impl Submission {
    fn evaluate(context: &impl SubmissionContext) -> Self {
        if !context.submission_active() {
            return Self::Inactive;
        }
        let prompt = context.agent().prompt().trim();
        if prompt.starts_with('/') {
            return context
                .agent()
                .command_state()
                .selected()
                .or_else(|| super::PromptCommand::from_input(prompt))
                .map_or(Self::Inactive, Self::Command);
        }
        if let Some(rejection) = rejection(context, prompt) {
            return Self::Rejected(rejection);
        }
        Self::Accepted(AcceptedSubmission {
            prompt: prompt.to_string(),
        })
    }
}

fn rejection(context: &impl SubmissionContext, prompt: &str) -> Option<SubmissionRejection> {
    if prompt.is_empty() {
        return Some(SubmissionRejection::EmptyPrompt);
    }
    if context.agent().session().is_running() {
        return Some(SubmissionRejection::AlreadyRunning);
    }
    if !context.project_available() {
        return Some(SubmissionRejection::MissingProject);
    }
    if context.agent().provider().is_none() {
        return Some(SubmissionRejection::MissingProvider);
    }
    None
}

pub(crate) fn submit(context: &mut impl SubmissionContext) -> Option<Effect> {
    match Submission::evaluate(context) {
        Submission::Inactive => None,
        Submission::Command(command) => execute_prompt_command(context, command),
        Submission::Rejected(rejection) => context.notify(rejection.message().to_string()),
        Submission::Accepted(accepted) => start_run(context, accepted),
    }
}

fn start_run(context: &mut impl SubmissionContext, accepted: AcceptedSubmission) -> Option<Effect> {
    let run_id = context.agent_mut().begin_prompt()?;
    let provider = context.agent().provider()?;
    let cancellation = context.agent_mut().begin_cancellation();
    Some(Effect::feature(ProviderAgentRun {
        run_id,
        prompt: accepted.prompt,
        cancellation,
        provider,
        context: context.tool_context()?,
        progress: RunProgress::default(),
    }))
}

fn execute_prompt_command(
    context: &mut impl SubmissionContext,
    command: super::PromptCommand,
) -> Option<Effect> {
    context.agent_mut().complete_prompt_command();
    match command {
        super::PromptCommand::ProviderLogin => context.open_provider_login(),
        super::PromptCommand::Logout => context.open_provider_logout(),
        super::PromptCommand::Model => context.open_model_selector(),
        super::PromptCommand::Thinking => {
            context.notify(format!("{} is not implemented yet", command.description()))
        }
    }
}

pub(crate) fn interrupt(context: &mut impl InterruptionContext) -> Option<Effect> {
    let pending = context.agent().session().pending_approval().cloned();
    let run_id = context.agent().session().run().map(|run| run.id);
    context.agent().cancel_active();
    context.agent_mut().session_mut().cancel();
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
            context
                .agent_mut()
                .session_mut()
                .complete_tool_call(run_id, result);
        });
    context.agent_mut().clear_suspension();
    context.transition_to_chat();
    context.persist_finalized()
}

#[cfg(test)]
pub(crate) use super::run::{MAX_TOOL_OUTPUT, ProviderRound, RoundAction};
pub(crate) use super::run::{ProviderAgentRun, RunProgress};

#[cfg(test)]
mod tests {
    use std::{future::Future, sync::Arc};

    use eggplant_agent::{
        AgentEvent, AgentProvider, AgentRequest, CancellationToken, MessageRole, ProviderMessage,
        RunStatus,
    };
    use eggplant_agent_storage::SessionStore as _;

    use super::*;
    use crate::agent::approval::{approve_write, reject_write};
    use crate::{
        agent::{
            commands::{handle_command, handle_input},
            dialog::{close, open},
            panel::{
                close_panel_input, enter_insert_mode, enter_normal_mode, focus_panel_input,
                toggle_help, toggle_panel,
            },
        },
        component::text_input::TextInputEvent,
        data_object::{CommandId, Context, Model, Visibility},
        manager::message_manager::MessageManager,
        startup::initial_model as build_model,
        test_utils::input_context,
        test_utils::open_file,
        tools::ToolKind,
    };

    fn perform(manager: &MessageManager, model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected agent effect");
        };
        manager.update(model, request.perform())
    }

    #[derive(Debug)]
    struct TestProvider;

    impl AgentProvider for TestProvider {
        fn stream<'a>(
            &'a self,
            request: AgentRequest,
            mut on_event: Box<dyn FnMut(AgentEvent) + Send + 'a>,
        ) -> std::pin::Pin<
            Box<dyn Future<Output = Result<(), eggplant_agent::AgentError>> + Send + 'a>,
        > {
            Box::pin(async move {
                if request.prompt.eq_ignore_ascii_case("fail") {
                    on_event(AgentEvent::Failed("injected provider failure".to_string()));
                } else {
                    on_event(AgentEvent::TextDelta("Test response to: ".to_string()));
                    on_event(AgentEvent::TextDelta(request.prompt));
                }
                on_event(AgentEvent::Complete);
                Ok(())
            })
        }
    }

    fn initial_model(path: Option<std::path::PathBuf>) -> color_eyre::Result<Model> {
        let mut model = build_model(path)?;
        model.agent.set_provider(Arc::new(TestProvider));
        model
            .agent
            .set_provider_metadata("test".to_string(), Some("echo".to_string()));
        Ok(model)
    }

    #[derive(Default)]
    struct FakeWork {
        agent: AgentState,
        active: bool,
        project: bool,
        opened: Option<crate::agent::PromptCommand>,
        notifications: Vec<String>,
        chat_restored: bool,
        finalized: bool,
    }

    impl SubmissionContext for FakeWork {
        fn agent(&self) -> &AgentState {
            &self.agent
        }

        fn agent_mut(&mut self) -> &mut AgentState {
            &mut self.agent
        }

        fn submission_active(&self) -> bool {
            self.active
        }

        fn project_available(&self) -> bool {
            self.project
        }

        fn tool_context(&self) -> Option<ToolContext> {
            Some(ToolContext::default())
        }

        fn open_provider_login(&mut self) -> Option<Effect> {
            self.opened = Some(crate::agent::PromptCommand::ProviderLogin);
            None
        }

        fn open_provider_logout(&mut self) -> Option<Effect> {
            self.opened = Some(crate::agent::PromptCommand::Logout);
            None
        }

        fn open_model_selector(&mut self) -> Option<Effect> {
            self.opened = Some(crate::agent::PromptCommand::Model);
            None
        }

        fn notify(&mut self, message: String) -> Option<Effect> {
            self.notifications.push(message);
            None
        }
    }

    impl InterruptionContext for FakeWork {
        fn agent(&self) -> &AgentState {
            &self.agent
        }

        fn agent_mut(&mut self) -> &mut AgentState {
            &mut self.agent
        }

        fn transition_to_chat(&mut self) {
            self.chat_restored = true;
        }

        fn persist_finalized(&mut self) -> Option<Effect> {
            self.finalized = true;
            None
        }
    }

    #[test]
    fn submission_policy_runs_without_the_application_model() {
        let mut context = FakeWork::default();
        context.agent.prompt.set_value("hello");

        assert!(submit(&mut context).is_none());
        assert_eq!(context.agent.prompt(), "hello");
        assert!(context.notifications.is_empty());

        context.active = true;
        assert!(submit(&mut context).is_none());
        assert_eq!(context.notifications, ["Open a project to use the agent"]);
        assert_eq!(context.agent.prompt(), "hello");
        assert!(context.agent.session().run().is_none());

        context.project = true;
        context.agent.set_provider(Arc::new(TestProvider));
        assert!(submit(&mut context).is_some());
        assert_eq!(context.agent.prompt(), "");
        assert!(context.agent.session().is_running());
    }

    #[test]
    fn missing_provider_preserves_the_prompt_and_suggests_login() {
        let mut context = FakeWork {
            active: true,
            project: true,
            ..FakeWork::default()
        };
        context.agent.prompt.set_value("hello");

        assert!(submit(&mut context).is_none());

        assert_eq!(
            context.notifications,
            ["No model available; use /provider or Space a i to log in"]
        );
        assert_eq!(context.agent.prompt(), "hello");
        assert!(context.agent.session().run().is_none());
    }

    #[test]
    fn command_and_interruption_follow_ups_use_the_abstract_context() {
        let mut context = FakeWork {
            active: true,
            project: true,
            ..FakeWork::default()
        };
        context.agent.prompt.set_value("/provider ");
        assert_eq!(context.agent.command_state().selected(), None);

        assert!(submit(&mut context).is_none());
        assert_eq!(
            context.opened,
            Some(crate::agent::PromptCommand::ProviderLogin)
        );
        assert_eq!(context.agent.prompt(), "");

        context.agent.prompt.set_value("/provider");
        assert!(
            context
                .agent
                .quick_fill_prompt_command(crate::agent::PromptCommandMovement::Current)
        );
        context
            .agent
            .handle_input_event(TextInputEvent::Insert(' '));
        assert_eq!(context.agent.command_state().selected(), None);
        context.agent.backspace();
        assert_eq!(context.agent.prompt(), "/provider");
        context.opened = None;
        assert!(submit(&mut context).is_none());
        assert_eq!(
            context.opened,
            Some(crate::agent::PromptCommand::ProviderLogin)
        );
        assert_eq!(context.agent.prompt(), "");

        context.agent.prompt.set_value("next");
        context.agent.begin_prompt().unwrap();
        assert!(interrupt(&mut context).is_none());
        assert_eq!(
            context.agent.session().run().unwrap().status,
            RunStatus::Cancelled
        );
        assert!(context.chat_restored);
        assert!(context.finalized);
    }

    #[test]
    fn dialog_requires_a_project_but_not_an_open_workspace_file() {
        let mut empty = Model::default();
        assert!(open(&mut empty).is_some());
        assert!(!empty.agent.dialog_open());

        let workspace = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(workspace.path().to_path_buf())).unwrap();
        assert!(open(&mut model).is_none());
        assert!(model.agent.dialog_open());
        assert_eq!(model.agent.prompt(), "");
        assert!(model.agent.context().is_none());
        close(&mut model);

        let external = tempfile::tempdir().unwrap();
        let path = external.path().join("dependency.rs");
        std::fs::write(&path, "fn dependency() {}\n").unwrap();
        open_file(&mut model, &path);
        assert!(open(&mut model).is_none());
        assert!(model.agent.dialog_open());
        assert_eq!(model.agent.prompt(), "");
        assert!(model.agent.context().is_none());
        close(&mut model);

        let standalone = tempfile::tempdir().unwrap();
        let path = standalone.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(path)).unwrap();
        assert!(open(&mut model).is_some());
        assert!(!model.agent.dialog_open());
    }

    #[test]
    fn dialog_submits_without_an_active_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open(&mut model);
        for character in "hello".chars() {
            handle_input(&mut model, TextInputEvent::Insert(character));
        }

        let effect = submit(&mut model).unwrap();
        let manager = MessageManager::default();
        if let Some(storage) = perform(&manager, &mut model, effect) {
            perform(&manager, &mut model, storage);
        }

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert!(
            model.agent.session().messages()[1]
                .text
                .contains("Test response to: hello")
        );
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
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        assert_eq!(model.agent.prompt(), "");
        enter_insert_mode(&mut model);
        assert_eq!(input_context(&model), Some(Context::AgentChatInsert));
        handle_input(&mut model, TextInputEvent::Insert('?'));
        assert_eq!(model.agent.prompt(), "?");

        let effect = submit(&mut model).unwrap();
        let manager = MessageManager::default();
        if let Some(storage) = perform(&manager, &mut model, effect) {
            perform(&manager, &mut model, storage);
        }
        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Complete
        );
        assert!(
            model.agent.session().messages()[1]
                .text
                .contains("Test response to: ?")
        );

        enter_normal_mode(&mut model);
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        close_panel_input(&mut model);
        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);
        assert_ne!(input_context(&model), Some(Context::AgentChat));
    }

    #[test]
    fn panel_help_owns_input_and_closes_without_moving_focus() {
        let mut model = Model {
            current_focus: crate::data_object::FocusTarget::ToolsPanel,
            ..Model::default()
        };
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;

        toggle_help(&mut model);
        assert!(model.agent.help_visible());
        assert!(model.modal.is_active(Context::AgentChatHelp));
        assert_eq!(input_context(&model), Some(Context::AgentChatHelp));
        assert!(model.context_stack.is_empty());
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );

        toggle_help(&mut model);
        assert!(!model.agent.help_visible());
        assert!(!model.modal.is_active(Context::AgentChatHelp));
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );

        toggle_help(&mut model);
        close_panel_input(&mut model);
        assert!(!model.agent.help_visible());
        assert!(!model.modal.is_active(Context::AgentChatHelp));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn panel_chat_requires_a_project_but_not_an_open_buffer() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        assert!(model.editor.active_buffer().is_none());
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;

        focus_panel_input(&mut model);
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        handle_input(&mut model, TextInputEvent::Insert('x'));
        assert_eq!(model.agent.prompt(), "");
        enter_insert_mode(&mut model);
        for character in "explain @main.rs".chars() {
            handle_input(&mut model, TextInputEvent::Insert(character));
        }
        let effect = submit(&mut model).unwrap();
        let manager = MessageManager::default();
        if let Some(storage) = perform(&manager, &mut model, effect) {
            perform(&manager, &mut model, storage);
        }

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
    fn slash_prompt_commands_do_not_start_an_agent_run() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        focus_panel_input(&mut model);
        enter_insert_mode(&mut model);
        model.agent.prompt.set_value("/provide");

        submit(&mut model);
        assert!(!model.provider_login.is_open());
        assert_eq!(model.agent.prompt(), "/provide");
        assert!(model.agent.session().run().is_none());

        handle_command(&mut model, CommandId("agent_command_quick_fill"));
        assert_eq!(model.agent.prompt(), "/provider");
        submit(&mut model);
        assert!(model.provider_login.is_open());
        assert_eq!(input_context(&model), Some(Context::ProviderLogin));
        assert_eq!(model.agent.prompt(), "");
        assert_eq!(model.agent.command_state().selected(), None);
        assert!(model.context_stack.is_empty());
        assert!(model.agent.session().run().is_none());

        crate::provider_login::handle_command(&mut model, CommandId("provider_login_close"));
        assert_eq!(input_context(&model), Some(Context::AgentChatInsert));
        assert_eq!(model.agent.prompt(), "");

        model.agent.prompt.set_value("/unknown");
        handle_input(&mut model, TextInputEvent::DeleteBackward);
        submit(&mut model);
        assert_eq!(model.agent.prompt(), "/unknow");
        assert!(model.agent.session().run().is_none());
        assert!(model.notifications.current().is_none());
    }

    #[test]
    fn navigation_opens_model_selector_without_starting_a_run() {
        let directory = tempfile::tempdir().unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.model_catalogs = Some(crate::test_utils::model_catalog_store().0);
        model
            .agent
            .set_provider_metadata("openai".to_string(), None);
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        focus_panel_input(&mut model);
        enter_insert_mode(&mut model);
        model.agent.prompt.set_value("/");

        handle_command(&mut model, CommandId("agent_command_next"));
        handle_command(&mut model, CommandId("agent_command_next"));
        assert_eq!(model.agent.prompt(), "/model");
        assert_eq!(
            model.agent.command_state().selected(),
            Some(crate::agent::PromptCommand::Model)
        );
        submit(&mut model);
        assert!(model.agent.session().run().is_none());
        assert!(model.modal.is_active(Context::ModelSelector));
        assert_eq!(input_context(&model), Some(Context::ModelSelector));
        assert_eq!(model.agent.prompt(), "");
        assert_eq!(model.agent.command_state().selected(), None);
        assert!(model.context_stack.is_empty());
        assert!(model.notifications.current().is_none());
    }

    #[test]
    fn slash_logout_opens_provider_logout_without_starting_a_run() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _storage_directory) = crate::test_utils::provider_credential_store();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.provider_credentials = Some(store);
        model.tools.active_tool = Some(ToolKind::Agent);
        model.tools.visibility = Visibility::Displayed;
        focus_panel_input(&mut model);
        enter_insert_mode(&mut model);
        model.agent.prompt.set_value("/logout");

        submit(&mut model);

        assert!(model.agent.session().run().is_none());
        assert!(model.modal.is_active(Context::ProviderLogout));
        assert_eq!(input_context(&model), Some(Context::ProviderLogout));
        assert_eq!(model.agent.prompt(), "");
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn inline_slash_command_resumes_the_agent_dialog() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        open(&mut model);
        assert!(model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(input_context(&model), Some(Context::AgentChatDialog));
        assert!(model.context_stack.is_empty());
        model.agent.prompt.set_value("/provider");
        handle_command(&mut model, CommandId("agent_command_quick_fill"));

        submit(&mut model);
        assert!(model.provider_login.is_open());
        assert_eq!(input_context(&model), Some(Context::ProviderLogin));
        assert_eq!(model.agent.prompt(), "");
        assert!(model.context_stack.is_empty());
        assert!(model.agent.session().run().is_none());

        crate::provider_login::handle_command(&mut model, CommandId("provider_login_close"));
        assert!(model.agent.dialog_open());
        assert_eq!(input_context(&model), Some(Context::AgentChatDialog));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn panel_discards_only_untouched_dialog_prefill() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &path);
        let context = Some(crate::agent::context::editor_context(&model).unwrap());

        model
            .agent
            .prepare_prompt(context.clone(), "main.rs:1".to_string());
        assert_eq!(model.agent.prompt(), "main.rs:1");
        model.agent.prepare_panel_prompt();
        assert_eq!(model.agent.prompt(), "");

        model.agent.prepare_prompt(context, "main.rs:1".to_string());
        model.agent.handle_input_event(TextInputEvent::Insert('?'));
        model.agent.prepare_panel_prompt();
        assert_eq!(model.agent.prompt(), "main.rs:1?");
    }

    #[test]
    fn failed_begin_preserves_the_prompt() {
        let mut model = Model::default();
        model
            .agent
            .session_mut()
            .begin("active".to_string())
            .unwrap();
        model.agent.prompt.set_value("next");

        assert!(model.agent.begin_prompt().is_none());
        assert_eq!(model.agent.prompt(), "next");
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
        handle_input(&mut model, TextInputEvent::Insert('?'));
        assert_eq!(model.agent.prompt(), "main.rs:2?");
        handle_input(&mut model, TextInputEvent::DeleteBackward);
        assert_eq!(model.agent.prompt(), "main.rs:2");
        close(&mut model);
        open(&mut model);
        assert_eq!(model.agent.prompt(), "main.rs:2");

        for _ in "main.rs:2".chars() {
            handle_input(&mut model, TextInputEvent::DeleteBackward);
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
        handle_input(&mut model, TextInputEvent::Insert('界'));
        assert_eq!(model.agent.prompt(), "main.rs界:1");
        handle_input(&mut model, TextInputEvent::DeleteBackward);
        assert_eq!(model.agent.prompt(), "main.rs:1");
        handle_input(&mut model, TextInputEvent::Insert('X'));
        assert_eq!(model.agent.prompt(), "main.rsX:1");
        handle_input(
            &mut model,
            TextInputEvent::Cursor(crate::component::text_input::TextCursorMovement::Right),
        );
        handle_input(&mut model, TextInputEvent::Insert('Y'));
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
            "Test response to: main.rs:1"
        );
        toggle_panel(&mut model);
        assert_eq!(model.tools.active_tool, Some(ToolKind::Agent));
        assert_eq!(
            model.current_focus,
            crate::data_object::FocusTarget::ToolsPanel
        );
        assert_eq!(input_context(&model), Some(Context::AgentChat));
        toggle_panel(&mut model);
        assert_eq!(model.current_focus, crate::data_object::FocusTarget::Editor);
        assert_ne!(input_context(&model), Some(Context::AgentChat));

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
            handle_input(&mut model, TextInputEvent::DeleteBackward);
        }
        for character in "fail".chars() {
            model
                .agent
                .handle_input_event(TextInputEvent::Insert(character));
        }
        let effect = submit(&mut model).unwrap();
        perform(&manager, &mut model, effect);

        assert_eq!(
            model.agent.session().run().unwrap().status,
            RunStatus::Failed("injected provider failure".to_string())
        );
        assert_eq!(model.agent.session().messages().len(), 1);
    }

    #[test]
    fn finalized_test_run_is_queryable_through_the_injected_store() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("main.rs");
        std::fs::write(&path, "fn main() {}\n").unwrap();
        let (store, _storage_directory) = crate::test_utils::session_store();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_session_store(store.clone());
        open_file(&mut model, &path);
        let manager = MessageManager::default();
        open(&mut model);
        let effect = submit(&mut model).unwrap();

        if let Some(storage) = perform(&manager, &mut model, effect) {
            perform(&manager, &mut model, storage);
        }

        tokio::runtime::Builder::new_current_thread()
            .enable_all()
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
                assert!(transcript.messages[1].text.contains("Test response"));
                assert_eq!(transcript.runs[0].provider, "test");
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

        if let Some(storage) = perform(&manager, &mut model, effect) {
            perform(&manager, &mut model, storage);
        }

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
        let (session_store, _storage_directory) = crate::test_utils::session_store();
        cancelled.agent.set_session_store(session_store);
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
