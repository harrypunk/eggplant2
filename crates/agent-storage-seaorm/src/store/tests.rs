use eggplant_agent::{
    CancellationToken, MessageRole, RunId, RunStatus, SessionId, ToolResult, ToolResultStatus,
};
use eggplant_agent_storage::{
    FinalizedRun, MessageRecord, RecentQuery, RunRecord, SearchKind, SearchQuery, SessionRecord,
    SessionStore, StorageError, ToolCallRecord, TranscriptPage, TranscriptQuery,
};
use futures_util::{TryStreamExt, stream};
use sea_orm::sqlx::{
    Executor,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

use super::*;

fn finalized(session: &str, run_number: u64, updated_at: i64) -> FinalizedRun {
    let session_id = SessionId::from(session);
    let run_id = RunId(run_number);
    FinalizedRun {
        session: SessionRecord {
            id: session_id.clone(),
            created_at: 1,
            updated_at,
            workspace: Some("/workspace".to_string()),
            title: Some(format!("Session {session}")),
        },
        run: RunRecord {
            id: run_id,
            session_id: session_id.clone(),
            status: RunStatus::Complete,
            created_at: updated_at - 2,
            completed_at: Some(updated_at),
            provider: "fake".to_string(),
            model: Some("test-model".to_string()),
            editor_context: Some("main.rs:1".to_string()),
        },
        messages: vec![
            MessageRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 0,
                role: MessageRole::User,
                text: format!("Alpha prompt run {run_number}"),
                created_at: updated_at - 2,
            },
            MessageRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 1,
                role: MessageRole::Assistant,
                text: format!("alpha answer run {run_number}"),
                created_at: updated_at - 1,
            },
            MessageRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 2,
                role: MessageRole::Assistant,
                text: "done".to_string(),
                created_at: updated_at,
            },
        ],
        tool_calls: vec![
            ToolCallRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 0,
                call_id: "shared-call".to_string(),
                name: "read".to_string(),
                arguments: format!("Alpha argument run {run_number}"),
                result: Some(ToolResult {
                    call_id: "shared-call".to_string(),
                    status: ToolResultStatus::Success,
                    content: format!("Alpha result run {run_number}"),
                    truncated: false,
                }),
                created_at: updated_at - 1,
                completed_at: Some(updated_at),
            },
            ToolCallRecord {
                session_id,
                run_id,
                sequence: 1,
                call_id: "shared-call".to_string(),
                name: "write".to_string(),
                arguments: "write argument".to_string(),
                result: Some(ToolResult {
                    call_id: "shared-call".to_string(),
                    status: ToolResultStatus::Error,
                    content: "write result".to_string(),
                    truncated: true,
                }),
                created_at: updated_at - 1,
                completed_at: Some(updated_at),
            },
        ],
    }
}

async fn store_at(path: &Path) -> SeaOrmSessionStore {
    SeaOrmSessionStore::connect(path).await.unwrap()
}

async fn seed<S: SessionStore>(store: &S, runs: Vec<FinalizedRun>) -> Result<(), StorageError> {
    stream::iter(runs.into_iter().map(Ok::<FinalizedRun, StorageError>))
        .try_for_each(|run| store.finalize_run(run, CancellationToken::default()))
        .await
}

async fn assert_recent_equal(
    reference: &SeaOrmSessionStore,
    sqlite: &SeaOrmSessionStore,
    query: RecentQuery,
) -> eggplant_agent_storage::RecentPage {
    let (reference, sqlite) = tokio::join!(
        reference.recent_sessions(query.clone()),
        sqlite.recent_sessions(query)
    );
    let reference = reference.unwrap();
    assert_eq!(reference, sqlite.unwrap());
    reference
}

async fn assert_transcript_equal(
    reference: &SeaOrmSessionStore,
    sqlite: &SeaOrmSessionStore,
    session_id: &str,
    query: TranscriptQuery,
) -> TranscriptPage {
    let (reference, sqlite) = tokio::join!(
        reference.transcript(SessionId::from(session_id), query.clone()),
        sqlite.transcript(SessionId::from(session_id), query)
    );
    let reference = reference.unwrap();
    assert_eq!(reference, sqlite.unwrap());
    reference
}

async fn assert_search_equal(
    reference: &SeaOrmSessionStore,
    sqlite: &SeaOrmSessionStore,
    query: SearchQuery,
) -> eggplant_agent_storage::SearchPage {
    let (reference, sqlite) = tokio::join!(reference.search(query.clone()), sqlite.search(query));
    let reference = reference.unwrap();
    assert_eq!(reference, sqlite.unwrap());
    reference
}

#[tokio::test]
async fn durable_reconnect_creates_parents_and_preserves_records() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("nested/agent storage ?#.sqlite");
    let store = store_at(&path).await;
    let run = finalized("durable", 7, 70);
    let expected = run.clone().bounded();
    seed(&store, vec![run]).await.unwrap();
    drop(store);

    let reopened = store_at(&path).await;
    let transcript = reopened
        .transcript(SessionId::from("durable"), TranscriptQuery::default())
        .await
        .unwrap();
    assert_eq!(
        transcript,
        TranscriptPage {
            session: expected.session,
            runs: vec![expected.run],
            messages: expected.messages,
            tool_calls: expected.tool_calls,
            next: None,
        }
    );
    assert!(path.is_file());

    let relative = SeaOrmSessionStore::connect("relative-agent.sqlite").await;
    assert!(matches!(relative, Err(StorageError::Backend(_))));
}

#[tokio::test]
async fn finalize_upserts_scoped_records_without_unique_call_ids() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_at(&temp.path().join("upsert.sqlite")).await;
    let original = finalized("upsert", 3, 30);
    seed(&store, vec![original.clone()]).await.unwrap();
    let mut replacement = original;
    replacement.session.updated_at = 40;
    replacement.session.title = Some("replacement".to_string());
    replacement.run.status = RunStatus::Cancelled;
    replacement.run.completed_at = Some(40);
    replacement.messages[0].text = "replacement text".to_string();
    replacement.tool_calls[0].result = Some(ToolResult {
        call_id: "shared-call".to_string(),
        status: ToolResultStatus::Cancelled,
        content: "cancelled result".to_string(),
        truncated: false,
    });
    replacement.tool_calls[1].result = None;
    let expected = replacement.clone().bounded();
    seed(&store, vec![replacement]).await.unwrap();

    let transcript = store
        .transcript(SessionId::from("upsert"), TranscriptQuery::default())
        .await
        .unwrap();
    assert_eq!(
        transcript,
        TranscriptPage {
            session: expected.session,
            runs: vec![expected.run],
            messages: expected.messages,
            tool_calls: expected.tool_calls,
            next: None,
        }
    );
    assert_eq!(
        store
            .recent_sessions(RecentQuery::default())
            .await
            .unwrap()
            .sessions
            .len(),
        1
    );
    let repeated = store
        .search(SearchQuery {
            call_id: Some("shared-call".to_string()),
            ..SearchQuery::tools(SessionId::from("upsert"))
        })
        .await
        .unwrap();
    assert_eq!(repeated.hits.len(), 2);
    assert_eq!(repeated.hits[1].status, None);
    assert_eq!(repeated.hits[1].text, "write argument");
}

#[tokio::test]
async fn recent_transcript_and_search_match_sqlite_keysets() {
    let temp = tempfile::tempdir().unwrap();
    let sqlite = store_at(&temp.path().join("parity.sqlite")).await;
    let reference = store_at(&temp.path().join("reference.sqlite")).await;
    let mut other_workspace = finalized("session-gamma", 1, 20);
    other_workspace.session.workspace = Some("/elsewhere".to_string());
    let runs = vec![
        finalized("session-alpha", 2, 30),
        finalized("session-alpha", 1, 10),
        finalized("session-beta", 1, 30),
        other_workspace,
    ];
    let (reference_seed, sqlite_seed) =
        tokio::join!(seed(&reference, runs.clone()), seed(&sqlite, runs));
    reference_seed.unwrap();
    sqlite_seed.unwrap();

    let first = assert_recent_equal(
        &reference,
        &sqlite,
        RecentQuery {
            limit: 2,
            ..RecentQuery::default()
        },
    )
    .await;
    assert_eq!(first.sessions.len(), 2);
    assert!(first.next.is_some());
    let second = assert_recent_equal(
        &reference,
        &sqlite,
        RecentQuery {
            before: first.next,
            limit: 2,
            ..RecentQuery::default()
        },
    )
    .await;
    assert_eq!(second.sessions.len(), 1);
    assert_eq!(second.next, None);
    assert_recent_equal(
        &reference,
        &sqlite,
        RecentQuery {
            workspace: Some("/workspace".to_string()),
            limit: 0,
            ..RecentQuery::default()
        },
    )
    .await;
    assert_recent_equal(
        &reference,
        &sqlite,
        RecentQuery {
            limit: usize::MAX,
            ..RecentQuery::default()
        },
    )
    .await;

    let transcript_first = assert_transcript_equal(
        &reference,
        &sqlite,
        "session-alpha",
        TranscriptQuery {
            limit: 2,
            ..TranscriptQuery::default()
        },
    )
    .await;
    assert_eq!(transcript_first.runs.len(), 1);
    assert_eq!(transcript_first.tool_calls.len(), 2);
    let transcript_second = assert_transcript_equal(
        &reference,
        &sqlite,
        "session-alpha",
        TranscriptQuery {
            after: transcript_first.next,
            limit: 2,
        },
    )
    .await;
    assert_eq!(transcript_second.runs.len(), 2);
    assert_eq!(transcript_second.tool_calls.len(), 4);
    let transcript_third = assert_transcript_equal(
        &reference,
        &sqlite,
        "session-alpha",
        TranscriptQuery {
            after: transcript_second.next,
            limit: 2,
        },
    )
    .await;
    assert_eq!(transcript_third.messages.len(), 2);
    assert_eq!(transcript_third.next, None);

    let messages_first = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            text: Some("Alpha".to_string()),
            role: Some(MessageRole::User),
            limit: 1,
            ..SearchQuery::messages(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert_eq!(messages_first.hits.len(), 1);
    assert!(messages_first.next.is_some());
    let messages_second = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            text: Some("Alpha".to_string()),
            role: Some(MessageRole::User),
            after: messages_first.next,
            limit: 1,
            ..SearchQuery::messages(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert_eq!(messages_second.next, None);
    let uppercase = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            text: Some("ALPHA".to_string()),
            ..SearchQuery::messages(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert_eq!(uppercase.hits.len(), 4);

    let tools_first = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            text: Some("Alpha".to_string()),
            tool_name: Some("read".to_string()),
            call_id: Some("shared-call".to_string()),
            limit: 1,
            ..SearchQuery::tools(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert!(tools_first.next.is_some());
    let tools_second = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            text: Some("Alpha".to_string()),
            tool_name: Some("read".to_string()),
            call_id: Some("shared-call".to_string()),
            after: tools_first.next,
            limit: 1,
            ..SearchQuery::tools(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert_eq!(tools_second.next, None);
    let repeated_ids = assert_search_equal(
        &reference,
        &sqlite,
        SearchQuery {
            kind: SearchKind::Tool,
            call_id: Some("shared-call".to_string()),
            role: Some(MessageRole::User),
            ..SearchQuery::tools(SessionId::from("session-alpha"))
        },
    )
    .await;
    assert_eq!(repeated_ids.hits.len(), 4);
}

#[tokio::test]
async fn persisted_output_is_bounded_exactly_like_the_port() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_at(&temp.path().join("bounded.sqlite")).await;
    let mut large = finalized("large", 1, 10);
    large.session.workspace = Some("é🙂".repeat(4_000));
    large.session.title = Some("title".repeat(1_000));
    large.run.status = RunStatus::Failed("failure".repeat(2_000));
    large.run.provider = "provider".repeat(100);
    large.run.model = Some("model".repeat(100));
    large.run.editor_context = Some("context".repeat(2_000));
    large.messages[1].text = "é🙂".repeat(40_000);
    large.tool_calls[0].call_id = "call".repeat(100);
    large.tool_calls[0].name = "name".repeat(100);
    large.tool_calls[0].arguments = "arguments".repeat(3_000);
    large.tool_calls[0].result = Some(ToolResult {
        call_id: "result-call".repeat(100),
        status: ToolResultStatus::Rejected,
        content: "🙂result".repeat(20_000),
        truncated: false,
    });
    let expected = large.clone().bounded();
    seed(&store, vec![large]).await.unwrap();

    let page = store
        .transcript(SessionId::from("large"), TranscriptQuery::default())
        .await
        .unwrap();
    assert_eq!(
        page,
        TranscriptPage {
            session: expected.session,
            runs: vec![expected.run],
            messages: expected.messages,
            tool_calls: expected.tool_calls,
            next: None,
        }
    );
    assert!(page.messages[1].text.len() <= 64 * 1024);
    assert!(page.messages[1].text.ends_with("[truncated]"));
}

#[tokio::test]
async fn identity_cancellation_and_missing_sessions_match_the_port() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_at(&temp.path().join("validation.sqlite")).await;
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        store
            .finalize_run(finalized("cancelled", 1, 10), cancelled)
            .await,
        Err(StorageError::Cancelled)
    );

    let mut invalid_message = finalized("invalid-message", 1, 20);
    invalid_message.messages[0].run_id = RunId(99);
    assert_eq!(
        store
            .finalize_run(invalid_message, CancellationToken::default())
            .await,
        Err(StorageError::InvalidIdentity)
    );
    let mut invalid_tool = finalized("invalid-tool", 1, 30);
    invalid_tool.tool_calls[0].session_id = SessionId::from("other");
    assert_eq!(
        store
            .finalize_run(invalid_tool, CancellationToken::default())
            .await,
        Err(StorageError::InvalidIdentity)
    );
    assert!(
        store
            .recent_sessions(RecentQuery::default())
            .await
            .unwrap()
            .sessions
            .is_empty()
    );
    assert_eq!(
        store
            .transcript(SessionId::from("missing"), TranscriptQuery::default())
            .await,
        Err(StorageError::SessionNotFound(SessionId::from("missing")))
    );
}

#[tokio::test]
async fn sqlite_integer_overflow_returns_an_error_and_rolls_back() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_at(&temp.path().join("overflow.sqlite")).await;
    let mut run_overflow = finalized("run-overflow", 1, 10);
    let overflow_id = RunId(u64::try_from(i64::MAX).unwrap() + 1);
    run_overflow.run.id = overflow_id;
    run_overflow
        .messages
        .iter_mut()
        .for_each(|message| message.run_id = overflow_id);
    run_overflow
        .tool_calls
        .iter_mut()
        .for_each(|call| call.run_id = overflow_id);
    let overflow_result = store
        .finalize_run(run_overflow, CancellationToken::default())
        .await;
    assert!(
        matches!(overflow_result, Err(StorageError::Backend(_))),
        "{overflow_result:?}"
    );

    let mut message_overflow = finalized("message-overflow", 1, 20);
    message_overflow.messages[1].sequence = u64::MAX;
    assert!(matches!(
        store
            .finalize_run(message_overflow, CancellationToken::default())
            .await,
        Err(StorageError::Backend(_))
    ));
    assert!(
        store
            .recent_sessions(RecentQuery::default())
            .await
            .unwrap()
            .sessions
            .is_empty()
    );
}

#[tokio::test]
async fn corrupt_databases_and_migration_failures_are_backend_errors() {
    let temp = tempfile::tempdir().unwrap();
    let corrupt = temp.path().join("corrupt.sqlite");
    std::fs::write(&corrupt, "this is not a SQLite database").unwrap();
    assert!(matches!(
        SeaOrmSessionStore::connect(&corrupt).await,
        Err(StorageError::Backend(_))
    ));

    let incompatible = temp.path().join("incompatible.sqlite");
    let options = SqliteConnectOptions::new()
        .filename(&incompatible)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    pool.execute("CREATE TABLE conflicting_schema (value TEXT)")
        .await
        .unwrap();
    pool.execute("CREATE INDEX idx_agent_sessions_recent ON conflicting_schema (value)")
        .await
        .unwrap();
    pool.close().await;
    let migration_result = SeaOrmSessionStore::connect(&incompatible).await;
    assert!(
        matches!(migration_result, Err(StorageError::Backend(_))),
        "{migration_result:?}"
    );
}

#[tokio::test]
async fn an_exclusively_locked_database_returns_an_error_without_panicking() {
    let temp = tempfile::tempdir().unwrap();
    let store = store_at(&temp.path().join("locked.sqlite")).await;
    let mut connection = store
        .db
        .get_sqlite_connection_pool()
        .acquire()
        .await
        .unwrap();
    connection.execute("BEGIN EXCLUSIVE").await.unwrap();

    let locked = store
        .finalize_run(finalized("locked", 1, 10), CancellationToken::default())
        .await;
    assert!(matches!(locked, Err(StorageError::Backend(_))));
    connection.execute("ROLLBACK").await.unwrap();
    drop(connection);

    store
        .finalize_run(finalized("locked", 1, 10), CancellationToken::default())
        .await
        .unwrap();
    assert_eq!(
        store
            .recent_sessions(RecentQuery::default())
            .await
            .unwrap()
            .sessions
            .len(),
        1
    );
}
