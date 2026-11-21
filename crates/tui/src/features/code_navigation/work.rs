use std::path::PathBuf;

use app::language::{
    DocumentEvent, LanguageError, LanguageEvent, LanguageMessage, LanguageMessageKind,
    LanguageProgress, LanguageProgressStage, LanguageServerSpec, LanguageService, NavigationKind,
    NavigationRequest,
};

use super::NavigationOutcome;
use crate::{
    capabilities::notification::Notification,
    data_object::{CommandId, Context, Model, StatusMessage},
    diagnostics::{DiagnosticFailureCompletion, DiagnosticsCompletion},
    document,
    manager::{
        command_registry::{Command, CommandRegistry},
        effect::{Completion, Effect, FeatureCompletion, FeatureEffect},
    },
    notifications::NotificationCompletion,
};

pub fn register_commands(commands: &mut CommandRegistry) -> color_eyre::Result<()> {
    [
        Command::feature(
            CommandId("code_definition"),
            "go to definition",
            handle_command,
        ),
        Command::feature(
            CommandId("code_references"),
            "find references",
            handle_command,
        ),
        Command::feature(
            CommandId("code_implementation"),
            "go to implementation",
            handle_command,
        ),
        Command::feature(
            CommandId("code_navigation_next"),
            "next location",
            handle_command,
        ),
        Command::feature(
            CommandId("code_navigation_previous"),
            "previous location",
            handle_command,
        ),
        Command::feature(
            CommandId("code_navigation_open"),
            "open location",
            handle_command,
        ),
        Command::feature(
            CommandId("close_code_navigation"),
            "close locations",
            handle_command,
        ),
    ]
    .into_iter()
    .try_for_each(|command| commands.register(command))
}

pub fn handle_command(model: &mut Model, id: CommandId) -> Option<Effect> {
    match id.as_str() {
        "code_definition" => start(model, NavigationKind::Definition),
        "code_references" => start(model, NavigationKind::References),
        "code_implementation" => start(model, NavigationKind::Implementation),
        "code_navigation_next" => {
            model.code_navigation.select_next();
            None
        }
        "code_navigation_previous" => {
            model.code_navigation.select_previous();
            None
        }
        "code_navigation_open" => open_selected(model),
        "close_code_navigation" => {
            close(model);
            None
        }
        _ => notify(
            model,
            StatusMessage::error(format!("unknown code-navigation command `{}`", id.as_str())),
        ),
    }
}

fn start(model: &mut Model, kind: NavigationKind) -> Option<Effect> {
    if model.code_navigation.is_busy() {
        return None;
    }
    let document = active_document(model)?;
    let position = model.editor.active_position()?;
    let request = model.code_navigation.begin(
        kind,
        document.buffer,
        document.path.clone(),
        document.revision,
        position,
    );
    language_work(model, document.event(), LanguageAction::Navigate(request))
}

/// Synchronize the active document, or retain only the newest event while busy.
pub fn synchronize(model: &mut Model) -> Option<Effect> {
    let event = model
        .code_navigation
        .take_trailing_sync()
        .or_else(|| active_document(model).map(|document| document.event()))?;
    if model.code_navigation.is_busy() {
        model.code_navigation.queue_sync(event);
        return None;
    }
    model.code_navigation.begin_sync();
    language_work(model, event, LanguageAction::Synchronize)
}

/// Close one document without starting a server when none is running.
pub fn close_document(model: &mut Model, path: PathBuf) -> Option<Effect> {
    model.diagnostics.clear_path(&path);
    if !model.code_navigation.has_service() {
        return None;
    }
    if model.code_navigation.is_busy() {
        model
            .code_navigation
            .queue_sync(DocumentEvent::Close { path });
        return None;
    }
    model.code_navigation.begin_sync();
    language_work(
        model,
        DocumentEvent::Close { path },
        LanguageAction::Synchronize,
    )
}

struct ActiveDocument {
    buffer: editor::BufferId,
    path: PathBuf,
    revision: i32,
    text: String,
    spec: LanguageServerSpec,
}

impl ActiveDocument {
    fn event(&self) -> DocumentEvent {
        DocumentEvent::Open {
            path: self.path.clone(),
            language: self.spec.language().to_string(),
            version: self.revision,
            text: self.text.clone(),
        }
    }
}

fn active_document(model: &Model) -> Option<ActiveDocument> {
    let buffer = model.editor.active_buffer()?;
    let path = model.editor.buffer_path(buffer)?.to_path_buf();
    let (_, revision) = model.editor.active_revision()?;
    let snapshot = model.editor.active_text_snapshot()?;
    let spec = model.language_servers.find_for_path(&path)?;

    Some(ActiveDocument {
        buffer,
        path,
        revision,
        text: snapshot.text,
        spec,
    })
}

fn language_work(
    model: &mut Model,
    document: DocumentEvent,
    action: LanguageAction,
) -> Option<Effect> {
    let root = model.workspace.as_ref()?.root().to_path_buf();
    let spec = model
        .language_servers
        .find_for_path(document_path(&document))?;
    let service = model.code_navigation.take_service();
    let starting = service.is_none();
    let restarting = starting && model.diagnostics.server_generation() > 0;
    if starting {
        model.diagnostics.restart_server();
    }
    let generation = model.diagnostics.server_generation();
    let notification = starting.then(|| {
        let action = if restarting { "Restarting" } else { "Starting" };
        crate::notifications::publish_status(
            model,
            StatusMessage::info(format!("{action} language server `{}`", spec.command())),
            Some("LSP"),
        )
    });
    let work = Some(Effect::feature(LanguageWork {
        service,
        spec,
        root,
        document,
        action,
        generation,
        diagnostics_sender: None,
    }));
    Effect::combine(notification, work)
}

fn document_path(document: &DocumentEvent) -> &std::path::Path {
    match document {
        DocumentEvent::Open { path, .. }
        | DocumentEvent::Change { path, .. }
        | DocumentEvent::Close { path } => path,
    }
}

enum LanguageAction {
    Navigate(NavigationRequest),
    Synchronize,
}

struct LanguageWork {
    service: Option<Box<dyn LanguageService>>,
    spec: LanguageServerSpec,
    root: std::path::PathBuf,
    document: DocumentEvent,
    action: LanguageAction,
    generation: u64,
    diagnostics_sender: Option<tokio::sync::mpsc::UnboundedSender<crate::data_object::Message>>,
}

impl FeatureEffect for LanguageWork {
    fn perform(self: Box<Self>) -> crate::data_object::Message {
        let Self {
            service,
            spec,
            root,
            document,
            action,
            generation,
            diagnostics_sender,
        } = *self;
        let mut service = match service {
            Some(service) => service,
            None => match app::lsp::process::LspProcess::spawn(&spec, &root) {
                Ok(process) => {
                    if let Some(sender) = &diagnostics_sender {
                        let _ = sender.send(crate::data_object::Message::FeatureCompletion(
                            FeatureCompletion::new(NotificationCompletion(
                                Notification::info(format!(
                                    "Language server `{}` started",
                                    spec.command()
                                ))
                                .with_title("LSP"),
                            )),
                        ));
                    }
                    let sender = diagnostics_sender;
                    Box::new(app::lsp::runtime::LspRuntime::start(
                        process,
                        move |result| {
                            let Some(sender) = &sender else {
                                return;
                            };
                            let message = match result {
                                Ok(LanguageEvent::Diagnostics(batch)) => {
                                    crate::data_object::Message::FeatureCompletion(
                                        FeatureCompletion::new(DiagnosticsCompletion {
                                            batch,
                                            server_generation: generation,
                                        }),
                                    )
                                }
                                Ok(LanguageEvent::Message(message)) => {
                                    crate::data_object::Message::FeatureCompletion(
                                        FeatureCompletion::new(NotificationCompletion(
                                            server_notification(message),
                                        )),
                                    )
                                }
                                Ok(LanguageEvent::Progress(progress)) => {
                                    crate::data_object::Message::FeatureCompletion(
                                        FeatureCompletion::new(NotificationCompletion(
                                            progress_notification(progress),
                                        )),
                                    )
                                }
                                Ok(LanguageEvent::Failure(error)) => {
                                    crate::data_object::Message::FeatureCompletion(
                                        FeatureCompletion::new(DiagnosticFailureCompletion {
                                            message: format!("Language server failed: {}", error.0),
                                            server_generation: generation,
                                            fatal: true,
                                        }),
                                    )
                                }
                                Err(error) => crate::data_object::Message::FeatureCompletion(
                                    FeatureCompletion::new(DiagnosticFailureCompletion {
                                        message: format!("Language server error: {}", error.0),
                                        server_generation: generation,
                                        fatal: false,
                                    }),
                                ),
                            };
                            let _ = sender.send(message);
                        },
                    )) as Box<dyn LanguageService>
                }
                Err(error) => {
                    return completion(
                        action,
                        None,
                        Err(LanguageError(format!(
                            "Failed to start language server `{}`: {}",
                            spec.command(),
                            error.0
                        ))),
                    );
                }
            },
        };
        let result = service.synchronize(document).and_then(|()| match &action {
            LanguageAction::Navigate(request) => service.navigate(request.clone()),
            LanguageAction::Synchronize => Ok(Vec::new()),
        });
        completion(action, Some(service), result)
    }

    fn execute(
        mut self: Box<Self>,
        sender: tokio::sync::mpsc::UnboundedSender<crate::data_object::Message>,
    ) {
        self.diagnostics_sender = Some(sender.clone());
        tokio::task::spawn_blocking(move || {
            let message = self.perform();
            let _ = sender.send(message);
        });
    }
}

fn completion(
    action: LanguageAction,
    service: Option<Box<dyn LanguageService>>,
    result: Result<Vec<app::language::SourceLocation>, LanguageError>,
) -> crate::data_object::Message {
    crate::data_object::Message::FeatureCompletion(FeatureCompletion::new(LanguageCompletion {
        action,
        service,
        result,
    }))
}

struct LanguageCompletion {
    action: LanguageAction,
    service: Option<Box<dyn LanguageService>>,
    result: Result<Vec<app::language::SourceLocation>, LanguageError>,
}

impl Completion for LanguageCompletion {
    fn update(self: Box<Self>, model: &mut Model) -> Option<Effect> {
        if let Some(service) = self.service {
            model.code_navigation.restore_service(service);
        }
        let effect = match self.action {
            LanguageAction::Navigate(request) => navigate_completion(model, request, self.result),
            LanguageAction::Synchronize => {
                model.code_navigation.finish_operation();
                match self.result {
                    Ok(_) => None,
                    Err(error) => notify_lsp(model, StatusMessage::error(error.0)),
                }
            }
        };
        let trailing = model
            .code_navigation
            .has_trailing_sync()
            .then(|| synchronize(model))
            .flatten();
        Effect::combine(effect, trailing)
    }
}

fn navigate_completion(
    model: &mut Model,
    request: NavigationRequest,
    result: Result<Vec<app::language::SourceLocation>, LanguageError>,
) -> Option<Effect> {
    let (buffer, revision) = model.editor.active_revision()?;
    match model
        .code_navigation
        .complete(request.id, buffer, revision, result)
    {
        NavigationOutcome::Ignored => None,
        NavigationOutcome::Failed(error) => notify_lsp(model, StatusMessage::error(error.0)),
        NavigationOutcome::NoResults => notify(
            model,
            StatusMessage::info(format!("No {} found", navigation_label(request.kind))),
        ),
        NavigationOutcome::Locations => {
            if model.code_navigation.locations().len() == 1 {
                let location = model.code_navigation.locations()[0].clone();
                open_location(model, location)
            } else {
                if model.modal.open(Context::CodeNavigation)
                    && model.context_stack.last() != Some(&Context::CodeNavigation)
                {
                    model.context_stack.push(Context::CodeNavigation);
                }
                None
            }
        }
    }
}

fn navigation_label(kind: NavigationKind) -> &'static str {
    match kind {
        NavigationKind::Definition => "definition",
        NavigationKind::References => "references",
        NavigationKind::Implementation => "implementation",
    }
}

fn open_selected(model: &mut Model) -> Option<Effect> {
    let location = model.code_navigation.selected_location()?.clone();
    open_location(model, location)
}

fn open_location(model: &mut Model, location: app::language::SourceLocation) -> Option<Effect> {
    let documents = model.explorer.documents().clone();
    close(model);
    document::open_file_at(&documents, model, &location.path, Some(location.start))
}

fn close(model: &mut Model) {
    model.code_navigation.clear_locations();
    model.modal.close(Context::CodeNavigation);
    model
        .context_stack
        .pop_if(|context| context == &Context::CodeNavigation);
}

fn server_notification(message: LanguageMessage) -> Notification {
    let notification = match message.kind {
        LanguageMessageKind::Error => Notification::error(message.message),
        LanguageMessageKind::Warning => Notification::warning(message.message),
        LanguageMessageKind::Information | LanguageMessageKind::Log => {
            Notification::info(message.message)
        }
    };
    notification.with_title("LSP")
}

fn progress_notification(progress: LanguageProgress) -> Notification {
    let message = match progress.stage {
        LanguageProgressStage::Begin | LanguageProgressStage::Report => {
            match (progress.message, progress.percentage) {
                (Some(message), Some(percentage)) => {
                    format!("{}: {message} ({percentage}%)", progress.title)
                }
                (Some(message), None) => format!("{}: {message}", progress.title),
                (None, Some(percentage)) => format!("{} ({percentage}%)", progress.title),
                (None, None) => progress.title,
            }
        }
        LanguageProgressStage::End => match progress.message {
            Some(message) => format!("{}: {message}", progress.title),
            None => format!("{} finished", progress.title),
        },
    };
    Notification::info(message).with_title("LSP")
}

fn notify(model: &mut Model, status: StatusMessage) -> Option<Effect> {
    Some(crate::notifications::report_status(model, status))
}

fn notify_lsp(model: &mut Model, status: StatusMessage) -> Option<Effect> {
    Some(crate::notifications::report_status_titled(
        model,
        status,
        Some("LSP"),
    ))
}

#[cfg(test)]
mod tests {
    use app::language::SourceLocation;
    use editor::DocumentPosition;

    use super::*;
    use crate::{
        data_object::Message, manager::message_manager::MessageManager, startup::initial_model,
        test_utils::open_file,
    };

    struct FakeService {
        locations: Vec<SourceLocation>,
    }

    impl LanguageService for FakeService {
        fn synchronize(&mut self, _: DocumentEvent) -> Result<(), LanguageError> {
            Ok(())
        }

        fn navigate(&mut self, _: NavigationRequest) -> Result<Vec<SourceLocation>, LanguageError> {
            Ok(self.locations.clone())
        }
    }

    fn location(path: &std::path::Path) -> SourceLocation {
        SourceLocation {
            path: path.to_path_buf(),
            start: DocumentPosition {
                line: 0,
                character: 0,
            },
            end: DocumentPosition {
                line: 0,
                character: 1,
            },
        }
    }

    fn model_with_locations(
        root: &std::path::Path,
        locations: Vec<SourceLocation>,
    ) -> (Model, MessageManager) {
        let mut model = initial_model(Some(root.to_path_buf())).unwrap();
        open_file(&mut model, &root.join("main.rs"));
        model
            .code_navigation
            .restore_service(Box::new(FakeService { locations }));
        (model, MessageManager::default())
    }

    fn run_feature(manager: &MessageManager, model: &mut Model, effect: Effect) -> Option<Effect> {
        let Effect::Feature(request) = effect else {
            panic!("expected navigation effect");
        };
        manager.update(model, request.perform())
    }

    #[test]
    fn one_result_opens_the_target_through_the_document_capability() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        std::fs::write(directory.path().join("target.rs"), "fn target() {}\n").unwrap();
        let target = directory.path().join("target.rs");
        let (mut model, manager) = model_with_locations(directory.path(), vec![location(&target)]);

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("code_definition")),
            )
            .unwrap();
        let open = run_feature(&manager, &mut model, effect);
        let Some(Effect::OpenDocument(request)) = open else {
            panic!("expected document-open effect");
        };
        manager.update(&mut model, Message::FileOpened(request.perform()));
        let active = model.editor.active_buffer().unwrap();
        assert_eq!(model.editor.buffer_path(active), Some(target.as_path()));
    }

    #[test]
    fn server_messages_preserve_warning_severity() {
        let notification = server_notification(LanguageMessage {
            kind: LanguageMessageKind::Warning,
            message: "workspace loaded".to_string(),
        });

        assert_eq!(
            notification.kind(),
            crate::capabilities::notification::NotificationKind::Warning
        );
        assert_eq!(notification.title(), Some("LSP"));
        assert_eq!(notification.message(), "workspace loaded");
    }

    #[test]
    fn progress_notifications_include_details_and_completion() {
        let report = progress_notification(LanguageProgress {
            stage: LanguageProgressStage::Report,
            title: "Indexing".to_string(),
            message: Some("workspace".to_string()),
            percentage: Some(50),
        });
        assert_eq!(report.title(), Some("LSP"));
        assert_eq!(report.message(), "Indexing: workspace (50%)");

        let end = progress_notification(LanguageProgress {
            stage: LanguageProgressStage::End,
            title: "Indexing".to_string(),
            message: None,
            percentage: None,
        });
        assert_eq!(end.message(), "Indexing finished");
    }

    #[test]
    fn lifecycle_notifications_cover_start_failure_and_restart() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let mut model = initial_model(Some(directory.path().to_path_buf())).unwrap();
        open_file(&mut model, &directory.path().join("main.rs"));

        assert!(matches!(synchronize(&mut model), Some(Effect::Batch(_))));
        let notification = model.notifications.current().unwrap();
        assert_eq!(notification.title(), Some("LSP"));
        assert_eq!(
            notification.message(),
            "Starting language server `rust-analyzer`"
        );

        Box::new(LanguageCompletion {
            action: LanguageAction::Synchronize,
            service: None,
            result: Err(LanguageError(
                "Failed to start language server `rust-analyzer`: unavailable".to_string(),
            )),
        })
        .update(&mut model);
        let notification = model.notifications.current().unwrap();
        assert_eq!(notification.title(), Some("LSP"));
        assert_eq!(
            notification.message(),
            "Failed to start language server `rust-analyzer`: unavailable"
        );

        assert!(matches!(synchronize(&mut model), Some(Effect::Batch(_))));
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Restarting language server `rust-analyzer`"
        );
    }

    #[test]
    fn fatal_failure_discards_an_inflight_service_and_restarts_on_next_sync() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let (mut model, manager) = model_with_locations(directory.path(), Vec::new());
        model.diagnostics.restart_server();
        let generation = model.diagnostics.server_generation();
        let work = synchronize(&mut model).unwrap();

        Box::new(DiagnosticFailureCompletion {
            message: "Language server failed: stopped".to_string(),
            server_generation: generation,
            fatal: true,
        })
        .update(&mut model);
        assert!(run_feature(&manager, &mut model, work).is_none());
        assert!(!model.code_navigation.has_service());

        assert!(matches!(synchronize(&mut model), Some(Effect::Batch(_))));
        assert_eq!(
            model.notifications.current().unwrap().message(),
            "Restarting language server `rust-analyzer`"
        );
    }

    #[test]
    fn stale_or_nonfatal_failures_preserve_the_current_service() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let (mut model, _) = model_with_locations(directory.path(), Vec::new());

        let stale = Box::new(DiagnosticFailureCompletion {
            message: "old server failed".to_string(),
            server_generation: 1,
            fatal: true,
        })
        .update(&mut model);
        assert!(stale.is_none());
        assert!(model.code_navigation.has_service());

        let nonfatal = Box::new(DiagnosticFailureCompletion {
            message: "bad notification".to_string(),
            server_generation: 0,
            fatal: false,
        })
        .update(&mut model);
        assert!(nonfatal.is_some());
        assert!(model.code_navigation.has_service());
    }

    #[test]
    fn close_document_uses_existing_service_without_navigation() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let (mut model, manager) = model_with_locations(directory.path(), Vec::new());
        let path = directory.path().join("main.rs");

        let effect = close_document(&mut model, path).unwrap();
        assert!(model.code_navigation.is_busy());
        assert!(run_feature(&manager, &mut model, effect).is_none());
        assert!(!model.code_navigation.is_busy());
    }

    #[test]
    fn busy_synchronization_coalesces_to_one_trailing_event() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let (mut model, manager) = model_with_locations(directory.path(), Vec::new());

        let first = synchronize(&mut model).unwrap();
        assert!(model.code_navigation.is_busy());
        model
            .editor
            .active_editor()
            .unwrap()
            .insert_text(" ")
            .unwrap();
        assert!(synchronize(&mut model).is_none());
        assert!(model.code_navigation.has_trailing_sync());

        let trailing = run_feature(&manager, &mut model, first);
        assert!(trailing.is_some());
        assert!(!model.code_navigation.has_trailing_sync());
        assert!(model.code_navigation.is_busy());
        let trailing = trailing.unwrap();
        assert!(run_feature(&manager, &mut model, trailing).is_none());
        assert!(!model.code_navigation.is_busy());
    }

    #[test]
    fn multiple_results_use_the_shared_popup_and_open_the_selection() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("main.rs"), "fn main() {}").unwrap();
        let first = directory.path().join("first.rs");
        let second = directory.path().join("second.rs");
        std::fs::write(&first, "fn first() {}\n").unwrap();
        std::fs::write(&second, "fn second() {}\n").unwrap();
        let (mut model, manager) =
            model_with_locations(directory.path(), vec![location(&first), location(&second)]);

        let effect = manager
            .update(
                &mut model,
                Message::InvokeCommand(CommandId("code_references")),
            )
            .unwrap();
        let effect = run_feature(&manager, &mut model, effect);
        assert!(effect.is_none());
        assert_eq!(model.code_navigation.locations().len(), 2);
        assert!(model.modal.is_active(Context::CodeNavigation));
        assert_eq!(model.context_stack.last(), Some(&Context::CodeNavigation));

        manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("code_navigation_next")),
        );
        let open = manager.update(
            &mut model,
            Message::InvokeCommand(CommandId("code_navigation_open")),
        );
        let Some(Effect::OpenDocument(request)) = open else {
            panic!("expected selected-location open effect");
        };
        manager.update(&mut model, Message::FileOpened(request.perform()));
        let active = model.editor.active_buffer().unwrap();
        assert_eq!(model.editor.buffer_path(active), Some(second.as_path()));
        assert!(!model.modal.is_active(Context::CodeNavigation));
    }
}
