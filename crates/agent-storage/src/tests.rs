use eggplant_agent::{
    CancellationToken, MessageRole, RunId, RunStatus, SessionId, ToolResult, ToolResultStatus,
};

use crate::*;

fn finalized(id: &str, updated_at: i64) -> FinalizedRun {
    let session_id = SessionId::from(id);
    let run_id = RunId(1);
    FinalizedRun {
        session: SessionRecord {
            id: session_id.clone(),
            created_at: updated_at - 1,
            updated_at,
            workspace: Some("/workspace".to_string()),
            title: Some(format!("Session {id}")),
        },
        run: RunRecord {
            id: run_id,
            session_id: session_id.clone(),
            status: RunStatus::Complete,
            created_at: updated_at - 1,
            completed_at: Some(updated_at),
            provider: "fake".to_string(),
            model: None,
            editor_context: Some("main.rs:1".to_string()),
        },
        messages: vec![
            MessageRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 0,
                role: MessageRole::User,
                text: "find alpha".to_string(),
                created_at: updated_at - 1,
            },
            MessageRecord {
                session_id: session_id.clone(),
                run_id,
                sequence: 1,
                role: MessageRole::Assistant,
                text: "alpha done".to_string(),
                created_at: updated_at,
            },
        ],
        tool_calls: vec![ToolCallRecord {
            session_id,
            run_id,
            sequence: 0,
            call_id: "call-1".to_string(),
            name: "read".to_string(),
            arguments: "main.rs".to_string(),
            result: Some(ToolResult {
                call_id: "call-1".to_string(),
                status: ToolResultStatus::Success,
                content: "alpha contents".to_string(),
                truncated: false,
            }),
            created_at: updated_at - 1,
            completed_at: Some(updated_at),
        }],
    }
}

#[tokio::test]
async fn recent_transcript_and_search_share_deterministic_keysets() {
    let store = InMemoryStore::new();
    store
        .finalize_run(finalized("session-1", 10), CancellationToken::default())
        .await
        .unwrap();
    store
        .finalize_run(finalized("session-2", 20), CancellationToken::default())
        .await
        .unwrap();

    let first = store
        .recent_sessions(RecentQuery {
            limit: 1,
            ..RecentQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(first.sessions[0].id, SessionId::from("session-2"));
    let second = store
        .recent_sessions(RecentQuery {
            before: first.next,
            limit: 1,
            ..RecentQuery::default()
        })
        .await
        .unwrap();
    assert_eq!(second.sessions[0].id, SessionId::from("session-1"));
    assert_eq!(second.next, None);

    let transcript = store
        .transcript(
            SessionId::from("session-1"),
            TranscriptQuery {
                limit: 1,
                ..TranscriptQuery::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(transcript.messages.len(), 1);
    assert_eq!(transcript.messages[0].role, MessageRole::User);
    assert_eq!(transcript.runs.len(), 1);
    assert_eq!(transcript.tool_calls.len(), 1);
    let remainder = store
        .transcript(
            SessionId::from("session-1"),
            TranscriptQuery {
                after: transcript.next,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(remainder.messages[0].role, MessageRole::Assistant);
    assert_eq!(remainder.next, None);

    let messages = store
        .search(SearchQuery {
            text: Some("alpha".to_string()),
            role: Some(MessageRole::Assistant),
            ..SearchQuery::messages(SessionId::from("session-1"))
        })
        .await
        .unwrap();
    assert_eq!(messages.hits.len(), 1);
    assert_eq!(messages.hits[0].text, "alpha done");

    let tools = store
        .search(SearchQuery {
            text: Some("alpha".to_string()),
            tool_name: Some("read".to_string()),
            call_id: Some("call-1".to_string()),
            ..SearchQuery::tools(SessionId::from("session-1"))
        })
        .await
        .unwrap();
    assert_eq!(tools.hits.len(), 1);
    assert_eq!(tools.hits[0].status, Some(ToolResultStatus::Success));
}

#[tokio::test]
async fn bounds_identity_and_cancellation_are_enforced_before_writes() {
    let store = InMemoryStore::new();
    let mut large = finalized("large", 10);
    large.messages[1].text = "é🙂".repeat(40_000);
    store
        .finalize_run(large, CancellationToken::default())
        .await
        .unwrap();
    let page = store
        .transcript(SessionId::from("large"), TranscriptQuery::default())
        .await
        .unwrap();
    assert!(page.messages[1].text.len() <= 64 * 1024);
    assert!(page.messages[1].text.ends_with("[truncated]"));

    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert_eq!(
        store
            .finalize_run(finalized("cancelled", 30), cancelled)
            .await,
        Err(StorageError::Cancelled)
    );
    let mut invalid = finalized("invalid", 40);
    invalid.messages[0].run_id = RunId(99);
    assert_eq!(
        store
            .finalize_run(invalid, CancellationToken::default())
            .await,
        Err(StorageError::InvalidIdentity)
    );
    let recent = store.recent_sessions(RecentQuery::default()).await.unwrap();
    assert_eq!(recent.sessions.len(), 1);
    assert_eq!(
        store
            .transcript(SessionId::from("missing"), TranscriptQuery::default())
            .await,
        Err(StorageError::SessionNotFound(SessionId::from("missing")))
    );
}
