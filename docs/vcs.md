# VCS addon

The VCS integration is an **optional user-experience addon**, not a core editor feature. Documents, buffers, editing, and automatic persistence remain fully usable without Git or the VCS page.

For keybindings, see [Key input, sequences, and hints](key-input.md).

## Scope

The addon provides:

- Branch or abbreviated detached-HEAD display in the editor status bar.
- A full-screen status page with dynamic staged and unstaged sections.
- Stage/unstage for regular files and deletions, including bulk operations.
- Cached inline diffs for changed files and commits.
- Safe file opening through the shared document capability.
- Explicit commits of already-staged changes with a multiline message editor.
- Bounded commit history with commit details and hash copying.
- Plain local-branch switching and creation with compatible local changes preserved.
- Switch-and-fast-forward merges to local target branches without fetching or rewriting history.

Discard remains a placeholder.

## Ownership and dependency direction

| Layer | Responsibility |
| --- | --- |
| `crates/vcs` | Helix VCS/Gitoxide interaction and owned branch, change, history, and diff values. |
| `features/vcs/` | Request identity, loading/error state, operation state, and stale-result rejection. |
| `page/vcs*/` | Presentation and view-model derivation without repository work. |
| Application shell | Page navigation, save continuation, and feature composition. |

### Feature state and workflows

- `ChangesController` owns changed files, selection, refresh/trailing state, index operation identity, and working-tree diff state.
- `RepositoryState` owns HEAD refresh identity.
- `HistoryController` owns history list/detail state, selection, caching, and refresh/pagination completion.
- `BranchState` owns branch-menu, start-point, naming, and switching phases.
- Active-buffer diff-base identity has a separate state object.
- `history.rs`, `change_workflow.rs`, `repository.rs`, and `branch.rs` contain adapters and request/completion implementations instead of growing `mod.rs`.

History-detail completions match repository, generation, and commit identity. Cached or cross-commit transitions invalidate older detail work.

The branch backend exposes local listing, existing-branch switching, and typed `BranchStartPoint` creation separately from merge. A future history action can reuse checkout without depending on popup state.

The commit workflow owns a separate scratch `EditorSession` and its save-before-commit barrier. Merge has a separate clean-status policy.

The application coordinator routes each save outcome to either the commit barrier or generic close/quit continuation. `shell/lifecycle` has no VCS dependency.

### Presentation boundaries

- `page/vcs/change_section.rs` owns section rows, selection, path formatting, and consumed height.
- `page/vcs_history/history_list.rs` and `commit_detail.rs` own their view models and selection-following viewports.
- `component/diff.rs` owns presentation `DiffState` and shares diff rendering without depending on the VCS feature.
- `AppPage` and `Context::VCS` provide shell navigation; closing a page restores the previous context.
- No Gitoxide types enter application snapshots or Ratatui components.

Reduced-trust repository discovery and neutral non-repository/unborn states keep failures non-blocking.

## Status page

The status page has three regions:

1. Common key hints and optional notices.
2. A one-line HEAD reference with the latest short commit ID and subject.
3. Content-driven staged and unstaged sections, hidden when empty.

Selection follows one staged-first order. Inline diffs appear directly beneath their expanded file. `?` toggles a bordered bottom help panel; Escape dismisses only the panel, while `q` closes the page.

Changed files use the shared safe document-opening and explorer-reveal path. Deleted files are not recreated.

## History page

### Listing and pagination

- Rows show short IDs, subjects, authors, relative times, and exact local/remote branch-tip labels.
- The status-page HEAD reference and history rows share one commit-message component.
- Toggling a commit lists its changed files inline.
- History loads 20 commits per page.
- The header shows `n load more` with loading, retry, and error feedback.

### Commit details

- Enter opens a right-side detail with full hash, complete message, author, full ISO date-time with offset, and changed files.
- `HistoryPane::{List, Detail}` keeps list and detail commands independent.
- List commands cannot run behind a visible detail, including while detail data is loading or failed.
- Each detail file can toggle its first-parent diff with shared diff colors and caching.
- Enter on a detail file row closes history and opens the working-tree file through the safe document flow; deleted entries are rejected.
- Merge commits intentionally use the first parent in this slice.
- Tree changes are filtered to regular blobs; multi-file commits retain every changed path.

### Copying commit hashes

- In a ready detail, `c` copies the seven-character short hash.
- `C` copies the full hash.
- The reusable `CopyHash` capability performs copying without depending on history state.
- Success or clipboard failure appears in a transient `Git` notification.
- Copying does not change the editor's unnamed yank register.

## Commit page

The commit page shows a read-only staged-file summary above a multiline scratch editor. It supports ordinary navigation, Insert editing, and undo/redo without file-backed auto-save.

- `q` closes the page and preserves the in-memory draft.
- `Ctrl-c` explicitly discards the draft.
- Submission locks editing until completion.
- Failure preserves the message; success clears it.
- `CommitAttempt` captures repository, message, and revision throughout its Saving and Committing phases.

Save-dependent workflows are single-owner and fail-fast. An active commit rejects close/quit until it finishes, while pending close/quit rejects commit submission without changing the draft or deferred action.

## Merge workflow

The merge workflow uses a centered branch picker on the status page:

- `m` loads existing local target branches.
- Typing filters the list; Up/Down selects; Enter submits.
- The target is checked out before fast-forwarding it to the source branch.
- Errors remain in the popup; success closes it and leaves HEAD on the target.

`MergeState` owns its `Closed → Loading → Selecting → Merging` phases, request identity, and completion transitions. Stale results are rejected inside the controller.

Git status and all editor buffers must be clean before opening or submitting. Pending changes prevent the operation rather than triggering an automatic save.

Remote targets, hashes, tags, arbitrary revisions, and fetching are outside this slice.

## Branch switching and creation

### Interaction

- `b` opens a bordered bottom Branch action bar.
- `s` opens a filtered existing-local-branch selector.
- `c` opens a start-point selector with HEAD first, followed by all local branches.
- A short or full hexadecimal commit ID can be entered as the start point instead.
- Create-and-switch advances to a separate branch-name field before repository work starts.
- The bottom bar and popup use separate input contexts, so `c` and `s` do not overload page-local commit/stage input.

Cancellation is allowed before mutation but not while checkout is active. Failures remain in the current selector/name phase, and stale target or mutation completions are ignored.

Plain branch switching has no automatic-save or clean-status gate. Its checkout safeguards are distinct from merge policy.

## Mutation behavior

### Shared rules

- Repository work runs in revisioned background effects.
- Production code uses Gitoxide directly and does not require an external Git executable.
- Conflicts and unsupported file types are rejected explicitly.
- Successful mutations refresh authoritative Git status instead of inferring index transitions in the UI.
- Older status reads are invalidated, and selection follows the same path when it remains present.
- Ready content remains visible during background status refreshes.

### Stage and unstage

- Stage/unstage uses filtered worktree-to-blob conversion and locked index writes.
- Only one stage/unstage mutation may be in flight; overlaps report `Git operation in progress`.
- Pending-save states are rejected.
- Bulk operations validate all targets and use one final index write.
- Unstaging preserves worktree contents.

### Commit

`Ctrl-s` saves modified file-backed buffers and waits for in-flight saves before committing.

The backend holds the index lock, builds a tree from exactly the staged index, and validates identity, message, staged changes, and repository state. HEAD is updated with expected-parent protection.

Unstaged and untracked content is never added implicitly. Hooks and signing are outside this slice. Success clears the draft and refreshes HEAD, status, history, and diff bases without reloading unchanged worktree files.

### Plain branch switching

- Existing targets resolve to exact local branches; neither branch tip moves.
- New branches can start from HEAD, an exact local branch, or a revision resolved to a commit.
- In-progress repository state, unborn HEAD, conflicts, sparse entries, submodules, and invalid/duplicate names are rejected.
- A locally changed path must have identical source and target tree entries; otherwise checkout fails before mutation.
- Compatible local index/worktree entries are retained and temporarily skipped during checkout.
- Clean added, modified, and deleted paths update normally.
- Unsafe untracked or ignored collisions are rejected.
- Index and symbolic-HEAD/new-ref updates commit only after checkout, with rollback on failure.

Successful switches and creations share repository, explorer, and open-buffer reconciliation with merge.

### Fast-forward merge

Merge refuses pending changes rather than saving them. The backend:

- Resolves only exact local target refs.
- Requires a born symbolic HEAD and clean status.
- Verifies fast-forward compatibility through the merge base.
- Holds the index lock and prepares expected-value branch/HEAD transactions before checkout.
- Removes old-only tracked paths and checks out the target tree.
- Commits index/ref updates with rollback on failure.

No merge commit, conflict resolution, fetch, or arbitrary revision evaluation is attempted.

After every successful switch, including an already-up-to-date target:

- Repository state is refreshed.
- All open file-backed buffers receive revision-aware reloads.
- Dirty buffers are preserved.

## Changed-file and commit diffs

Diffs are bounded unified output generated from HEAD, index, worktree, or commit blobs:

- Added lines use `diff.plus`.
- Removed lines use `diff.minus`.
- Hunk headers use `diff.delta`.
- Context uses normal base text rather than uniformly dimmed output.

Expanded diffs render every fetched line. An ellipsis appears only when the backend's 200-line safety limit is reached.

Completed diffs are cached by path and staged state or commit identity. Closing and reopening a toggle does not recompute them. Status refreshes and mutations invalidate the relevant cache.

Selected file rows use a bold `>` marker. Empty and loading states apply the explicit dark base background.

## Active-buffer diff gutter

### Rendering

The addon loads the active file's HEAD content in a background effect. `EditorSession::set_buffer_diff_base` applies the result and rejects stale document revisions.

Helix `DiffHandle` tracks subsequent in-memory edits automatically. Editor snapshots expose only `Added`, `Modified`, and `Deleted` line markers.

The gutter displays `│` for every change, using the theme's addition, hunk, or removal colors without changing its width. Untracked files use an empty base; non-repositories remain unmarked.

### Refresh and invalidation

- Loaded bases are cached per buffer.
- Switching or opening buffers requests the active base.
- External reloads invalidate the affected base.
- Worktree events refresh status and reload eligible buffers.
- Index events refresh status without unnecessary history work.
- HEAD/ref/log events refresh repository state and invalidate cached bases.
- Ordinary saves mark status for refresh.

External/save status requests coalesce as one in-flight read plus one trailing dirty read, so events during I/O are not forgotten. Per-request generations prevent older responses from replacing newer state. No external Git process is used.

## Deferred work

- Reviewed discard workflows.
- Conflict resolution, staging of unsupported entry types, and other advanced Git behavior.
