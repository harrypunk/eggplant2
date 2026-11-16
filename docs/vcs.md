# VCS addon

The VCS integration is an **optional user-experience addon**, not a core editor feature. Documents, buffers, editing, and automatic persistence remain fully usable without Git or the VCS page.

For keybindings, see [Key input, sequences, and hints](key-input.md).

## Scope

The addon currently provides:

- Branch or abbreviated detached-HEAD display in the editor status bar.
- A full-screen status page with common hints and a one-line HEAD reference.
- Dynamic staged and unstaged file sections.
- Stage/unstage for regular files and deletions, plus bulk stage-all/unstage-all operations.
- Toggleable inline diffs with caching.
- Safe file opening through the existing document capability.
- Explicit commits of already-staged changes with a multiline scratch-message editor.

Discard remains a placeholder. A separate history page provides bounded commit summaries with short IDs, subjects, authors, relative times, and local/remote branch labels following each commit's author for refs pointing exactly at that commit. The Git status HEAD reference uses the same relative-time formatter. Toggling a commit lists its changed files inline. Opening the commit shows a right-side detail with full hash, complete message, author, full ISO date-time with offset, and its file list. Each detail file can toggle its own first-parent diff with the same theme colors and cache behavior as file diffs. Merge commits intentionally use the first parent in this slice. Tree changes are filtered to regular blobs so directory objects never render as text, and multi-file commits retain every changed path. History loads 20 commits per page; the header shows a prominent `n load more` hint plus loading and retry feedback.

## Ownership and dependency direction

- `crates/vcs` owns Helix VCS/Gitoxide interaction and returns owned branch, change, history, and diff values.
- `features/vcs/` owns request generations, loading/error state, duplicate suppression, operation state, and stale-result rejection. Its commit workflow owns a separate scratch `EditorSession` and the save-before-commit barrier.
- `page/vcs.rs`, `page/vcs_history.rs`, and `page/vcs_commit.rs` render feature state without performing repository work.
- `AppPage` and `Context::Vcs` provide shell navigation; closing the page restores the previous editor or explorer context.
- No Gitoxide types enter application snapshots or Ratatui components.

Reduced-trust repository discovery and neutral non-repository/unborn states keep failures non-blocking.

## Page model

The status page has three regions:

1. Common key hints and optional notices.
2. A one-line HEAD reference with the latest short commit ID and subject.
3. Content-driven staged and unstaged sections, hidden when empty.

Selection follows one staged-first order. Empty sections consume no space, and inline diffs appear directly beneath their expanded file. `?` toggles a bordered bottom help panel; Escape dismisses only the panel, while `q` retains page-close behavior. Commit browsing stays on the separate history page rather than embedding a log in Git status.

Changed files use the existing safe document-opening and explorer-reveal path. Deleted files are not recreated.

The commit page shows a read-only staged-file summary in the semantic `file_path` color above a multiline scratch editor. It supports ordinary navigation, Insert editing, and undo/redo without file-backed auto-save. Closing with `q` preserves the in-memory draft; `Ctrl-c` explicitly discards it. Submission locks editing until completion, preserves the message on failure, and clears it on success.

## Mutation behavior

Real stage/unstage operations use filtered worktree-to-blob conversion and locked index writes directly through `gix`; production code does not require an external Git executable. Bulk operations validate all targets and use one final index write rather than partially applying a sequence of single-file writes.

Operations:

- Run in revisioned background effects.
- Refuse pending-save states.
- Preserve worktree contents when unstaging.
- Merge successful changes into the current staged-first model instead of rescanning the repository.
- Reject conflicts and unsupported file types explicitly.
- Preserve ready content during background status refreshes.

Explicit `Ctrl-s` submission saves modified file-backed buffers and waits for in-flight saves before committing. The backend holds the index lock, builds a tree from exactly the staged index, validates identity/message/staged changes and repository state, and updates HEAD through Gitoxide with expected-parent protection. Unstaged and untracked content is never added implicitly. Hooks and signing are outside this slice. Success clears the draft and refreshes HEAD, status, history, and diff bases without reloading unchanged worktree files.

## Diff behavior

The toggle command expands and collapses a bounded unified diff generated from HEAD, index, and worktree blobs:

- Added lines use `diff.plus`.
- Removed lines use `diff.minus`.
- Hunk headers use `diff.delta`.
- Context uses normal base text rather than uniformly dimmed output.

Expanded diffs render every fetched line. An ellipsis appears only when the backend's 200-line safety limit is reached.

Completed diffs are cached per path and staged state. Closing and reopening a toggle does not recompute them. Status refreshes and mutations invalidate the cache.

Selected file rows use a bold `>` marker rather than the default theme's reversed `ui.menu.selected` background. Empty and loading states apply the explicit dark base background.

## Active-buffer diff gutter

The addon loads the active file's HEAD content in a background effect and applies it through `EditorSession::set_buffer_diff_base`, which rejects stale document revisions. Helix `DiffHandle` tracks subsequent in-memory edits automatically. Editor snapshots expose only `Added`, `Modified`, and `Deleted` line markers, keeping Helix VCS types outside rendering.

The existing gutter displays a vertical `│` for every change, using the theme's addition, hunk, or removal colors without changing its width. The shape is fixed for now; configurable indicator styles can be added later. Untracked files use an empty base, non-repositories remain unmarked, and loaded bases are cached per buffer. Switching/opening buffers requests the active base, while external reloads invalidate it. The existing watcher also recognizes `.git` HEAD, ref, log, and packed-ref changes: repository metadata refreshes HEAD/status state, invalidates cached bases, and reloads the active buffer. Per-request generations prevent an older base response from replacing a newer result. No external Git process is used.

## Deferred work

- Reviewed discard and fast-forward-only merge workflows.
- Conflict resolution, staging of unsupported entry types, and other advanced Git behavior.
