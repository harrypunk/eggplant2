use super::*;

#[test]
fn streams_ordered_text_and_rejects_stale_events() {
    let mut session = AgentSession::default();
    let run = session.begin("explain this".to_string()).unwrap();
    assert!(session.append_delta(run, "first"));
    assert!(session.append_delta(run, " second"));
    assert!(session.complete(run));
    assert!(!session.append_delta(run, "late"));

    let messages = session.messages();
    assert_eq!(messages[0].role, MessageRole::User);
    assert_eq!(messages[1].role, MessageRole::Assistant);
    assert_eq!(messages[1].text, "first second");
    assert_eq!(session.run().unwrap().status, RunStatus::Complete);
}

#[test]
fn thinking_and_answer_parts_preserve_order_and_coalesce_adjacent_deltas() {
    let mut session = AgentSession::default();
    let run = session.begin("explain this".to_string()).unwrap();
    assert!(session.append_thinking_delta(run, "plan"));
    assert!(session.append_thinking_delta(run, " more"));
    assert!(session.append_delta(run, "result"));
    assert!(session.append_thinking_delta(run, "check"));
    assert!(session.append_delta(run, "!"));
    assert!(session.complete(run));
    assert!(!session.append_thinking_delta(run, "late"));

    let message = &session.messages()[1];
    assert_eq!(message.text, "result!");
    assert_eq!(
        message.parts(),
        &[
            MessagePart::Thinking("plan more".to_string()),
            MessagePart::Answer("result".to_string()),
            MessagePart::Thinking("check".to_string()),
            MessagePart::Answer("!".to_string()),
        ]
    );
}

#[test]
fn cancel_preserves_partial_text_and_blocks_later_deltas() {
    let mut session = AgentSession::default();
    let run = session.begin("write".to_string()).unwrap();
    session.append_delta(run, "partial");
    assert!(session.cancel());
    assert!(!session.append_delta(run, "ignored"));
    assert!(!session.complete(run));
    assert_eq!(session.run().unwrap().status, RunStatus::Cancelled);
    assert_eq!(session.messages()[1].text, "partial");
}

#[test]
fn empty_prompt_and_concurrent_runs_are_rejected() {
    let mut session = AgentSession::default();
    assert!(session.begin(" ".to_string()).is_none());
    session.begin("one".to_string()).unwrap();
    assert!(session.begin("two".to_string()).is_none());
}

#[test]
fn tool_results_match_their_run_and_survive_cancellation() {
    let mut session = AgentSession::default();
    let first = session.begin("read".to_string()).unwrap();
    let call = ToolCall {
        id: "call-1".to_string(),
        name: "read".to_string(),
        arguments: "{}".to_string(),
    };
    let result = ToolResult {
        call_id: call.id.clone(),
        status: crate::ToolResultStatus::Cancelled,
        content: "cancelled".to_string(),
        truncated: false,
    };
    assert!(session.record_tool_call(first, &call));
    assert!(session.cancel());
    assert!(session.complete_tool_call(first, result.clone()));
    assert_eq!(session.tool_events()[0].result, Some(result.clone()));

    let second = session.begin("read again".to_string()).unwrap();
    assert_ne!(first, second);
    assert!(!session.complete_tool_call(first, result));
    assert_eq!(session.tool_events().len(), 1);
}

#[test]
fn approval_is_single_shot_and_bound_to_run_and_call() {
    let mut session = AgentSession::default();
    let run = session.begin("change".to_string()).unwrap();
    let call = ToolCall {
        id: "write-1".to_string(),
        name: "write".to_string(),
        arguments: "{}".to_string(),
    };
    let approval = ToolApproval {
        call: call.clone(),
        summary: "Replace text".to_string(),
        diff: "-old\n+new".to_string(),
    };
    assert!(session.record_tool_call(run, &call));
    assert!(session.request_approval(run, approval.clone()));
    assert!(!session.request_approval(run, approval.clone()));
    assert_eq!(session.run().unwrap().status, RunStatus::WaitingForApproval);
    assert!(session.resolve_approval(run, "another-call").is_none());
    assert_eq!(session.resolve_approval(run, &call.id), Some(approval));
    assert_eq!(session.run().unwrap().status, RunStatus::Streaming);
    assert!(session.pending_approval().is_none());
}

#[test]
fn cancellation_discards_pending_approval() {
    let mut session = AgentSession::default();
    let run = session.begin("change".to_string()).unwrap();
    let call = ToolCall {
        id: "write-1".to_string(),
        name: "write".to_string(),
        arguments: "{}".to_string(),
    };
    assert!(session.record_tool_call(run, &call));
    assert!(session.request_approval(
        run,
        ToolApproval {
            call,
            summary: "Replace text".to_string(),
            diff: String::new(),
        }
    ));
    assert!(session.cancel());
    assert!(session.pending_approval().is_none());
    assert_eq!(session.run().unwrap().status, RunStatus::Cancelled);
}

#[test]
fn accumulates_usage_but_rejects_a_different_run() {
    let mut session = AgentSession::default();
    let run = session.begin("question".to_string()).unwrap();
    let first = TokenUsage {
        input: 120,
        output: 30,
        total: 150,
    };
    let second = TokenUsage {
        input: 80,
        output: 20,
        total: 100,
    };

    assert!(session.record_usage(run, first));
    assert!(session.record_usage(run, second));
    assert!(!session.record_usage(RunId(run.0 + 1), first));
    assert_eq!(
        session.usage(),
        TokenUsage {
            input: 200,
            output: 50,
            total: 250,
        }
    );
    assert_eq!(session.latest_usage(), Some(second));
}

#[test]
fn provider_history_flattens_answered_turns_without_replaying_thinking() {
    let mut session = AgentSession::default();
    let first = session.begin("first question".to_string()).unwrap();
    session.append_thinking_delta(first, "hidden reasoning");
    session.append_delta(first, "first answer");
    session.complete(first);

    let failed = session.begin("unanswered failure".to_string()).unwrap();
    session.fail(failed, "provider failed".to_string());

    let cancelled = session.begin("partial question".to_string()).unwrap();
    session.append_delta(cancelled, "partial answer");
    session.cancel();

    let expected = vec![
        ProviderMessage::user("first question".to_string()),
        ProviderMessage::assistant("first answer".to_string(), Vec::new()),
        ProviderMessage::user("partial question".to_string()),
        ProviderMessage::assistant("partial answer".to_string(), Vec::new()),
    ];
    assert_eq!(session.provider_history(), expected);

    session.begin("active question".to_string()).unwrap();
    assert_eq!(session.provider_history(), expected);
}
