use eggplant_agent_storage::{ProviderAuthType, ProviderCredential, StorageError};

use crate::{
    component::text_input::TextInputEvent,
    data_object::{CommandId, Context, GeneralAction, Model, StatusMessage},
    manager::{
        command_registry::Command, effect::Effect, general_action::GeneralActionSubscription,
        input_context::input_context,
    },
};

pub fn commands() -> Vec<Command> {
    [
        Command::feature(
            CommandId("provider_login_open"),
            "open provider login",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_next"),
            "select next provider login option",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_previous"),
            "select previous provider login option",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_activate"),
            "activate provider login option",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_focus_next"),
            "focus next provider login field",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_paste"),
            "paste into the focused provider login field",
            handle_command,
        ),
        Command::feature(
            CommandId("provider_login_close"),
            "go back or close provider login",
            handle_command,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    if id.as_str() == "provider_login_open" {
        return open(model);
    }
    if !available(model) || model.provider_login.is_saving() {
        return None;
    }
    match id.as_str() {
        "provider_login_next" => model.provider_login.select_next(),
        "provider_login_previous" => model.provider_login.select_previous(),
        "provider_login_activate" => return activate(model),
        "provider_login_focus_next" => model.provider_login.focus_next(),
        "provider_login_paste" => return super::paste::paste(model),
        "provider_login_close" => return back(model),
        _ => {}
    }
    None
}

pub fn general_action_subscriptions() -> Vec<GeneralActionSubscription> {
    vec![GeneralActionSubscription::new(
        Context::ProviderLogin,
        handle_general_action,
    )]
}

pub fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => model.provider_login.select_next(),
        GeneralAction::Previous => model.provider_login.select_previous(),
        GeneralAction::Activate => return activate(model),
        GeneralAction::Back => return back(model),
    }
    None
}

fn back(model: &mut Model) -> Option<Effect> {
    match model.provider_login.back() {
        super::ProviderLoginBack::Back => None,
        super::ProviderLoginBack::Close => close(model),
    }
}

pub fn handle_input(model: &mut Model, event: TextInputEvent) -> Option<Effect> {
    if available(model) {
        model.provider_login.handle_input_event(event);
    }
    None
}

fn available(model: &Model) -> bool {
    input_context(model) == Some(Context::ProviderLogin) && model.provider_login.is_open()
}

fn activate(model: &mut Model) -> Option<Effect> {
    match model.provider_login.page() {
        super::ProviderLoginPage::OpenAiModelSelection(_) => {
            return super::openai_work::activate_selected(model);
        }
        super::ProviderLoginPage::OpenAiModelsLoading(_)
        | super::ProviderLoginPage::OpenAiActivating(_) => return None,
        _ => {}
    }
    let Some(submission) = model.provider_login.submission() else {
        model.provider_login.activate();
        return None;
    };
    let (provider, base_url, key) = match submission {
        super::ProviderLoginSubmission::OpenAi {
            provider,
            endpoint,
            api_key,
        } => {
            let endpoint = endpoint.trim().to_string();
            if endpoint.is_empty() {
                model.provider_login.set_error("Endpoint must not be empty");
                return None;
            }
            (provider, Some(endpoint), api_key)
        }
        super::ProviderLoginSubmission::ApiKey { provider, api_key } => (provider, None, api_key),
    };
    save_api_key(model, provider, base_url, key)
}

fn save_api_key(
    model: &mut Model,
    provider: super::ProviderSpec,
    base_url: Option<String>,
    key: String,
) -> Option<Effect> {
    let key = key.trim().to_string();
    if key.is_empty() {
        model.provider_login.set_error("API key must not be empty");
        return None;
    }
    let Some(store) = model.provider_credentials.clone() else {
        model
            .provider_login
            .set_error("Durable login is unavailable until provider storage is configured");
        return None;
    };
    if !model.provider_login.begin_save(provider) {
        return None;
    }
    let now = chrono::Utc::now().timestamp_millis();
    Some(super::storage::persist(
        store,
        ProviderCredential {
            provider_id: provider.id().to_string(),
            auth_type: ProviderAuthType::ApiKey,
            base_url,
            selected_model: None,
            api_key: Some(key),
            access_token: None,
            refresh_token: None,
            expires_at: None,
            account_id: None,
            created_at: now,
            updated_at: now,
        },
    ))
}

pub(super) fn complete_save(
    model: &mut Model,
    provider_id: &str,
    result: Result<(), StorageError>,
) -> Option<Effect> {
    let error = result
        .err()
        .map(|error| error.to_string().chars().take(512).collect::<String>());
    match model
        .provider_login
        .complete_save(provider_id, error.clone())
    {
        Some(true) => complete_save_success(model, provider_id),
        Some(false) => Some(crate::notifications::publish_status(
            model,
            StatusMessage::error(format!(
                "Failed to save API key for {provider_id}: {}",
                error.unwrap_or_else(|| "unknown storage error".to_string())
            )),
            Some("Provider"),
        )),
        None => None,
    }
}

fn complete_save_success(model: &mut Model, provider_id: &str) -> Option<Effect> {
    match provider_id {
        "openai" => match super::openai_work::after_save(model) {
            super::openai_work::OpenAiAfterSave::FetchModels(effect) => Some(effect),
            super::openai_work::OpenAiAfterSave::CloseWithoutCatalog => {
                close_with_saved_notice(model, provider_id)
            }
        },
        _ => close_with_saved_notice(model, provider_id),
    }
}

fn close_with_saved_notice(model: &mut Model, provider_id: &str) -> Option<Effect> {
    close(model);
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(format!("Saved API key for {provider_id}")),
        Some("Provider"),
    ))
}

pub fn open(model: &mut Model) -> Option<Effect> {
    if model.provider_login.is_open() {
        return None;
    }
    let return_context = suspend_inline_agent(model);
    if !model.modal.open(Context::ProviderLogin) {
        resume_inline_agent(model, return_context);
        return None;
    }
    model.provider_login.open(return_context);
    model.context_stack.push(Context::ProviderLogin);
    None
}

fn suspend_inline_agent(model: &mut Model) -> Option<Context> {
    if input_context(model) != Some(Context::AgentChatDialog)
        || !model.agent.dialog_open()
        || !model.modal.is_active(Context::AgentChatDialog)
    {
        return None;
    }
    model.agent.suspend_dialog();
    model.modal.close(Context::AgentChatDialog);
    model
        .context_stack
        .pop_if(|context| context == &Context::AgentChatDialog);
    Some(Context::AgentChatDialog)
}

pub(super) fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.provider_login.close();
    model.modal.close(Context::ProviderLogin);
    model
        .context_stack
        .pop_if(|context| context == &Context::ProviderLogin);
    resume_inline_agent(model, return_context);
    None
}

fn resume_inline_agent(model: &mut Model, context: Option<Context>) {
    if context != Some(Context::AgentChatDialog)
        || !model.agent.dialog_open()
        || !model.modal.open(Context::AgentChatDialog)
    {
        return;
    }
    model.agent.resume_dialog();
    model.context_stack.push(Context::AgentChatDialog);
}

#[cfg(test)]
mod tests {
    use eggplant_agent_storage::ProviderCredentialStore;

    use super::{super::paste, *};
    use crate::{
        capabilities::clipboard::ClipboardReader, manager::message_manager::MessageManager,
    };

    struct TestClipboard(Result<String, String>);

    impl ClipboardReader for TestClipboard {
        fn read_text(&self) -> Result<String, String> {
            self.0.clone()
        }
    }

    fn open_api_key_at(model: &mut Model, index: usize) {
        open(model);
        handle_command(model, CommandId("provider_login_next"));
        handle_command(model, CommandId("provider_login_activate"));
        for _ in 0..index {
            handle_command(model, CommandId("provider_login_next"));
        }
        handle_command(model, CommandId("provider_login_activate"));
    }

    fn insert_key(model: &mut Model, key: &str) {
        if let crate::provider_login::ProviderLoginPage::OpenAiInput(_) =
            model.provider_login.page()
        {
            handle_command(model, CommandId("provider_login_focus_next"));
        }
        key.chars().for_each(|character| {
            handle_input(model, TextInputEvent::Insert(character));
        });
    }

    fn complete_effect(model: &mut Model, effect: Effect) {
        let Effect::Feature(effect) = effect else {
            panic!("provider login should use a feature effect");
        };
        let message = effect.perform();
        MessageManager::default().update(model, message);
    }

    fn submit_and_complete(model: &mut Model) {
        let effect = handle_command(model, CommandId("provider_login_activate"))
            .expect("API-key submission should create a save effect");
        complete_effect(model, effect);
    }

    #[test]
    fn dialog_owns_modal_input_until_closed() {
        let mut model = Model::default();

        open(&mut model);
        assert!(model.provider_login.is_open());
        assert!(model.modal.is_active(Context::ProviderLogin));
        assert_eq!(model.context_stack.last(), Some(&Context::ProviderLogin));

        close(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(!model.modal.is_active(Context::ProviderLogin));
        assert!(model.context_stack.is_empty());
    }

    #[test]
    fn commands_navigate_submenus_and_escape_steps_back() {
        use crate::provider_login::{ProviderAuthMethod, ProviderLoginPage};

        let mut model = Model::default();
        open(&mut model);
        handle_command(&mut model, CommandId("provider_login_activate"));
        assert_eq!(
            model.provider_login.page(),
            ProviderLoginPage::Providers(ProviderAuthMethod::Account)
        );

        handle_command(&mut model, CommandId("provider_login_close"));
        assert_eq!(
            model.provider_login.page(),
            ProviderLoginPage::AuthenticationMethod
        );
        handle_command(&mut model, CommandId("provider_login_next"));
        handle_command(&mut model, CommandId("provider_login_activate"));
        handle_command(&mut model, CommandId("provider_login_next"));
        handle_command(&mut model, CommandId("provider_login_activate"));
        let ProviderLoginPage::ApiKeyInput(provider) = model.provider_login.page() else {
            panic!("provider selection should open its API-key input");
        };
        assert_eq!(provider.id(), "kimi-for-coding");

        handle_command(&mut model, CommandId("provider_login_close"));
        assert_eq!(
            model.provider_login.page(),
            ProviderLoginPage::Providers(ProviderAuthMethod::ApiKey)
        );
        handle_command(&mut model, CommandId("provider_login_close"));
        handle_command(&mut model, CommandId("provider_login_close"));
        assert!(!model.provider_login.is_open());
        assert!(!model.modal.is_active(Context::ProviderLogin));
    }

    #[test]
    fn saves_multiple_providers_without_selecting_a_runtime_provider() {
        let (store, _directory) = crate::test_utils::provider_credential_store();
        let mut model = Model {
            provider_credentials: Some(store.clone()),
            ..Model::default()
        };

        open_api_key_at(&mut model, 0);
        insert_key(&mut model, "openai-secret");
        submit_and_complete(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.agent.provider().is_none());

        open_api_key_at(&mut model, 3);
        insert_key(&mut model, "deepseek-secret");
        submit_and_complete(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.agent.provider().is_none());

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let openai = runtime.block_on(store.get("openai")).unwrap().unwrap();
        let deepseek = runtime.block_on(store.get("deepseek")).unwrap().unwrap();
        assert_eq!(openai.api_key.as_deref(), Some("openai-secret"));
        assert_eq!(
            openai.base_url.as_deref(),
            Some("https://api.openai.com/v1")
        );
        assert_eq!(deepseek.api_key.as_deref(), Some("deepseek-secret"));
        assert_eq!(deepseek.base_url, None);
        assert_eq!(runtime.block_on(store.list()).unwrap().len(), 2);
    }

    #[test]
    fn clipboard_paste_is_revision_checked_across_fields() {
        let mut model = Model::default();
        open_api_key_at(&mut model, 0);
        for _ in 0..crate::provider_login::DEFAULT_OPENAI_ENDPOINT.len() {
            handle_input(&mut model, TextInputEvent::DeleteBackward);
        }
        let effect = paste::paste_with_reader(
            &mut model,
            TestClipboard(Ok("https://localhost:1234/v1\n".to_string())),
        )
        .unwrap();
        complete_effect(&mut model, effect);
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::Endpoint)
                .value(),
            "https://localhost:1234/v1"
        );

        handle_command(&mut model, CommandId("provider_login_focus_next"));
        let effect =
            paste::paste_with_reader(&mut model, TestClipboard(Ok("key-secret".to_string())))
                .unwrap();
        complete_effect(&mut model, effect);
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::ApiKey)
                .value(),
            "key-secret"
        );

        let stale =
            paste::paste_with_reader(&mut model, TestClipboard(Ok("must-not-paste".to_string())))
                .unwrap();
        handle_command(&mut model, CommandId("provider_login_focus_next"));
        complete_effect(&mut model, stale);
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::ApiKey)
                .value(),
            "key-secret"
        );
    }

    #[test]
    fn invalid_or_unpersistable_keys_stay_in_the_masked_draft() {
        let mut model = Model::default();
        open_api_key_at(&mut model, 0);
        handle_command(&mut model, CommandId("provider_login_activate"));
        assert_eq!(
            model.provider_login.error(),
            Some("API key must not be empty")
        );

        insert_key(&mut model, "secret");
        assert!(handle_command(&mut model, CommandId("provider_login_activate")).is_none());
        assert!(
            model
                .provider_login
                .error()
                .unwrap()
                .contains("storage is configured")
        );
        assert_eq!(
            model
                .provider_login
                .openai_input(crate::provider_login::OpenAiField::ApiKey)
                .value(),
            "secret"
        );
        assert!(model.provider_login.is_open());
    }

    #[test]
    fn inline_agent_dialog_resumes_after_provider_login() {
        let mut model = Model::default();
        let buffer = crate::test_utils::create_buffer(&mut model, "fn main() {}\n");
        model.agent.open_dialog(
            Some(crate::agent::EditorContext {
                buffer,
                path: std::path::PathBuf::from("/workspace/main.rs"),
                revision: 1,
                line: 1,
            }),
            "main.rs:1".to_string(),
        );
        assert!(model.modal.open(Context::AgentChatDialog));
        model.context_stack.push(Context::AgentChatDialog);

        open(&mut model);
        assert!(model.provider_login.is_open());
        assert!(model.modal.is_active(Context::ProviderLogin));
        assert!(!model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(model.context_stack.last(), Some(&Context::ProviderLogin));
        assert!(model.agent.dialog_open());
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Suspended
        );

        close(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.agent.dialog_open());
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Open
        );
        assert!(model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(model.context_stack.last(), Some(&Context::AgentChatDialog));
    }

    #[test]
    fn another_modal_prevents_opening() {
        let mut model = Model::default();
        assert!(model.modal.open(Context::FilePicker));
        open(&mut model);
        assert!(!model.provider_login.is_open());
        assert!(model.modal.is_active(Context::FilePicker));
    }
}
