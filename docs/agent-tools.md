# Agent tools

Status: bounded `read` and `grep` are implemented through provider tool calls. Approved revision-safe `write` remains design-only.

The model sees small, stable tools. Eggplant owns validation and execution. Tools reuse existing capabilities where those capabilities already enforce the required invariant, and introduce a narrow adapter where an agent-safe precondition is missing.

## Existing Eggplant reuse map

| Capability | Existing source | Agent decision |
| --- | --- | --- |
| Text acceptance | `editor/src/document.rs` | Reuse `read_text_file` for regular-file/UTF-8/NUL checks, but add workspace authorization, symlink policy, and byte/line limits; it is not a complete agent `read`. |
| Owned snapshots | `editor/src/snapshot.rs`, `features/buffer_grep/work.rs` | Reuse owned `BufferId` + revision + text for current-buffer reads/search; add explicit-ID snapshot access for non-active buffers. |
| Workspace traversal/search | `capabilities/search.rs`, `features/project_grep/work.rs`, `capabilities/filesystem.rs` | Reuse smart-case regex, ignore policy, and traversal, extracting a public bounded core where current functions are private. Do not invoke external grep. |
| Editor mutation | `editor/src/session.rs`, `active_editor/editing.rs`, `features/editing/target.rs` | Add a new explicit-buffer/revision/exact-text transaction adapter. Existing commands are active-buffer UI editing, not an agent API. |
| Save/reload | `editor/src/save.rs`, `session.rs`, `shell/buffer.rs`, `features/external_sync.rs` | Compose the same after-edit/save/completion and conflict protections; coordinate explicit-ID saves for non-active buffers. |
| Approval diff UI | `features/vcs/diff.rs`, `buffer_diff.rs` | Reuse presentation/identity patterns, not `vcs::diff` itself. Proposed-before/after buffer diffs need a small bounded text-diff adapter. |

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
- Success, validation failure, execution failure, rejection, or cancellation status.
- Bounded text/structured content suitable for the provider.
- A truncation indicator when output was limited.
- Optional UI-only detail such as a diff, without sending unnecessary data to the provider.

Tool call IDs are correlation identities, not authorization tokens. A valid ID does not make an unsafe request valid. Persist only bounded tool arguments/results when session storage is added; large artifacts need a separate reference model. See [Agent persistence](agent-persistence.md).

## Shared validation

Before execution:

1. Check that the tool name and argument shape are supported.
2. Resolve the target against the fixed workspace root.
3. Canonicalize existing paths and validate symlink/parent rules.
4. Enforce workspace scope unless a separate, explicit user grant applies.
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

The first mutation tool should be narrow: replace exact text in an existing open buffer with an expected revision. Whole-file creation, delete, rename, and arbitrary patch formats are separate tools/slices.

### Inputs

- Explicit `BufferId` and canonical path.
- Expected document revision captured before approval.
- Exact old text or a well-defined range plus expected text.
- Replacement text.
- A user-visible reason/summary supplied by the workflow, not trusted as validation.

### Required sequence

1. Compute a preview/diff without changing the buffer.
2. Present the path, summary, and bounded diff in the Agent panel or approval popup.
3. Wait for explicit user approval of this call.
4. On the main update path, revalidate buffer identity, canonical path, expected revision, and exact old text/range.
5. Apply one undoable editor transaction through the editor adapter.
6. Run the normal after-edit/auto-save/language-sync path. If the target is not the active buffer, coordinate persistence and synchronization explicitly by `BufferId`/path rather than relying on active-document behavior.
7. Report success only after the transaction is accepted; report save failure separately and never claim the edit was rolled back automatically.

If the revision or exact text changed, reject with a conflict. The model may request a fresh read and try again; it cannot force the write.

### What not to do

- Do not invoke Normal/Insert active-buffer key commands as an unattended API.
- Do not write directly with unrestricted `std::fs::write` for an open buffer.
- Do not bypass auto-save, external-modification conflict checks, or dirty-buffer state.
- Do not silently expand approval to later writes, other paths, Git operations, or shell commands.
- Do not accept a model-generated `BufferId`/revision as authoritative without matching it to current editor state.

Where existing “write” functionality is filesystem-oriented rather than revision-aware editing, add the narrow editor adapter instead of forcing the agent through an unsafe API.

## Approval model

Read/grep inside the workspace may run without per-call approval, subject to limits. Mutations require approval by default.

An approval applies to one tool call ID plus an immutable summary of:

- Tool name and target.
- Expected revision or filesystem precondition.
- Proposed change/diff.

Any change to those values requires a new approval. Hiding a popup, switching buffers, or starting another run must not accidentally approve the request. Rejection and cancellation append explicit tool results so the model can respond appropriately.

A future user setting may relax read-only approval, but unrestricted “allow all tools” is not required for the first version.

## Tool execution and agent loop

Read-only tools execute as bounded effects and return typed results to the owning agent runtime. Mutation tools have two phases:

```text
model tool request
  → validate and compute preview
  → pending approval in AgentSession
  → user accepts/rejects
  → revalidate and execute or append rejection
  → bounded tool result to provider
```

The agent loop must have a maximum step count and maximum cumulative tool output. Repeated invalid calls, path escapes, revision conflicts, and truncation count toward stop conditions rather than creating infinite retries.

## Result bounds

Set concrete defaults during implementation and test their boundaries. The contract requires limits for:

- Bytes/characters per read.
- Lines and context per result.
- Grep matches and total output.
- Transcript entries and retained tool detail.
- Diff size shown in approval UI.
- Tool calls per run and cumulative provider input.

Limits should produce useful truncation metadata. Do not simply cut a UTF-8 string at an arbitrary byte or hide the fact that content was omitted.

### M12.3 implementation defaults

| Bound | Current limit |
| --- | --- |
| Tool arguments | 16 KiB per call; unsupported fields are rejected |
| File/snapshot input | 1 MiB per file, read with an extra-byte overflow check |
| `read` range | One-based offset; 1–200 lines per call |
| `grep` results | Smart-case regex compiled once; 50 matching lines and 0–2 context lines |
| Tool result | 50 KiB per call, with a UTF-8-safe `[truncated]` marker |
| Run tool activity | Three execution rounds and eight calls |
| Cumulative provider tool output | 100 KiB per run; another tool round after exhaustion fails explicitly |
| Retained tool events | 50 entries; results correlate with both call ID and current `RunId` |
| Panel preview | Two lines and 240 characters, plus an explicit truncation indicator |

Workspace grep uses the shared ignore policy and sorted path traversal. It processes one bounded file at a time rather than retaining all workspace text. A provider-round `Complete` event is not emitted to the session until the run has no further tool work; failed or cancelled rounds never execute queued calls.

## Deferred tools

The following require separate review and should not appear as accepted no-ops in the first tool registry:

- Arbitrary shell/process execution.
- New-file/delete/rename/copy operations.
- Multi-file patches and automatic LSP workspace edits.
- Git stage/commit/reset/branch/merge operations.
- Network fetch, browser, credential, or secret-reading tools.
- Third-party dynamic tool loading.

Each needs its own path, approval, output, and failure semantics. A general `run_command` tool must not become a shortcut around those contracts.

## Required tests

For each tool, cover:

- Valid input and deterministic bounded output.
- Wrong argument shape, unknown tool, and unsupported file type.
- Workspace escape, absolute path abuse, symlink parent escape, and sensitive-file denial.
- Empty results, Unicode, large files, result caps, and truncation metadata.
- Cancellation and stale run/request completion.
- Approval rejection and approval/input mismatch.
- Buffer/path/revision/exact-text conflicts.
- Save failure and honest user/provider result reporting.
- Preservation of ordinary editing when provider or tool execution fails.

Workspace tests use temporary directories and scripted fake providers; they require no real model, network, credentials, or user home directory.
