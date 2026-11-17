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
- Switch-and-fast-forward merges to existing local target branches without fetching or rewriting history.

Discard remains a placeholder. A separate history page provides bounded commit summaries with short IDs, subjects, authors, relative times, and local/remote branch labels following each commit's author for refs pointing exactly at that commit. The Git status HEAD reference and history rows share one commit-message component, including relative-time formatting. Toggling a commit lists its changed files inline. Opening the commit shows a right-side detail with full hash, complete message, author, full ISO date-time with offset, and its file list. An explicit `HistoryPane::{List, Detail}` mode owns layout, commands, and vertical navigation independently of whether detail data is loading, ready, or failed, so list commands cannot run behind a visible detail. Each detail file can toggle its own first-parent diff with the same theme colors and cache behavior as file diffs. Merge commits intentionally use the first parent in this slice. Tree changes are filtered to regular blobs so directory objects never render as text, and multi-file commits retain every changed path. History loads 20 commits per page; the header shows a prominent `n load more` hint plus loading and retry feedback.

## Ownership and dependency direction

- `crates/vcs` owns Helix VCS/Gitoxide interaction and returns owned branch, change, history, and diff values.
- `features/vcs/` owns request generations, loading/error state, duplicate suppression, operation state, and stale-result rejection. History-detail completions match repository, generation, and commit identity; cached or cross-commit transitions invalidate older detail work. Its commit workflow owns a separate scratch `EditorSession` and the save-before-commit barrier; merge instead requires a clean status before opening or submitting.
- `page/vcs/mod.rs`, `page/vcs_history/mod.rs`, and `page/vcs_commit.rs` render feature state without performing repository work. `page/vcs/change_section.rs` owns section rows, selection, path formatting, and consumed height; `page/vcs_history/history_list.rs` and `commit_detail.rs` own their respective view models and selection-following viewports; `component/diff.rs` owns presentation `DiffState` and shares diff rendering between status and history without depending on the VCS feature.
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

The merge workflow uses a centered branch picker on the status page: `m` loads existing local target branches, typing filters the list, Up/Down selects, and Enter switches to the target before fast-forwarding that target to the source branch. Its private `Closed → Loading → Selecting → Merging` phase owns popup availability and input policy; `MergeState` also owns request-identity checks and completion transitions, so stale results are rejected without adapter-level revision/repository guards. Failures return to selection, and success closes the workflow. Git status and file-backed buffers must be clean before opening or submitting; pending changes simply prevent the operation. Errors remain in the popup, while success leaves HEAD on the target and closes it. Remote targets, hashes, tags, arbitrary revisions, and fetching are outside this slice.

## Mutation behavior

Real stage/unstage operations use filtered worktree-to-blob conversion and locked index writes directly through `gix`; production code does not require an external Git executable. Bulk operations validate all targets and use one final index write rather than partially applying a sequence of single-file writes.

Operations:

- Run in revisioned background effects.
- Allow only one stage/unstage mutation in flight; overlapping commands report `Git operation in progress` without starting more work.
- Stage/unstage refuse pending-save states; commit and merge save first.
- Preserve worktree contents when unstaging.
- Refresh authoritative Git status after every successful mutation instead of inferring index transitions in the UI. Older status reads are invalidated, and selection follows the same path when it remains present.
- Reject conflicts and unsupported file types explicitly.
- Preserve ready content during background status refreshes.

Explicit `Ctrl-s` submission saves modified file-backed buffers and waits for in-flight saves before committing. The backend holds the index lock, builds a tree from exactly the staged index, validates identity/message/staged changes and repository state, and updates HEAD through Gitoxide with expected-parent protection. Unstaged and untracked content is never added implicitly. Hooks and signing are outside this slice. Success clears the draft and refreshes HEAD, status, history, and diff bases without reloading unchanged worktree files.

Fast-forward merge refuses pending changes rather than saving them. The backend resolves only exact local target refs, requires a born symbolic HEAD and clean status, and verifies that the target can advance to the source commit through the merge base. It holds the index lock, prepares branch and symbolic-HEAD expected-value transactions before checkout, removes old-only tracked paths, checks out the target tree, and commits index/ref updates with rollback on failure. No merge commit, conflict resolution, fetch, or arbitrary revision evaluation is attempted. Every successful switch refreshes repository state and schedules revision-aware reloads for all open file-backed buffers without overwriting dirty buffers, including targets that were already up to date.

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

- Reviewed discard workflows.
- Conflict resolution, staging of unsupported entry types, and other advanced Git behavior.
