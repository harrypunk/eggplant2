use std::sync::Arc;

use eggplant_agent::{CancellationToken, SessionId, SessionRestoreError};
use eggplant_agent_storage::{RecentQuery, SessionRecord, SessionStore, StorageError};

use super::super::storage::{LoadedSession, load_session};
use crate::{
    agent::AgentUiAction,
    data_object::{Context, GeneralAction, Message, Model, StatusMessage},
    manager::effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
};

pub(crate) fn open(model: &mut Model) -> Option<Effect> {
    if model.agent.session().is_running() {
        return notify(
            model,
            "Finish or interrupt the active run before restoring a session",
        );
    }
    let Some(store) = model.agent.runtime().session_store() else {
        return notify(model, "Session storage is not configured");
    };
    let Some(workspace) = model
        .workspace
        .as_ref()
        .map(|workspace| workspace.root().to_string_lossy().into_owned())
    else {
        return notify(model, "Open a project to restore sessions");
    };
    let active_session = model.agent.session().id().clone();
    let return_context = super::super::dialog::suspend_for_overlay(model);
    if !model.modal.open(Context::AgentSessionSelector) {
        super::super::dialog::resume_overlay(model, return_context);
        return None;
    }
    let generation = model
        .agent
        .ui_mut()
        .session_selector_mut()
        .open(return_context, active_session);
    Some(Effect::feature(SessionEffect::Recent {
        store,
        workspace,
        generation,
    }))
}

pub(super) fn close(model: &mut Model) -> Option<Effect> {
    let return_context = model.agent.ui_mut().session_selector_mut().close();
    model.modal.close(Context::AgentSessionSelector);
    super::super::dialog::resume_overlay(model, return_context);
    None
}

pub(crate) fn handle_general_action(model: &mut Model, action: GeneralAction) -> Option<Effect> {
    match action {
        GeneralAction::Next => model.agent.ui_mut().session_selector_mut().select_next(),
        GeneralAction::Previous => model
            .agent
            .ui_mut()
            .session_selector_mut()
            .select_previous(),
        GeneralAction::Activate => return activate(model),
        GeneralAction::Back => return close(model),
        GeneralAction::FocusNext | GeneralAction::Paste => {}
    }
    None
}

fn activate(model: &mut Model) -> Option<Effect> {
    let store = model.agent.runtime().session_store()?;
    let (session_id, generation) = model
        .agent
        .ui_mut()
        .session_selector_mut()
        .begin_restore()?;
    Some(Effect::feature(SessionEffect::Restore {
        store,
        session_id,
        generation,
    }))
}

fn notify(model: &mut Model, message: &str) -> Option<Effect> {
    Some(crate::notifications::publish_status(
        model,
        StatusMessage::info(message.to_string()),
        Some("Agent"),
    ))
}

enum SessionEffect {
    Recent {
        store: Arc<dyn SessionStore>,
        workspace: String,
        generation: u64,
    },
    Restore {
        store: Arc<dyn SessionStore>,
        session_id: SessionId,
        generation: u64,
    },
}

impl SessionEffect {
    async fn perform_async(self) -> SessionCompletion {
        match self {
            Self::Recent {
                store,
                workspace,
                generation,
            } => {
                let result = store
                    .recent_sessions(RecentQuery {
                        workspace: Some(workspace),
                        ..RecentQuery::default()
                    })
                    .await
                    .map(|page| page.sessions);
                SessionCompletion::Recent { generation, result }
            }
            Self::Restore {
                store,
                session_id,
                generation,
            } => {
                let result =
                    load_session(store.as_ref(), session_id, CancellationToken::default()).await;
                SessionCompletion::Restore { generation, result }
            }
        }
    }
}

impl FeatureEffect for SessionEffect {
    fn perform(self: Box<Self>) -> Message {
        runtime().block_on(self.perform_async()).into_message()
    }

    fn execute(self: Box<Self>, sender: tokio::sync::mpsc::UnboundedSender<Message>) {
        tokio::spawn(async move {
            let _ = sender.send(self.perform_async().await.into_message());
        });
    }
}

enum SessionCompletion {
    Recent {
        generation: u64,
        result: Result<Vec<SessionRecord>, StorageError>,
    },
    Restore {
        generation: u64,
        result: Result<LoadedSession, StorageError>,
    },
}

impl SessionCompletion {
    fn into_message(self) -> Message {
        Message::FeatureCompletion(FeatureCompletion::new(self))
    }
}

impl Completion for SessionCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        match *self {
            SessionCompletion::Recent { generation, result } => {
                model
                    .agent
                    .ui_mut()
                    .session_selector_mut()
                    .complete_recent(generation, result.map_err(bounded_error));
                None
            }
            SessionCompletion::Restore { generation, result } => {
                complete_restore(model, generation, result)
            }
        }
    }
}

fn complete_restore(
    model: &mut Model,
    generation: u64,
    result: Result<LoadedSession, StorageError>,
) -> Option<Effect> {
    if !model
        .agent
        .ui()
        .session_selector()
        .is_current_restoring(generation)
    {
        return None;
    }
    match result {
        Err(error) => {
            model
                .agent
                .ui_mut()
                .session_selector_mut()
                .fail_restore(generation, bounded_error(error));
            None
        }
        Ok(LoadedSession {
            snapshot,
            created_at,
        }) => match model
            .agent
            .runtime_mut()
            .restore_session(snapshot, created_at)
        {
            Ok(_) => {
                model.agent.dispatch(AgentUiAction::SessionChanged);
                close(model)
            }
            Err(SessionRestoreError::RunInProgress) => {
                model.agent.ui_mut().session_selector_mut().fail_restore(
                    generation,
                    "Finish or interrupt the active run before restoring a session".to_string(),
                );
                None
            }
        },
    }
}

fn bounded_error(error: StorageError) -> String {
    error.to_string().chars().take(512).collect()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("session selector runtime must build")
}

#[cfg(test)]
mod tests {
    use eggplant_agent::{MessageRole, RunId, RunStatus};
    use eggplant_agent_storage::{FinalizedRun, MessageRecord, RunRecord, SessionRecord};
    use eggplant_agent_storage_seaorm::SeaOrmSessionStore;

    use super::*;
    use crate::manager::message_manager::MessageManager;

    fn perform(manager: &MessageManager, model: &mut Model, effect: Effect) {
        let Effect::Feature(request) = effect else {
            panic!("expected session effect");
        };
        manager.update(model, request.perform());
    }

    async fn persist(
        store: &SeaOrmSessionStore,
        session_id: SessionId,
        workspace: String,
        title: &str,
    ) {
        let run = RunRecord {
            id: RunId(1),
            session_id: session_id.clone(),
            status: RunStatus::Complete,
            created_at: 10,
            completed_at: Some(20),
            provider: "test".to_string(),
            model: Some("fake".to_string()),
            editor_context: None,
        };
        let messages = [
            (MessageRole::User, "stored question"),
            (MessageRole::Assistant, "stored answer"),
        ]
        .into_iter()
        .enumerate()
        .map(|(sequence, (role, text))| MessageRecord {
            session_id: session_id.clone(),
            run_id: RunId(1),
            sequence: sequence as u64,
            role,
            text: text.to_string(),
            created_at: 20,
        })
        .collect();
        store
            .finalize_run(
                FinalizedRun {
                    session: SessionRecord {
                        id: session_id,
                        created_at: 10,
                        updated_at: 20,
                        workspace: Some(workspace),
                        title: Some(title.to_string()),
                    },
                    run,
                    messages,
                    tool_calls: Vec::new(),
                },
                CancellationToken::default(),
            )
            .await
            .unwrap();
    }

    fn model_with_sessions() -> (Model, Arc<SeaOrmSessionStore>, tempfile::TempDir, SessionId) {
        let (store, directory) = crate::test_utils::session_store();
        let mut model =
            crate::startup::initial_model(Some(directory.path().to_path_buf())).unwrap();
        model.agent.set_session_store(store.clone());
        let workspace = model
            .workspace
            .as_ref()
            .unwrap()
            .root()
            .to_string_lossy()
            .into_owned();
        let stored = SessionId::from("stored-session");
        runtime().block_on(async {
            persist(store.as_ref(), stored.clone(), workspace, "Stored session").await;
            persist(
                store.as_ref(),
                SessionId::from("other-workspace"),
                "/other/workspace".to_string(),
                "Other workspace",
            )
            .await;
        });
        (model, store, directory, stored)
    }

    #[test]
    fn selecting_a_saved_session_restores_it_and_closes_the_modal() {
        let (mut model, _store, _directory, stored) = model_with_sessions();
        let manager = MessageManager::default();
        let effect = open(&mut model).unwrap();
        perform(&manager, &mut model, effect);
        assert_eq!(model.agent.session_selector().sessions().len(), 1);
        assert_eq!(model.agent.session_selector().sessions()[0].id, stored);

        let effect = activate(&mut model).unwrap();
        perform(&manager, &mut model, effect);

        assert_eq!(model.agent.session().id(), &stored);
        assert_eq!(model.agent.session().messages().len(), 2);
        assert!(!model.modal.is_active(Context::AgentSessionSelector));
        let next = model
            .agent
            .runtime_mut()
            .begin("next question".to_string(), None, 30)
            .unwrap();
        assert_eq!(next.id, RunId(2));
    }

    #[test]
    fn closing_selector_restores_the_suspended_agent_dialog() {
        let (mut model, _store, _directory, _stored) = model_with_sessions();
        crate::agent::dialog::open(&mut model);
        let _effect = open(&mut model);
        assert!(!model.modal.is_active(Context::AgentChatDialog));
        assert!(model.modal.is_active(Context::AgentSessionSelector));
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Suspended
        );

        close(&mut model);

        assert!(!model.modal.is_active(Context::AgentSessionSelector));
        assert!(model.modal.is_active(Context::AgentChatDialog));
        assert_eq!(
            model.agent.dialog_state(),
            crate::agent::AgentDialogState::Open
        );
    }

    #[test]
    fn stale_restore_completion_preserves_the_current_session() {
        let (mut model, _store, _directory, stored) = model_with_sessions();
        let original = model.agent.session().id().clone();
        let manager = MessageManager::default();
        let effect = open(&mut model).unwrap();
        perform(&manager, &mut model, effect);
        let effect = activate(&mut model).unwrap();
        close(&mut model);

        perform(&manager, &mut model, effect);

        assert_eq!(model.agent.session().id(), &original);
        assert_ne!(model.agent.session().id(), &stored);
        assert!(model.agent.session().messages().is_empty());
    }
}
