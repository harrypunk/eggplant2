# Agent tools

Status: bounded `read` and `grep` plus revision-safe exact-text `write` and no-overwrite `create` are implemented through provider tool calls. All four execute automatically without built-in approval prompts; broader mutations remain deferred.

The model sees small, stable tools. Eggplant owns validation and execution. Tools reuse existing capabilities where those capabilities already enforce the required invariant, and introduce a narrow adapter where an agent-safe precondition is missing.

## Existing Eggplant reuse map

| Capability | Existing source | Agent decision |
| --- | --- | --- |
| Text acceptance | `editor/src/document.rs` | Reuse `read_text_file` for regular-file/UTF-8/NUL checks, but add workspace authorization, symlink policy, and byte/line limits; it is not a complete agent `read`. |
| Owned snapshots | `editor/src/snapshot.rs`, `features/buffer_grep/work.rs` | Reuse owned `BufferId` + revision + text; `buffer_text_snapshot(id)` covers active and non-active buffers without activation. |
| Workspace traversal/search | `capabilities/search.rs`, `features/project_grep/work.rs`, `capabilities/filesystem.rs` | Reuse smart-case regex, ignore policy, and traversal, extracting a public bounded core where current functions are private. Do not invoke external grep. |
| Editor mutation | `editor/src/session.rs`, `active_editor/editing.rs`, `features/editing/target.rs` | Use `replace_exact_text` for explicit-buffer/path/revision/exact-text transactions. Existing commands remain active-buffer UI editing, not an agent API. |
| Save/reload | `editor/src/save.rs`, `session.rs`, `shell/buffer.rs`, `features/external_sync.rs` | Compose the same after-edit/save/completion and conflict protections; coordinate explicit-ID saves for non-active buffers. |
| File creation | `capabilities/filesystem.rs`, `editor/src/session.rs` | Reuse `CreateTarget`/no-overwrite creation and `create_file_document`; do not add an agent-specific filesystem writer. |

These are boundaries to reuse, not permission to reach through private feature state or make optional VCS a dependency of the agent.

## Common contract

Every tool invocation has:

- Current session and `RunId`.
- Provider-supplied call ID, preserved in the result.
- Stable tool name and validated arguments.
- Workspace root and the editor context captured for this call.
- Cancellation and step/output limits.

Every result has:

- The same call ID.
- Success, error, or cancellation status for new executions. `ToolResultStatus::Rejected` remains only for historical/storage compatibility; new executions do not use it.
- Bounded text/structured content suitable for the provider.
- A truncation indicator when output was limited.

There is no separate approval payload, pending approval state, or approval-only diff. Tool call IDs are correlation identities, not authorization tokens. A valid ID does not make an unsafe request valid. Persist only bounded tool arguments/results when session storage is added; large artifacts need a separate reference model. See [Agent persistence](agent-persistence.md).

## Shared validation

Before execution:

1. Check that the tool name and argument shape are supported.
2. Resolve the target against the fixed workspace root.
3. Canonicalize existing paths and validate symlink/parent rules.
4. Enforce the fixed workspace scope; current tools provide no per-call/session approval or grant that widens it.
5. Reject unsupported file types, invalid UTF-8 where text is required, and over-limit input.
6. Revalidate the expected buffer/document revision immediately before mutation.

Validation failures return a bounded tool error to the model and concise UI feedback. They must not panic, silently change the target, or retry as a more powerful operation. A provider response stopped for length must not execute its tool calls: parseable-looking arguments may be truncated.

## `read`

Read a bounded range from one text file or one open buffer snapshot.

### Inputs

- Path, or an explicitly supported buffer reference.
- Optional one-based start line and line count.
- Optional byte/character limit governed by the tool, not by the model.

### Behavior

- For an open buffer, use an owned `BufferTextSnapshot` when the request needs current unsaved text.
- For an on-disk file, reuse regular-file/text acceptance and workspace path rules.
- Preserve line numbers in output so the model can reference the source accurately.
- Do not follow a symlink outside the workspace or read arbitrary sensitive files.
- Return a truncation marker and next-range information when useful.

The first implementation should prefer explicit line ranges over whole-file reads. Large-file support means bounded access, not loading unlimited content into the prompt.

## `grep`

Search current buffer text or workspace files using Eggplant's existing smart-case/regex and traversal policy.

### Inputs

- Smart-case regular expression.
- Scope: current buffer or workspace.
- Optional path/glob restriction supported by the implementation.
- Bounded result count and context-line count.

### Behavior

- Current-buffer search uses an owned text snapshot and the existing search capability.
- Workspace search reuses project traversal, Git/built-in ignore handling, regular-file checks, and binary/invalid-text rejection.
- Search runs in a blocking effect with request identity; terminal input remains responsive.
- Results contain path, line number, matching text, and bounded context.
- Result ordering and caps are deterministic; truncation is explicit.

Do not invoke an external `grep`/shell command or create a second regex/ignore implementation. Do not bypass project ignore rules merely because the requester is an agent.

## `write`

The first mutation tool is narrow: replace exactly one non-overlapping old-text match in an existing open buffer with an expected revision. Old text must be non-empty and different from the replacement. Whole-file creation, delete, rename, and arbitrary patch formats are separate tools/slices. Reads of open buffers return `BufferId`, revision, and path metadata for constructing this request.

### Inputs

- Explicit `BufferId` and canonical path.
- Expected document revision captured in the worker's owned context.
- Exact old text.
- Replacement text.
- A user-visible reason/summary supplied by the workflow, not trusted as validation.

### Required sequence

1. The worker validates the captured open-buffer snapshot, canonical path, expected revision, and exactly one old-text match without changing the buffer.
2. With the write as the only call in its round, the worker emits `MutationRequested` plus an owned provider-neutral `AgentContinuation` and exits without completing the run. It never blocks on a UI channel.
3. On the main update path, `MutationExecutor` uses `MutationContext` to revalidate current buffer identity, canonical path, expected revision, exact old text, and save-in-flight state.
4. `MutationTool` applies one undoable editor transaction through the editor adapter, or returns an honest error without changing user text.
5. Eggplant runs the normal after-edit/auto-save/language-sync path. If the target is not the active buffer, it coordinates persistence and synchronization explicitly by `BufferId`/path rather than relying on active-document behavior.
6. `MutationExecutor` captures fresh context and resumes the provider with the bounded result, so subsequent reads cannot reuse pre-write text. Success means the transaction was accepted, not that persistence succeeded; save failures remain visible and never imply automatic rollback.

If the revision or exact text changed, Eggplant returns a conflict error. The model may request a fresh read and try again; it cannot force the write. A save already in flight for the target is also a retryable conflict, and switching the active buffer alone is not a conflict.

There is no approval step in this sequence: no `y`/`n` handling, mutation-specific `Esc` behavior, pending approval state, approval modal, or approval-only diff. `Ctrl-F7` retains its ordinary active-run cancellation meaning.

### New-file creation

`create` accepts only a workspace-relative path and full text content. It rejects existing paths and directories, then composes the existing `CreateTarget`/`filesystem::create` no-overwrite path, `DocumentFactory::open` syntax-aware buffer creation, and `EditorSession::create_file_document` full-text initialization. It uses the same automatic single-mutation-per-round `MutationRequested`/`MutationExecutor` flow as exact replacement. The resulting buffer uses the ordinary after-edit auto-save/language-sync pipeline, so nested paths and first-save persistence need no agent-specific filesystem writer. Existing-file `write` retains its exact old-text/revision contract. `MutationKind` provides typed dispatch to the `MutationTool` implementations.

### What not to do

- Do not invoke Normal/Insert active-buffer key commands as an unattended API.
- Do not write directly with unrestricted `std::fs::write` for an open buffer.
- Do not bypass auto-save, external-modification conflict checks, or dirty-buffer state.
- Do not let one automatic mutation weaken validation for later calls, other paths, Git operations, or shell commands.
- Do not accept a model-generated `BufferId`/revision as authoritative without matching it to current editor state.

Where existing “write” functionality is filesystem-oriented rather than revision-aware editing, add the narrow editor adapter instead of forcing the agent through an unsafe API.

## Automatic execution model

Following Pi core's lack of built-in tool approval prompts, `read`/`grep` and `write`/`create` execute automatically after validation. Eggplant has no per-call or session approval setting, `y`/`n` approval keys, approval modal, pending approval state, or approval-only diff. Automatic execution does not relax canonical workspace paths, `BufferId`/revision/exact-match checks, no-overwrite creation, save ordering, or cancellation.

Validation and execution failures produce honest error results; cancellation remains explicit. `ToolResultStatus::Rejected` is retained only for historical/storage compatibility; new executions never produce it. Legacy `waiting_for_approval` storage rows restore as cancelled rather than as active continuations.

## Tool execution and agent loop

Read-only tools execute as bounded effects and return typed results to the owning agent runtime. Mutation tools hand off from captured worker context to the main update path:

```text
model tool request (sole call in the round)
  → worker validates captured context
  → MutationRequested + owned AgentContinuation
  → main-update MutationExecutor through MutationContext
  → MutationTool revalidates and applies, or returns an error
  → explicit-ID save/language sync + fresh context capture
  → bounded tool result resumes the provider
```

The agent loop follows Pi's `packages/agent/src/agent-loop.ts`: there is no fixed tool-step or tool-call limit. It continues while new calls arrive and stops on natural completion, error/abort, an explicit end decision, or user cancellation. Output-size, validation, path/revision, and cancellation policies provide the safety boundaries instead of an arbitrary round cutoff.

## Result bounds

Set concrete defaults during implementation and test their boundaries. The contract requires limits for:

- Bytes/characters per read.
- Lines and context per result.
- Grep matches and total output.
- Transcript entries and retained tool detail.
- Cumulative tool output and provider input.

Limits should produce useful truncation metadata. Do not simply cut a UTF-8 string at an arbitrary byte or hide the fact that content was omitted.

### Implemented defaults

| Bound | Current limit |
| --- | --- |
| Tool arguments | 16 KiB per call; unsupported fields are rejected |
| File/snapshot input | 1 MiB per file, read with an extra-byte overflow check |
| `read` range | One-based offset; 1–200 lines per call |
| `grep` results | Smart-case regex compiled once; 50 matching lines and 0–2 context lines |
| Tool result | 50 KiB per call, with a UTF-8-safe `[truncated]` marker |
| Run tool activity | No fixed round/call count; natural completion or cancellation |
| Cumulative provider tool output | 100 KiB per run; another tool round after exhaustion fails explicitly |
| Retained tool events | 50 entries; results correlate with both call ID and current `RunId` |
| Panel preview | Two lines and 240 characters, plus an explicit truncation indicator |

Workspace grep uses the shared ignore policy and sorted path traversal. It processes one bounded file at a time rather than retaining all workspace text. A provider-round `Complete` event is not emitted to the session until the run has no further tool work; failed or cancelled rounds never execute queued calls.

## Deferred tools

The following require separate review and should not appear as accepted no-ops in the first tool registry:

- Arbitrary shell/process execution.
- Delete/rename/copy operations.
- Multi-file patches and automatic LSP workspace edits.
- Git stage/commit/reset/branch/merge operations.
- Network fetch, browser, credential, or secret-reading tools.
- Third-party dynamic tool loading.

Each needs its own path, validation, output, and failure semantics. A general `run_command` tool must not become a shortcut around those contracts.

## Required tests

For each tool, cover:

- Valid input and deterministic bounded output.
- Wrong argument shape, unknown tool, and unsupported file type.
- Workspace escape, absolute path abuse, symlink parent escape, and sensitive-file denial.
- Empty results, Unicode, large files, result caps, and truncation metadata.
- Cancellation and stale run/request completion.
- `MutationRequested` continuation identity and captured-context/main-update validation mismatches.
- Buffer/path/revision/exact-text conflicts.
- Save failure and honest user/provider result reporting.
- Preservation of ordinary editing when provider or tool execution fails.

Workspace tests use temporary directories and scripted fake providers; they require no real model, network, credentials, or user home directory.
