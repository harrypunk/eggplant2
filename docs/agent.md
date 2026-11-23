# AI agent architecture

Status: M12.1–M12.5 are implemented. Inline chat, OpenAI streaming, bounded tools, approved writes, and in-memory/opt-in SeaORM SQLite persistence are complete. Cloud backends and backend-specific full-text search remain deferred.

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
- A bottom, auto-height Unicode-aware prompt using the shared text-input component; assistant messages remain above it.
- Streaming/queued/waiting-approval/error status.
- Enter to submit, Escape to hide, and an explicit cancel command for the active run.

Hiding the popup does not cancel the run. Reopening restores the current session. Multiline prompts, prompt history, session pickers, and rich Markdown are later enhancements, not prerequisites for the first useful workflow.

### Agent tool panel

The existing tools panel gains an Agent view. It is non-modal and shows:

- Project-scoped conversation messages and streaming assistant text above a bottom, auto-height input. No file or buffer needs to be open; users can write freeform prompts and project-file references such as `@src/main.rs`. `Ctrl-L` focuses panel input and `Ctrl-H` returns to the editor; Enter uses the same submit workflow.
- Current run state and elapsed activity where useful.
- Tool call names, concise arguments/summaries, bounded output, and truncation.
- Pending write approvals and their diffs.
- Errors and cancellation without losing the previous transcript.

The panel and popup are two presentations of one Agent workflow. Both receive the same `AgentViewProps` from one adapter: session, prompt, command candidates, storage warning, and theme. `AgentSurface` selects the active workflow for shared submission and commands. `AgentPrompt` owns text editing, draft origin, and selection transitions; `AgentPromptView` owns the shared menu-above-input layout and cursor geometry, so presentation changes do not require parallel prompt implementations. The dialog uses shared props directly; the panel adds only focus and `AgentInputMode`. Neither presentation owns a separate provider connection, session, command catalog, or agent loop. Only dialog `path:line` prefill, panel Normal/Insert modes, focus presentation, and layout differ. Opening the Agent panel focuses its input. Panel Normal supports `i`, `?` help, and `Ctrl-H`/`Ctrl-L` navigation; Insert accepts text, submits with Enter, and returns to Normal with `Esc` without losing panel focus. Help is a separate modal popup, closed with `?` or `Esc`, rather than content in the conversation layout. A focused panel uses a double border and an explicit mode title. Mouse or other future input sources can still request focus directly through the geometry-only focus controller.

### Current keybindings

| Action | Binding |
| --- | --- |
| Open inline Agent dialog | `Ctrl-I` |
| Dismiss dialog while preserving the run | `Esc` |
| Submit prompt | `Enter` |
| Interrupt active run | `Ctrl-F7` |
| Toggle shared Agent tool panel | `Space a t` |
| Open provider-login placeholder | `Space a p` |
| Focus Agent panel input | `Ctrl-L` |
| Return from Agent panel to editor | `Ctrl-H` |

The inline dialog requires an open project, not an active buffer. With an active workspace file, its prompt starts as editable `workspace/relative/path:line`; without an eligible buffer, it starts empty. External buffers do not supply prefill and do not prevent chatting. The project panel likewise starts with a freeform prompt and does not require an active buffer; `@path` references remain ordinary prompt text and do not implicitly attach file contents. The scripted provider used in this slice streams `Simulated response to: <prompt>`; `fail` simulates a provider failure.

`AgentChatDialog` is the inline modal context. `AgentChat` and `AgentChatInsert` are derived from panel focus and `AgentInputMode`; they are not stored in the context stack. `AgentChatHelp` owns the stored modal help context. `ModalController` is authoritative for Agent, help, and provider-login overlay visibility as well as modal input ownership. `AgentDialogState` records only workflow lifecycle (`Closed`, `Open`, or `Suspended`) and is not a second visibility source.

### Provider-login UX shell

`Space a p` opens a provider-login placeholder popup. In either the inline Agent dialog or the panel's Insert input, a leading `/` opens a minimal command menu immediately above the text input whose candidates are derived directly from the same prompt state and catalog. It shows command names with short descriptions, with no title or key-hint rows. Its height is derived from content and bounded by available layout space; when the list exceeds that viewport, the shared selection-following window keeps the quick-filled command visible. Empty conversations leave the assistant area blank rather than showing placeholder text. `/`, `/p`, and `/PRO` filter the shared catalog containing `/provider`, `/model`, `/thinking`, and `/logout`. Opening or typing a filter selects nothing, so Enter does nothing. Tab quick-fills the first match; Ctrl-N/Ctrl-P quick-fill and move the selection with wrapping while retaining the original query's candidates. Only then does Enter execute the selected command, never submitting slash text as a provider message. Model, thinking, and logout commands currently publish placeholder notifications. The separate `provider_login` feature owns dialog state, commands, and modal registration; `component/agent/provider_login.rs` is a pure placeholder view. Opening it from the inline dialog suspends and later restores `AgentChatDialog`, while panel Insert state is derived and resumes automatically. Authentication, credential storage, and provider mutation are deferred.

## Session and run state

A provider-neutral session owns ordered conversation entries:

- User message, including explicitly captured editor context.
- Assistant text and optional provider metadata.
- Tool call and tool result, correlated by call ID.
- Run status: idle, waiting for provider, streaming, waiting for tool approval, cancelling, complete, failed, or cancelled.

Every run has a `RunId`. Streaming events also have a monotonically increasing sequence. Completion/event application requires the current session and matching run identity; late deltas from an old run cannot append to a new conversation.

Only one foreground run per session is required initially. A second submission while active is rejected or queued explicitly; it must not silently race the current run.

Start with in-memory or linear append-only session storage. Persist finalized messages rather than every live delta if persistence is added. Pi's parent/leaf session tree, compaction, recovery, and branching machinery are not initial requirements. For SQLite/relational persistence and the proposed SeaORM boundary, see [Agent persistence](agent-persistence.md).

## Editor context

Only the inline file-focused flow captures an editor context. Project-panel runs have no implicit buffer context; their workspace root remains the project identity used by tools and persistence.

The inline prompt may capture an owned, visible context snapshot:

- Workspace root.
- Active `BufferId`, canonical path, and expected document revision.
- Cursor or selected range and the selected text.
- Optional diagnostics summary requested by the user.

Do not copy the whole workspace or every open buffer into each prompt. Tools retrieve additional content on demand. Show the user what context will be sent, and provide a way to submit without active-buffer content.

Context is a point-in-time snapshot, not authority to edit. A later mutation must revalidate `BufferId` and expected revision on the main update path.

## Runtime boundaries

`commands.rs` owns the typed `AgentCommand` catalog, registration, and dispatch. `dialog.rs` owns inline open/close and `path:line` prefill; `panel.rs` owns focus, Normal/Insert, and help. `context.rs` captures editor/workspace inputs, `approval.rs` owns write resolution, and `run.rs` owns provider/tool execution without `Model`. `work.rs` retains submission, interruption, and completion application rather than becoming a second command registry.

The intended ownership is:

| Module | Responsibility |
| --- | --- |
| `eggplant-agent` | Standalone provider-neutral session, run/event identity, tool request/result values, and runtime ports; no editor/terminal/filesystem dependency. |
| Agent runtime adapter | Provider streaming, bounded agent loop, cancellation, tool-call orchestration. |
| `agent-storage` | `SessionStore` port, finalized records, and in-memory default; see [Agent persistence](agent-persistence.md). |
| `agent-storage-seaorm` | Opt-in SeaORM SQLite adapter, versioned migrations, transactions, keyset pagination, and search; no cloud drivers. |
| `features/agent` | Session/update state, commands, typed effects/completions, popup/panel coordination. |
| Agent components | Concrete popup and panel rendering from resolved props; no I/O or provider types. |
| Existing capabilities | Filesystem/search/document/buffer/save/diff behavior and their safety checks. |
| Shell runtime | One event loop, effect scheduling, modal/focus/cursor precedence. |

Provider SDK types must not appear in `Model`, components, filesystem tools, or editor code. Ratatui types must not appear in the agent runtime.

### Existing Eggplant seams

- `manager/effect.rs` provides the generic effect/completion envelope; `features/notifications.rs` shows custom async execution, and `features/code_navigation/work.rs` already forwards multiple typed events through the shared sender. Use a similarly focused streaming adapter.
- `tools/state.rs` owns only tool-panel visibility/active selection. Agent transcript/run state must remain in `AgentSession` so hiding the panel preserves it. Extend the existing tool-content dispatcher in `ui.rs`; do not register a duplicate `PanelSlot::Tools`.
- `component/text_input.rs` supplies a stateful bordered input with local value/cursor state and Unicode-scalar Left/Right movement. `component/text_layout.rs` keeps measurement, visual wrapping, cursor geometry, and viewport selection consistent. `component/auto_height.rs` owns dialog size clamps and reserved-content policy. Agent uses `TextFlow::Wrapped` with the shared `AutoHeight` policy; picker and grep inputs retain `TextFlow::SingleLine`. The prompt remains one logical line, not a multiline editor.
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

`eggplant-agent` exposes an object-safe `AgentProvider` port: an async method that accepts an owned `AgentRequest` and emits ordered `AgentEvent` values through a callback. `AgentRequest` includes a `CancellationToken`, so `Ctrl-F7` stops provider work as well as changing session status.

The optional `eggplant-agent-openai` adapter implements that port for OpenAI-compatible `chat/completions` endpoints. Configure it in `~/.eggplant/config.toml`:

```toml
[features.agent]
provider = "openai"
model = "gpt-4o-mini"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
```

`base_url`, `model`, and `api_key_env` default to the values shown. Omit `provider` to retain the offline fake provider. The adapter reads the key only when a run starts; it does not store it in TOML, transcripts, notifications, logs, or tool output. Request/transport/status errors are redacted and bounded.

Streaming responses use incremental UTF-8-safe SSE decoding. Deltas arrive through the same session as the fake provider, with an authoritative completion or failure. A response stopped for length fails rather than executing tool calls. The localhost test suite verifies actual HTTP request shape, streaming events, Unicode fragmentation, status errors, and API-key redaction without external network access.

The adapter also declares bounded `read`, `grep`, and approved exact-text `write` functions and merges incremental `tool_calls` deltas by index. Tool execution receives owned workspace/buffer context, not `Model`. Reads of open buffers include their `BufferId`, revision, and path so a model can construct a write request without hidden editor access. Follow-up requests serialize user, assistant, and tool-role history so the model can consume results without a second conversation or UI event loop. A `[DONE]` completion closes only the current provider round; the overall run completes after the final model response. The declarative run state machine allows three tool-execution rounds, eight calls, and 100 KiB of cumulative tool output, and rejects invalid or duplicate call IDs before execution.

A write must be the only call in its round. The worker validates a captured open-buffer snapshot, emits a bounded approval plus an owned `AgentContinuation`, and exits without completing the run. `y` approves once and `n` rejects; `Esc` hides the unchanged approval and `Ctrl-F7` cancels without resuming. The main update path revalidates identity, canonical path, revision, save-in-flight state, and exactly one old-text match before applying one undoable transaction. It then resumes the continuation with fresh snapshots, explicit-ID auto-save/language-sync effects, and an honest result that does not claim the file was saved.

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
- Future tuning of the implemented byte/line/step limits, informed by provider behavior and terminal responsiveness.
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
