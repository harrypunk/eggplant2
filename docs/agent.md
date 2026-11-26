# AI agent architecture

Status: M12.1–M12.5 are implemented. Inline chat, OpenAI streaming, bounded tools, approved writes, and SeaORM SQLite persistence are complete. Cloud backends and backend-specific full-text search remain deferred.

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

The panel and popup are two presentations of one Agent workflow. Both receive surface-specific `AgentViewProps` projections from one adapter: session, prompt, command candidates, storage warning, and theme. `AgentSurface` selects the active workflow for shared submission and commands. `AgentPrompt` owns text editing, draft origin, and selection transitions; `AgentPromptView` owns the shared menu-above-input layout and cursor geometry, so presentation changes do not require parallel prompt implementations. The dialog uses shared props directly and fills the available main panel within symmetric margins instead of using fixed dimensions; its assistant area flexes with the remaining height while the prompt retains its measured height. The panel adds only focus and `AgentInputMode`. Neither presentation owns a separate provider connection, session, command catalog, or agent loop. Only dialog `path:line` prefill, panel Normal/Insert modes, focus presentation, and layout differ. Opening the Agent panel focuses its input. Panel Normal supports `i`, `?` help, the configured leader menu, and `Ctrl-H`/`Ctrl-L` navigation; Insert accepts text (including Space), submits with Enter, and returns to Normal with `Esc` without losing panel focus. Help is a separate modal popup, closed with `?` or `Esc`, rather than content in the conversation layout. A focused panel uses a double border and an explicit mode title. Mouse or other future input sources can still request focus directly through the geometry-only focus controller.

### Shared usage footer

The inline dialog and tool panel reserve one bottom row for the same pure `AgentFooter`. The left side shows cumulative `↑input ↓output` tokens for the current in-memory session and the latest provider round's context occupancy; the right side shows `provider · model`. OpenAI-compatible streaming requests use `stream_options.include_usage`, and usage events are accepted only for their matching run. Multiple tool rounds accumulate independently while the latest round supplies the context sample.

Set `features.agent.context_window` to the active model's token capacity to display `NN.N%/capacity`. OpenAI-compatible `/models` responses do not standardize this value, so the footer deliberately shows `context ?` without a capacity and `?/capacity` before the first usage report. It never guesses a capacity from a model name. Context occupancy uses warning styling above 70% and error styling above 90%.

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

`AgentChatDialog`, `AgentApproval`, and `AgentChatHelp` are modal-session contexts, not context-stack entries. `AgentChat` and `AgentChatInsert` are derived from panel focus and `AgentInputMode`. `ModalController` is authoritative for Agent, help, and provider-login overlay visibility as well as modal input ownership. Dialog ↔ approval transitions replace the active modal phase atomically. `AgentDialogState` records only workflow lifecycle (`Closed`, `Open`, or `Suspended`) and is not a second visibility source.

### Provider-login UX shell

`Space a p` opens provider login. The workflow has separate authentication, provider, and form pages.

#### Menus and navigation

- Account providers: `openai`, `qwen.com`, and `kimi.com`. These remain placeholders.
- The API-key menu contains the compatible providers listed below and generic key-only entries.
- Up/Down and Ctrl-N/Ctrl-P navigate menus.
- Enter opens the selected submenu or provider page.
- Escape steps back through provider pages before closing the dialog. During model loading or activation it closes the dialog immediately; late completions are ignored.

#### OpenAI-compatible forms

Each compatible provider has an independent descriptor and default endpoint. Add new providers to this table rather than expanding provider lists in the shared behavior below.

| Provider ID | Default endpoint |
| --- | --- |
| `openai` | `https://api.openai.com/v1` |
| `kimi-for-coding` | `https://api.kimi.com/coding/v1` |
| `qwen-tokenplan-individual-cn` | `https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1` |

Protocol-specific notes:

- Kimi: Pi uses Anthropic Messages at `https://api.kimi.com/coding`. Eggplant deliberately uses Kimi's OpenAI-compatible `/coding/v1` API to reuse the existing provider-neutral streaming adapter rather than adding a second transport stack.
- Qwen TokenPlan Individual CN: Pi separates Individual (Singapore) from CN (Beijing). Eggplant's explicitly CN-scoped provider uses Pi's Beijing endpoint and OpenAI Completions protocol.

The shared form and workflow apply to every table row:

- Endpoints remain editable for custom compatible endpoints, and model choices always come from the authenticated catalog rather than an embedded list.
- The API-key field is masked through an injected `TextInputViewStyle`; neither field exposes the key in debug output.
- Both fields use wrappable `TextInput` views, and dialog height follows their measured content.
- Tab loops focus through Endpoint and API key. No backward-focus binding is needed.
- Ctrl-V reads through the `ClipboardReader` interface in a background effect. Focus/edit revisions discard stale clipboard completions instead of pasting into the wrong field.
- Enter asynchronously saves the endpoint and key through `ProviderCredentialStore`. Durable saves use `~/.eggplant/ai/data.sqlite` by default; an absolute `session_db` can override it.
- Providers have independent credentials, but saving does not select a provider or model or mutate the active provider configuration.

#### Catalog and model activation

- A successful compatible-provider save fetches authenticated `GET {base_url}/models` through the injected `OpenAiProviderFactory`, using the selected provider's own stored credential.
- Successful responses are also normalized and upserted under that provider ID through `ModelCatalogStore` with `checked_at` and optional validators. Cache failures warn without blocking selection.
- Explicit loading, selection, and activation pages replace boolean workflow flags.
- Only model IDs returned by the endpoint are selectable. The list uses a bounded, selection-following viewport.
- Enter persists the chosen model and then activates that provider. Activation failures leave the selection available and do not install a provider.
- Catalog failures return to the preserved endpoint/key form, so the user can retry without re-entering secrets.
- Startup restores the most recently activated OpenAI-compatible provider/model when configuration does not explicitly select `openai` or `fake`; explicit configuration takes precedence. Providers retain independent last-selected models, while the latest activation timestamp determines the single restored runtime provider.

#### `/model` selector

- `/model` loads cached catalogs for every stored OpenAI-compatible credential, then refreshes each provider independently in the background with authenticated `GET {base_url}/models` and persists fresh results under the same provider IDs.
- The view groups models into provider sections with indented model rows. Selection uses a typed `ProviderModel { provider_id, model_id }` identity rather than a model-name string, and only that one identity is activated.
- Cached sections remain navigable during `Refreshing`. Selection is preserved by full provider/model identity; a missing selection falls back to the first model without retaining its old numeric index.
- Successful refreshes merge into the grouped list while failed providers retain their cached sections with a warning. Empty results or missing compatible credentials produce an error.
- Enter activates the selected available provider/model, persists `selected_model` on that credential, and restores the latest activation on restart. Activation failures leave the selector open without changing the active provider.
- The selector shares the general modal actions: Up/Down and Ctrl-N/Ctrl-P navigate, Enter selects, and Escape closes while restoring the originating Agent surface.

#### Agent slash commands

A leading `/` in the inline dialog or panel Insert input opens a live-filtered command menu above the prompt. The menu derives candidates from the shared prompt state and catalog: `/provider`, `/model`, `/thinking`, and `/logout`.

- Command state remains shared, but view composition projects candidates only to the prompt-active `AgentSurface`. When both presentations are visible, the modal dialog takes precedence; otherwise only the focused panel in Insert mode can show the menu. The inactive presentation retains the shared text without rendering a second menu.
- Opening or changing the filter selects nothing; Enter preserves incomplete or unknown slash text without executing it.
- Tab quick-fills the first match.
- Ctrl-N/Ctrl-P quick-fill and move selection with wrapping while retaining the original query's candidates.
- Enter executes an explicitly selected command or an exact typed command after trimming surrounding whitespace, matching Pi's submission behavior. Editing away a selection therefore does not prevent a complete command from running; slash text is never submitted as a provider message.
- Executing a command clears the composer and command selection before opening its follow-up UI, matching Pi's accepted-command behavior. Unselected or unknown slash text remains editable.
- The menu height follows content within the available layout. Its selection-following viewport keeps the selected row visible.
- `/model` opens the cached model selector; `/thinking` and `/logout` currently publish placeholder notifications.

#### Ownership and deferred work

`provider_login` owns dialog state, commands, and modal registration. Each provider owns a separate descriptor under `provider_login/providers/`; `openai_compatible_form.rs` owns the provider-neutral endpoint/key form, and `openai_compatible_work.rs` contains only the shared protocol workflow and accepts the selected provider ID dynamically. Adding another compatible provider does not modify existing provider descriptors or add a provider-specific workflow branch. `component/agent/provider_login.rs` is a pure menu, form, and placeholder view. Provider login and model selection own modal sessions rather than context-stack entries. Opening either from the inline Agent dialog suspends and later restores the `AgentChatDialog` modal session without duplicating it in the context stack; panel Insert state resumes automatically.

Account/OAuth authentication, token refresh, and providers requiring other wire protocols remain deferred. Every compatible provider in the table supports API-key login, catalog selection, activation, `/model` refresh, and restart restoration through the shared credential-store and OpenAI-compatible factory interfaces.

## Session and run state

A provider-neutral session owns ordered conversation entries:

- User message, including explicitly captured editor context.
- Assistant text and optional provider metadata.
- Tool call and tool result, correlated by call ID.
- Run status: idle, waiting for provider, streaming, waiting for tool approval, cancelling, complete, failed, or cancelled.

Every run has a `RunId`. Streaming events also have a monotonically increasing sequence. Completion/event application requires the current session and matching run identity; late deltas from an old run cannot append to a new conversation.

Only one foreground run per session is required initially. A second submission while active is rejected or queued explicitly; it must not silently race the current run.

Use SQLite persistence for finalized messages rather than every live delta. Pi's parent/leaf session tree, compaction, recovery, and branching machinery are not initial requirements. For relational persistence and the SeaORM boundary, see [Agent persistence](agent-persistence.md).

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

`commands.rs` owns the typed `AgentCommand` catalog, registration, and dispatch; `adapter.rs` is the small `Model`-aware composition adapter. `dialog.rs` owns inline open/close and `path:line` prefill; `panel.rs` owns focus, Normal/Insert, and help. `context.rs` captures editor/workspace inputs, `approval.rs` owns write resolution, and `run.rs` owns provider/tool execution without `Model`. `work.rs` retains submission and interruption policy through consumer-sized `SubmissionContext` and `InterruptionContext` interfaces rather than receiving the global `Model`; only `adapter.rs` supplies concrete surface, workspace, modal, notification, and persistence coordination. Unit tests use a local fake context, while integration tests retain real-adapter coverage.

The intended ownership is:

| Module | Responsibility |
| --- | --- |
| `eggplant-agent` | Standalone provider-neutral session, run/event identity, tool request/result values, and runtime ports; no editor/terminal/filesystem dependency. |
| Agent runtime adapter | Provider streaming, bounded agent loop, cancellation, tool-call orchestration. |
| `agent-storage` | `SessionStore` port, finalized records, and provider credential persistence; see [Agent persistence](agent-persistence.md). |
| `agent-storage-seaorm` | Opt-in SeaORM SQLite adapter, versioned migrations, transactions, keyset pagination, and search; no cloud drivers. |
| `features/agent` | Session/update state, commands, typed effects/completions, popup/panel coordination. |
| Agent components | Concrete popup and panel rendering from resolved props; no I/O or provider types. |
| Existing capabilities | Filesystem/search/document/buffer/save/diff behavior and their safety checks. |
| Shell runtime | One event loop, effect scheduling, modal/focus/cursor precedence. |

Provider SDK types must not appear in `Model`, components, filesystem tools, or editor code. Ratatui types must not appear in the agent runtime.

### Existing Eggplant seams

- `manager/effect.rs` provides the generic effect/completion envelope; `features/notifications.rs` shows custom async execution, and `features/code_navigation/work.rs` already forwards multiple typed events through the shared sender. Use a similarly focused streaming adapter.
- `tools/state.rs` owns only tool-panel visibility/active selection. Agent transcript/run state must remain in `AgentSession` so hiding the panel preserves it. Extend the existing tool-content dispatcher in `ui.rs`; do not register a duplicate `PanelSlot::Tools`.
- `component/text_input.rs` supplies a stateful bordered input with local value/cursor state and Unicode-scalar Left/Right movement. `component/text_layout.rs` keeps measurement, visual wrapping, cursor geometry, and viewport selection consistent. `component/auto_height.rs` owns dialog size clamps and reserved-content policy. The Agent panel uses `TextFlow::Wrapped` with the shared `AutoHeight` policy; the margin-filled dialog measures the same wrapped input against its available content area instead of choosing a fixed outer size. Picker and grep inputs retain `TextFlow::SingleLine`. The prompt remains one logical line, not a multiline editor.
- `capabilities/document.rs` supplies safe user-visible open behavior and the lifecycle port pattern; use it for navigation rather than treating it as the agent's bounded read tool.
- `config/mod.rs` declares typed `[features.agent]` settings. `composition/agent.rs` is the composition seam for injected providers and persistence; `main.rs` only invokes `configure_agent` and does not construct provider or database implementations.

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

`base_url`, `model`, and `api_key_env` default to the values shown. `context_window` is optional and affects only footer occupancy reporting. Omit `provider` to retain the offline fake provider. The adapter reads the key only when a run starts; it does not store it in TOML, transcripts, notifications, logs, or tool output. Request/transport/status errors are redacted and bounded.

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
- Session persistence/resume format; the first useful workflow may use the default SQLite store directly.
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
