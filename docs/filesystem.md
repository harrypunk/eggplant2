# Filesystem work and the file picker

Eggplant owns filesystem policy. Helix provides document/editing primitives and document construction through `DocumentFactory`; it does not own picker ranking, explorer traversal, previews, or future rename/delete commands.

## Responsibility boundaries

| Layer | Responsibility |
| --- | --- |
| Command menu / input context | Turn key sequences into semantic messages such as `OpenFilePicker` or `OpenPickerFile`. |
| `FilePickerState` | Own query, cached file set, ranked results, selection, preview state, and background revisions. |
| `FilePickerRequest` | Describe blocking scan, filter, or preview work with owned inputs. |
| Effect runtime | Execute blocking work with `tokio::task::spawn_blocking` and send completion messages back to the single update loop. |
| `FilePicker` component | Render query/results and text preview inside the reusable `Popup`; perform no filesystem I/O. |
| `DocumentFactory` / `EditorSession` | Open the selected document and manage buffers, paths, dirty state, cursor, and saving. |
| Filesystem adapter | Share Git-ignore and built-in ignore policy; later host narrowly scoped mutation operations. |

No component reads a directory or file during rendering. No worker mutates `Model` directly.

## Picker data flow

```text
Space f f
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

`MessageManager::open_file` is the single application open path for explorer, picker, and future open commands. After Helix document construction succeeds, `EditorSession` supplies the canonical buffer path. The same update then asks `ExplorerState::reveal` to expand visible ancestors and select that exact file, even if the explorer is hidden. Reveal is transactional: outside-workspace, ignored, missing, and read-failure paths do not partially change expansion or selection. Only after reconciliation does the picker close and the editor receive focus.

## Shared read-only view rendering

`TextViewSnapshot` is the editor crate's common projection of a Helix `Document + View`: visible lines, first-line offset, total line count, and horizontal offset. `EditorSnapshot` wraps it with buffer/mode/dirty/cursor metadata for the real editor; previews use the smaller projection directly and do not fabricate editor metadata.

`preview_document` constructs an independent document/view through the same `DocumentFactory`, then drops those Helix values after snapshotting. Preview documents never join `EditorSession`, buffer tabs, auto-save, or close/quit workflows and expose no editing API. The picker renders successful previews with the same TUI `TextView` used by `EditorBuffer`, so line numbers, scrolling, and syntax highlighting, themes, and match annotations have one presentation path. Grep and find-replace previews can reuse this path with match annotations and a different initial viewport offset.

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

## Future filesystem mutations

Rename, move, and delete should extend the same message/effect/completion path. They should not be implemented as direct calls from explorer or picker components.

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
