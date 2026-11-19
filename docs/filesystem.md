# Filesystem work and the file picker

Eggplant owns filesystem policy. Helix provides document/editing primitives and document construction through `DocumentFactory`; it does not own picker ranking, explorer traversal, previews, or workspace mutation policy.

## Responsibility boundaries

| Layer | Responsibility |
| --- | --- |
| Command menu / input context | Turn key sequences into semantic messages such as `OpenFilePicker` or `OpenPickerFile`. |
| `FilePickerState` | Own query, cached file set, ranked results, selection, preview state, and background revisions. |
| `FilePickerRequest` | Describe blocking scan, filter, or preview work with owned inputs. |
| Effect runtime | Execute blocking work with `tokio::task::spawn_blocking` and send completion messages back to the single update loop. |
| `FilePicker` component | Render query/results and text preview inside the reusable `Popup`; perform no filesystem I/O. |
| `DocumentFactory` / `EditorSession` | Open the selected document and manage buffers, paths, dirty state, cursor, and saving. |
| Filesystem adapter | Share Git-ignore policy and own workspace-safe create/move/copy/delete primitives and canonical containment checks. |

No component reads a directory or file during rendering. No worker mutates `Model` directly.

## Picker data flow

```text
Space Space (or Space f f)
   │
   ▼
CommandAction::OpenFilePicker → Message::OpenFilePicker
   │
   ▼
update: open picker state
   │ returns Effect::FilePicker(Scan { root, depth, revision })
   ▼
spawn_blocking: Git-aware top-level scan (recursive on first search)
   │ sends FileScanFinished { revision, result }
   ▼
update: retain Arc<Vec<FileEntry>>
   │ returns Filter { files, latest query, revision }
   ▼
spawn_blocking: Nucleo fuzzy matching and ranking
   │ sends FileFilterFinished { revision, results }
   ▼
update: select first result
   │ returns Preview { selected path, preview revision }
   ▼
spawn_blocking: independent Document + View snapshot
   │ sends FilePreviewFinished { revision, preview }
   ▼
update → render
```

Startup seeds the top-level picker cache from the already-loaded explorer listing, so the first open renders immediately without another read. A ready entry cache is retained when the picker closes, so reopening also renders cached results immediately. If no ready cache exists, the picker falls back to a top-level scan. The first non-empty query upgrades a ready top-level cache with one recursive background scan; subsequent typing filters the retained `Arc<Vec<FileEntry>>` without further traversal. Typing while either scan is running does not invalidate it, and completion filters with the latest query.

Filter and preview revisions are independent. A late filter completion cannot overwrite a newer query, and a late preview cannot replace the preview for a newer selection. Closing the picker invalidates both revisions.

## Open acceptance and explorer synchronization

Every application document open uses `DocumentFactory::open`. Existing targets must be regular, NUL-free UTF-8 text files. Directories, links, binary data, invalid UTF-8, and other file types return `not a text file` without changing buffers, picker state, or explorer state. A missing path remains valid for deliberate new-file startup behavior.

External watcher events also invalidate picker caches through `FilePickerState`: content changes evict that path's preview and reload it when selected, while structure changes clear the entry/preview caches and rescan a visible picker. Hidden pickers rebuild stale entry state on their next open.

`DocumentOpening` assigns a generated `OpenRequestId` to each pending path/position. The TUI request and completion envelopes carry that identity through the existing editor open request. Completion requires both the latest ID and matching path, so an older same-path result cannot consume a newer position or clear the newer pending request.

Pending opens also retain their originating input context. File-picker opens retain a session generation that advances on every open. After document insertion, picker closing and editor focus run only when that origin/context is still current; an older explorer completion cannot displace a new picker, and an older picker completion cannot close a reopened picker session. Shared explorer reveal and VCS synchronization still apply.

`document::open_file` is the shared application open capability for explorer, picker, and future open commands; `document::open_file_at` additionally accepts an optional `DocumentPosition`. They return an `OpenDocument` effect instead of reading and parsing on the UI thread. The worker performs one UTF-8 file read and prepares `Rope`, language configuration, and optional `Syntax`; the main update then constructs Helix's non-`Send` `Document` through `DocumentFactory::build_document`. `document::finish_open_file` validates request identity, inserts the document, reports failures, and returns an optional explorer reveal effect with the opened-file metadata. The shell composes reveal and VCS effects while retaining responsibility for picker close and editor focus.

After insertion, `EditorSession` supplies the canonical buffer path. The same update creates an `ExplorerState::reveal_request` with an owned expansion snapshot, even if the explorer is hidden. Background traversal expands the visible ancestors; the latest successful completion selects that exact file. Next/previous buffer commands reuse this reconciliation for the newly active file, while untitled buffers leave the explorer unchanged. Reveal is transactional: outside-workspace paths do not create a request, and ignored, missing, stale, or read-failure results do not partially change entries, expansion, or selection. Picker closing and editor focus are reconciled independently after document insertion rather than waiting for filesystem traversal.

## Shared read-only view rendering

`TextViewSnapshot` is the editor crate's common projection of a Helix `Document + View`: visible lines, first-line offset, total line count, and horizontal offset. `EditorSnapshot` wraps it with buffer/mode/dirty/cursor metadata for the real editor; previews use the smaller projection directly and do not fabricate editor metadata.

`preview_document` constructs an independent document/view through the same `DocumentFactory`, then drops those Helix values after snapshotting. Preview documents never join `EditorSession`, buffer tabs, auto-save, or close/quit workflows and expose no editing API. The picker renders successful previews with the same TUI `TextView` used by `EditorBuffer`, so line numbers, scrolling, and syntax highlighting, themes, and match annotations have one presentation path. Grep and find-replace previews can reuse this path with match annotations and a different initial viewport offset.

## Project live grep

`Space s p` opens `project_grep`, which searches fixed-workspace files on disk rather than the in-memory snapshots used by current-buffer grep. `TextSearcher` compiles each smart-case regex once, `workspace_walk` applies the shared Git/built-in ignore policy, and `read_text_file` rejects links, non-regular files, NUL-containing files, and invalid UTF-8 without making them search results. Search and preview work run on the blocking pool with independent revisions and typed `FeatureCompletion` routing.

Project results retain relative path, zero-based line, text, and match ranges. Only selection changes trigger `preview_document_at`, which provides match-centered syntax highlighting. Acceptance closes the extension and calls `document::open_file_at`; the normal safe insertion/reveal flow completes before cursor movement, with no project-specific completion branch in the shell.

## External buffer synchronization

A `notify` watcher covers the fixed workspace, or the initial file's parent when no workspace is open. Create/modify/remove events become `ExternalFileChanged` messages. The explorer consumes only structural create/delete/rename events and schedules a generation-checked background reload without clearing unrelated status or losing a still-present selected path. Only the latest completion can replace the visible tree. Content-only changes do not rescan the tree. Dirty buffers and in-progress saves remain untouched, while clean buffers schedule the same background `DocumentOpenRequest` used by normal opens. Missing paths never erase an open buffer.

The reload completion carries the expected Helix document version. Stale completions are ignored. Existing files apply a Helix diff, append history, preserve cursor/view state, update syntax, and become clean without invoking another save. Missing files and read/UTF-8 failures leave the existing buffer intact. No manual reload command or reload queue is introduced.

## Fuzzy matching and ranking

Use `nucleo-matcher` rather than implementing a fuzzy algorithm in Eggplant. It is maintained for and used by the Helix ecosystem, supports Unicode normalization and path-oriented matching, and reuses matcher scratch allocations.

The worker parses the query with smart case/normalization, scores each relative display path with path matching, sorts scores descending, and uses the display path as a deterministic tie-breaker. The matcher implementation stays in `file_picker::work`, so a future ranking change does not affect widget or command code.

Ranking policy should be adjusted through tests and observed UX, not by leaking scores into central application state. Match-index highlighting can be added later using the matcher's slower indices API only for visible results.

## Large-repository behavior

The first implementation uses established crates and explicit effects instead of a custom async filesystem or fuzzy engine:

- `ignore` prunes Git-ignored and built-in ignored directories during traversal.
- Scanning, matching, and preview reads run on Tokio's blocking pool, not the terminal thread.
- Top-level entries provide a fast initial response; recursive traversal is deferred until the first search.
- Ready file entries persist across picker closes and are shared with filter jobs through `Arc`.
- Preview documents are opened on the blocking pool through the shared `DocumentFactory`; their Helix views project only visible rows. Binary and non-UTF-8 files show an unavailable message.
- Successful preview snapshots are cached by path. Cache hits render synchronously, and uncached loads retain the previous successful preview instead of flashing a loading message.
- Only selection changes trigger preview reads.

Revision checks guarantee stale results are harmless, but they do not stop obsolete CPU or I/O work. If profiling on large repositories shows excessive queued work, introduce a dedicated picker worker that coalesces queued queries and supports cancellation. Do not add a complex scheduler before that need is measured. A future watcher or explicit refresh can keep the file-entry and preview caches current. A future recent-files command such as `Space f r` can reuse the same entries with recency metadata and ordering instead of introducing another scanner.

## Explorer add

Explorer `a` opens a revisioned path prompt. The initial path is the selected directory, or the selected file's parent, relative to the fixed workspace root. A trailing `/` creates a directory; other paths create an empty regular file with create-if-absent semantics. Existing targets are never truncated, and missing parent directories are created one component at a time within the workspace.

Absolute paths and parent traversal are rejected before work starts. The blocking worker canonicalizes every existing or newly created parent component and rejects parents outside the canonical workspace, including symlink escapes. Successful files open through `document::open_file`; successful directories use `reveal_request`, which expands ancestors and selects the new directory without a separate refresh. Failures retain the entered path for correction, and stale completions cannot close or change a newer prompt.

## Explorer delete

Explorer `d` opens a confirmation popup for the selected workspace entry. `y` confirms and Escape cancels without filesystem changes. The blocking filesystem capability verifies the canonical parent, rejects workspace escapes, and rechecks the target type before deletion. Files and links use `remove_file`; directories use recursive `remove_dir_all`. Links are removed themselves rather than followed.

Affected modified or saving buffers reject confirmation without deleting anything. After successful deletion, the application atomically closes affected clean buffers, including buffers below a deleted directory, and refreshes Explorer. Failures retain the confirmation and use notifications; revision checks ignore stale completions.

Raw create/delete operations and workspace path enforcement live in `capabilities/filesystem`, while Explorer owns prompts, operation state, and application coordination.

## Explorer move and copy

Explorer `m` and `c` share a destination prompt prefilled with the selected workspace-relative source path. Enter starts a revisioned transfer and Escape cancels while editing. The filesystem capability rejects absolute paths, traversal, existing destinations, directory self-transfers, source type changes, and canonical workspace escapes. Missing destination parents are created safely. Links are moved or recreated themselves rather than followed.

Move uses `rename` and applies the modified/saving source-buffer rejection and clean close-after-success policy. Copy uses file copying or recursive directory copying, preserves the source, and leaves all source buffers open, including modified buffers. Both reveal the destination. Errors notify and retain the edited destination.

## Future filesystem mutations

Future filesystem mutations should extend the same message/effect/completion path. They should not be implemented as direct calls from explorer or picker components.

```text
Command/menu selection
        │
        ▼
semantic operation message + stable target identity/path
        │
        ▼
update: validate intent, save/reconcile dirty buffers, create request
        │
        ▼
filesystem effect outside update
        │
        ▼
completion message with operation ID and result
        │
        ▼
update EditorSession + explorer/picker state, or report an error
```

Recommended rules for future mutations:

1. **Use explicit operation types.** A closed enum such as `Rename`, `Move`, or `Delete` is sufficient. Avoid a generic stringly typed command bus.
2. **Keep operations workspace-aware.** Validate source and destination against the fixed workspace root and define overwrite, symlink, and cross-device behavior before implementation.
3. **Coordinate open buffers.** Use stable `BufferId`s for application identity. Save dirty affected buffers before mutation when required, and never silently discard edits.
4. **Change model paths only after success.** Rename/move completion must reconcile affected open documents through a narrow `EditorSession` API and preserve canonical-path deduplication. TUI code must not manipulate Helix `Document` paths directly.
5. **Delete conservatively.** Define confirmation and open-buffer behavior explicitly. A failed delete leaves the model and visible selection unchanged.
6. **Refresh projections after success.** Reload the explorer and invalidate/rescan picker results as needed; do not assume every internal projection has already changed.
7. **Report actionable errors.** Completion errors keep safe state and use the shared status model.
8. **Ignore stale completions.** Operation IDs or revisions prevent an old worker result from mutating a newer workspace/document state.

Helix involvement stops at the editor adapter: document construction, supported path/document updates, text, history, and saving. Workspace authorization, user confirmation, picker behavior, and operation orchestration remain Eggplant responsibilities.
