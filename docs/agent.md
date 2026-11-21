# AI agent architecture

Status: design only. No production agent implementation is claimed.

The agent is a primary Eggplant workflow, not a detached chat window or an excuse to bypass editor safety. It should make a small coding task feel like a natural extension of navigation, search, diagnostics, and editing.

Detailed tool contracts live in [Agent tools](agent-tools.md). Implementation order and exit criteria live in [PLAN Milestone 12](../PLAN.md#milestone-12-inline-ai-agent).

## Product principles

1. **Inline first:** ask a question or give a task without leaving the current editing context.
2. **One session, two views:** the inline popup and tool panel render the same conversation and run state.
3. **Visible work:** streaming text, tool calls, approvals, failures, and cancellation are understandable without reading logs.
4. **Small agent loop:** model response → bounded tool call → tool result → model response, with explicit stop conditions.
5. **Reuse Eggplant:** tools use existing filesystem, search, document, buffer, revision, save, and diff boundaries.
6. **Untrusted model output:** a model can request a tool; only Eggplant validates and authorizes its execution.
7. **No second editor:** agent updates use the same Elm-style message/effect loop and preserve one terminal owner.

## Lessons from Pi

Study the official [Pi repository](https://github.com/earendil-works/pi); the local reference checkout is `/home/j9/work/fork/pi`. Its useful dependency direction is `pi-ai` provider streams → `pi-agent-core` loop/tools → coding-agent sessions/adapters, with `pi-tui` separate from agent behavior. The full coding-agent now includes substantial retry, compaction, caching, extension, MCP, and session-tree machinery; read the core first rather than treating the entire checkout as minimal.

Adopt these ideas:

- A provider-neutral conversation made of owned messages and streaming events.
- An agent loop independent of terminal rendering and key handling.
- Tools described by narrow request/result contracts and executed outside the model abstraction.
- Session state that can drive interactive, print, or other frontends without encoding terminal policy.
- Useful defaults without requiring sub-agents, plan mode, an extension host, or a general plugin platform in the first slice.

Do not copy:

- Node.js or Pi as a runtime dependency of Eggplant.
- Pi's extension/package ecosystem, RPC surface, telemetry stack, or terminal toolkit.
- Pi's lack of a built-in permission boundary. Eggplant must make workspace access and mutations explicit.
- A large provider SDK abstraction before one fake provider and one real streaming provider have proven the required interface.

Pi is a reference implementation to learn from, not code to transliterate. Prefer the smallest Rust interfaces that satisfy Eggplant's actual workflows.

### Pi reading map

Read these in order before designing Eggplant interfaces:

| Area | Official source | What to learn |
| --- | --- | --- |
| Agent loop | [`agent-loop.ts`](https://github.com/earendil-works/pi/blob/main/packages/agent/src/agent-loop.ts), [`types.ts`](https://github.com/earendil-works/pi/blob/main/packages/agent/src/types.ts) | Ordinary async loop, discriminated events, validated tools, errors as results. |
| Provider streams | [`ai/types.ts`](https://github.com/earendil-works/pi/blob/main/packages/ai/src/types.ts), [`event-stream.ts`](https://github.com/earendil-works/pi/blob/main/packages/ai/src/utils/event-stream.ts) | Narrow stream contract, ordered deltas, authoritative final result. |
| Stateful wrapper | [`agent.ts`](https://github.com/earendil-works/pi/blob/main/packages/agent/src/agent.ts) | Active-run ownership, cancellation, transcript mirror, queued steering. |
| Coding tools | [`tools/index.ts`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/tools/index.ts), [`truncate.ts`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/tools/truncate.ts) | Small backend ports, model/UI result separation, actionable truncation. |
| Sessions | [`agent-session.ts`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/core/agent-session.ts), [`session-format.md`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/session-format.md) | Explicit ownership and finalized-message persistence; defer the full tree machinery. |
| UI boundary | [`interactive-mode.ts`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/modes/interactive/interactive-mode.ts), [`tool-execution.ts`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/src/modes/interactive/components/tool-execution.ts) | UI consumes session events; components do not execute tools. |
| Security limits | [`security.md`](https://github.com/earendil-works/pi/blob/main/packages/coding-agent/docs/security.md) | Project trust is not tool confinement; Eggplant needs stronger explicit boundaries. |

The same files are available under the local checkout for deeper reading.

## User experience

### Inline agent popup

The popup is a modal input workflow owned by the shell's existing context/modal policy. Its first version contains:

- A compact conversation/run summary above the input.
- A single-line Unicode-aware prompt using the existing text-input component.
- Streaming/queued/waiting-approval/error status.
- Enter to submit, Escape to hide, and an explicit cancel command for the active run.

Hiding the popup does not cancel the run. Reopening restores the current session. Multiline prompts, prompt history, session pickers, and rich Markdown are later enhancements, not prerequisites for the first useful workflow.

### Agent tool panel

The existing tools panel gains an Agent view. It is non-modal and shows:

- Conversation messages and streaming assistant text.
- Current run state and elapsed activity where useful.
- Tool call names, concise arguments/summaries, bounded output, and truncation.
- Pending write approvals and their diffs.
- Errors and cancellation without losing the previous transcript.

The panel and popup read the same `AgentSession`; neither owns a separate provider connection or agent loop. Opening the panel does not move editor focus unless the user explicitly focuses it.

### Suggested commands

The exact keymap is an implementation decision validated against existing contexts. Reserve a single agent leader group rather than scattering unrelated chords:

- Open/hide inline agent.
- Toggle the Agent tool panel.
- Submit prompt.
- Cancel active run.
- Approve/reject one pending mutation.

Reserved `AgentChat` and `AgentChatDialog` contexts are placeholders only; they are not evidence of an implementation.

## Session and run state

A provider-neutral session owns ordered conversation entries:

- User message, including explicitly captured editor context.
- Assistant text and optional provider metadata.
- Tool call and tool result, correlated by call ID.
- Run status: idle, waiting for provider, streaming, waiting for tool approval, cancelling, complete, failed, or cancelled.

Every run has a `RunId`. Streaming events also have a monotonically increasing sequence. Completion/event application requires the current session and matching run identity; late deltas from an old run cannot append to a new conversation.

Only one foreground run per session is required initially. A second submission while active is rejected or queued explicitly; it must not silently race the current run.

Start with in-memory or linear append-only session storage. Persist finalized messages rather than every live delta if persistence is added. Pi's parent/leaf session tree, compaction, recovery, and branching machinery are not initial requirements.

## Editor context

A prompt may capture an owned, visible context snapshot:

- Workspace root.
- Active `BufferId`, canonical path, and expected document revision.
- Cursor or selected range and the selected text.
- Optional diagnostics summary requested by the user.

Do not copy the whole workspace or every open buffer into each prompt. Tools retrieve additional content on demand. Show the user what context will be sent, and provide a way to submit without active-buffer content.

Context is a point-in-time snapshot, not authority to edit. A later mutation must revalidate `BufferId` and expected revision on the main update path.

## Runtime boundaries

The intended ownership is:

| Module | Responsibility |
| --- | --- |
| `app::agent` | UI-neutral messages, run/event identity, tool request/result values, provider and runtime ports. |
| Agent runtime adapter | Provider streaming, bounded agent loop, cancellation, tool-call orchestration. |
| `features/agent` | Session/update state, commands, typed effects/completions, popup/panel coordination. |
| Agent components | Concrete popup and panel rendering from resolved props; no I/O or provider types. |
| Existing capabilities | Filesystem/search/document/buffer/save/diff behavior and their safety checks. |
| Shell runtime | One event loop, effect scheduling, modal/focus/cursor precedence. |

Provider SDK types must not appear in `Model`, components, filesystem tools, or editor code. Ratatui types must not appear in the agent runtime.

### Existing Eggplant seams

- `manager/effect.rs` provides the generic effect/completion envelope; `features/notifications.rs` shows custom async execution, and `features/code_navigation/work.rs` already forwards multiple typed events through the shared sender. Use a similarly focused streaming adapter.
- `tools/state.rs` owns only tool-panel visibility/active selection. Agent transcript/run state must remain in `AgentSession` so hiding the panel preserves it. Extend the existing tool-content dispatcher in `ui.rs`; do not register a duplicate `PanelSlot::Tools`.
- `component/text_input.rs` supplies a bordered single-line input with Unicode display-width and end-of-value cursor placement. It is not a multiline prompt or arbitrary-cursor editor.
- `capabilities/document.rs` supplies safe user-visible open behavior and the lifecycle port pattern; use it for navigation rather than treating it as the agent's bounded read tool.
- `config/mod.rs` and `main.rs` are the configuration and composition seams for an injected provider. Typed `[features.agent]` settings belong here.

### Streaming through the existing effect model

A run may emit many typed events rather than one final completion. Its effect adapter can forward ordered run events through the existing message sender, subject to these rules:

- No provider-specific global `Message` variants or runtime match arms.
- No independent terminal/event loop.
- Coalesce high-frequency text deltas if needed; never reorder tool-call boundaries.
- Bound queued output and provider/tool result sizes.
- Cancellation stops further work and stale events are ignored.
- Hiding UI never owns the worker lifetime.

If the current one-shot `FeatureEffect` envelope is insufficient, introduce a focused streaming adapter at that boundary rather than pretending a stream is a single completion.

## Agent loop

The first loop is intentionally small:

1. Append user input and captured context.
2. Request a streaming model response.
3. Append ordered assistant text and tool calls.
4. Validate a tool call against the registered tool contract.
5. Execute read-only tools within limits, or request approval for a mutation.
6. Append a bounded tool result.
7. Continue until the provider stops, the user cancels, an error occurs, or a step/output limit is reached.

Provider failures become ordered session/run error data rather than destroying the loop or panicking the UI. Do not execute tool calls from a response stopped for length or whose arguments may be truncated. Use the provider stream's authoritative final response to reconcile live deltas; a mutable partial response is not an immutable event-time snapshot.

No autonomous sub-agents, background swarms, planning framework, or unrestricted shell loop is required. Step limits and cancellation are product features, not optional hardening.

## Provider configuration

Start with a fake streaming provider for deterministic tests and one real provider selected by a configuration spike. Configuration should separate:

- Provider type and model name.
- Endpoint where applicable.
- Environment variable name for credentials.
- Request/step/output limits.

Do not persist API keys in transcripts, configuration examples, logs, notifications, or tool output. Provider errors should be concise and must not dump authorization headers or full request bodies.

## Safety summary

- Read and grep are bounded and workspace-scoped by default.
- Writes require explicit approval and expected-revision validation.
- Active-buffer UI commands are not unattended editing APIs.
- No arbitrary shell/process execution in the initial agent.
- No automatic LSP workspace edits or Git mutations.
- Tool results and model text are data, never implicit commands to Eggplant.
- Ordinary editing remains usable when no provider is configured or a run fails.

See [Agent tools](agent-tools.md) for path, result, revision, and approval contracts.

## Non-goals for the first implementation

- General plugin/extension hosts or third-party native tool loading.
- Sub-agents, plan mode, autonomous background agents, or multi-session orchestration.
- Arbitrary shell commands, browser automation, or credential tools.
- Full Markdown/LSP completion UI inside the prompt.
- Automatic commit, reset, branch, merge, or history mutation.
- Bypassing auto-save, dirty-buffer, UTF-8, path, or revision invariants.

## Decisions deferred to concrete spikes

- The first real provider and its minimal streaming/configuration interface.
- Exact token/byte/line/step limits, informed by provider behavior and terminal responsiveness.
- Final keybindings after conflict and context tests.
- Session persistence/resume format; the first useful workflow may remain in-memory.
- Token accounting and context compaction after transcript/tool limits are measured.
- Multi-file patch and session-review UX after one approved revision-safe write works.

Do not resolve these questions by introducing a general framework. Record the chosen behavior when a real consumer requires it.

## Testing strategy

Use scripted fake providers and fake tool calls to cover:

- Streaming order, coalescing, cancellation, and stale run events.
- Popup hide/reopen and shared popup/panel state.
- Modal precedence, narrow terminals, Unicode, and bounded transcripts.
- Provider failure without exposing secrets or losing ordinary editing.
- Tool argument/path/result validation and iteration limits.
- Approval rejection, revision conflicts, and save failure without false success.
- No real provider credentials or network access in workspace tests.

Real-provider smoke tests remain manual and must not become build requirements.
