# Editor design with Helix and Elm

Status: implemented first-edition design. Later extension and automation work is explicitly marked as future rather than changing these invariants.

## 1. Goals and constraints

The first editing implementation should support:

- Opening a folder without implicitly opening a file.
- Multiple open file buffers with stable identity.
- Switching, editing, saving, and closing buffers.
- Always-on auto-save after edits and before close/quit, without a manual Save command.
- One visible editor view with predictable cursor and scrolling behavior.
- A small Vim-inspired Normal/Insert command set.
- Ratatui rendering without adopting Helix's terminal application.

The design should optimize for correctness and incremental growth, not for future feature count. Split windows, LSP, and agent-driven edits are not part of this iteration.

### Design principles

1. **Make empty states valid.** A workspace with no open document is normal, not an error.
2. **Use stable identities.** Async completions and future AI operations must not depend on vector positions.
3. **Separate document state from view state.** Text/undo history and visible-area state have different lifetimes.
4. **Keep one source of truth.** Do not mirror document text, dirty state, cursor, or scroll offsets into widgets.
5. **Keep side effects outside state transitions.** Filesystem work should cross an explicit effect/message boundary.
6. **Choose Helix APIs by measured integration cost.** Reuse useful behavior without adopting unrelated editor policy.

## 2. Domain model

| Concept | Responsibility |
| --- | --- |
| **Workspace** | Opened folder, explorer state, and workspace-level actions. It does not require a document. |
| **Buffer** | An open document: stable ID, text, path, encoding, dirty state, and editing history. |
| **View** | State used to display a buffer: target buffer, dimensions, cursor/selection context, and scroll state. |
| **EditorSession** | Owns buffers, the optional visible view, edit mode, and editor commands. |
| **BufferInfo** | Derived display data such as ID, display name, path, and dirty state. It is not the buffer store. |

A buffer can conceptually be displayed by more than one view. The first edition does not need a split-tree implementation, but it should avoid making document text subordinate to a Ratatui widget.

`DocumentFactory` owns Helix configuration/loaders and document construction outside `EditorSession`. The session accepts constructed documents through `insert_document` and manages their buffer/view state. This keeps runtime resources and direct file opening out of the central state object.

## 3. Logical state and ownership

A useful logical model is:

```rust
// Illustrates relationships, not final Rust types.
struct EditorSession {
    buffers: BufferStore,
    active_view: Option<EditorView>,
    mode: EditMode,
}

struct EditorView {
    buffer: BufferId,
    viewport: ViewportAdapter,
}

struct Buffer {
    id: BufferId,
    document: DocumentAdapter,
}
```

The active buffer is derived through `active_view.buffer`; it does not need a second `active_buffer` field. `active_view = None` explicitly represents an editor with no visible document.

This is deliberately smaller than a general window tree. A future split implementation can replace the single optional view with a view collection without changing buffer identity or document ownership.

### Required invariants

- If `active_view` exists, its `BufferId` exists in `BufferStore`.
- Every open buffer has a stable ID independent of display order.
- Opening an already-open canonical path activates the existing buffer rather than duplicating it. All open commands use the same text-only `DocumentFactory` acceptance path, then reveal the canonical opened path in the explorer by expanding its visible ancestors and selecting it. Buffer switching performs the same reveal for the newly active file; untitled buffers leave explorer selection unchanged.
- Only the document adapter owns text, transactions, and history.
- Only one component owns viewport offsets for the active view.
- Editor commands never create a buffer implicitly when no document is active.

### Buffer storage is an implementation choice

`Vec<BufferInfo>` is one possible presentation structure, but it is not sufficient as the document model because it omits editing state and makes positional identity tempting. Possible authoritative stores include:

| Store | Advantages | Costs / appropriate use |
| --- | --- | --- |
| `Vec<Buffer>` | Simple and adequate for a small number of buffers. | Requires scanning; IDs must remain separate from indices. |
| `BTreeMap<BufferId, Buffer>` | Simple ID lookup and deterministic iteration. | Ordering is by ID unless a separate order is maintained. |
| `HashMap<BufferId, Buffer>` | Straightforward lookup with no ordering requirement. | UI/MRU order needs separate data if required. |
| Generational map such as `SlotMap` | Efficient stable keys and stale-ID protection. | Extra dependency and complexity; adopt only if justified. |

Start with the simplest map-based implementation that satisfies the invariants. `Vec<BufferInfo>` can be derived for a picker or tab line. If MRU order becomes a feature, store ordered `BufferId`s or timestamps rather than treating `BufferInfo` as the source of truth.

### Session and active-editor responsibilities

`EditorSession` owns buffer identity/lifecycle, editor mode, session-level snapshots, and save/external-reload coordination. It does not implement every motion or text operation. `active_editor()` returns an `ActiveEditor` adapter around the active Helix `Document + View` plus the session scrolloff policy. Navigation, viewport, and editing operations live in focused `active_editor/` modules and retain cursor-following invariants without exposing Helix types to the TUI.

Adding a motion or editing feature should normally change only its feature module and the appropriate `ActiveEditor` capability. A session change is justified for buffer lifecycle, persistence, or session-level state—not for adding key-specific behavior. `session/tests.rs` owns the remaining lifecycle and capability integration tests.

### Preserving cursor and scroll when switching

Switching buffers should not reset the user's position. Where that state lives depends on the selected adapter:

- Helix `Document` already stores selection and `ViewPosition` per `ViewId`.
- A core-only Eggplant adapter can retain a small last-view-state record per buffer.

Do not duplicate both mechanisms. The spike should establish which adapter owns this behavior.

## 4. Folder-open and empty-editor behavior

Workspace and editor state are independent. The workspace root is fixed after startup. The explorer presents a visible filesystem tree beneath that root; folding or unfolding directories changes only the tree's expansion state, never the workspace root. Root and nested `.gitignore` rules plus built-in `.git`, `node_modules`, and `__pycache__` entries are filtered by default. The explorer ignore-visibility command toggles between hiding and showing all ignored entries. Ignore filtering happens before producing the flattened tree; unrelated dotfiles remain visible.

```rust
struct Model {
    workspace: Option<Workspace>,
    explorer: ExplorerState,
    editor: EditorSession,
    focus: FocusTarget,
    // Other application state...
}
```

When starting Eggplant with a folder, or opening one in a fresh session:

1. Set the workspace root.
2. Load or refresh explorer entries.
3. Focus the explorer if that is the intended startup workflow.
4. Initialize the editor with no active view and no buffers.

Opening a directory must not create a document, automatically open the first file, or create a hidden scratch buffer. These are separate actions:

| Action | Result |
| --- | --- |
| Open folder | Establish workspace/explorer state. |
| Open file | Create or activate a file-backed buffer and its view. |
| New file | Deliberately create an unsaved buffer with no path. |

With no active view, render an empty/welcome area, do not place the editor text cursor, and keep explorer/global commands available. Editing commands should be ignored or return a clear status message; they must not panic or implicitly create a file.

Closing the last buffer returns to this same valid state.

## 5. Elm architecture integration

The editor fits inside the existing application loop; it does not run a second loop:

```text
Crossterm event
      │
      ▼
focused input router / keymap
      │
      ▼
Application or Editor Message
      │
      ▼
update(&mut Model, Message) -> Effects
      │
      ├── pure state transition, including transactions and viewport updates
      └── effect executor performs file/background work
                    │
                    ▼
          completion Message with stable ID
                    │
                    ▼
              same update loop
      │
      ▼
Ratatui view(&Model)
```

The exact return type can remain small: a single `Option<Effect>` is sufficient until an update needs multiple effects, and existing follow-up `Message` chaining can remain separate. The architectural boundary matters more than introducing a generic effects framework.

### Message and effect boundaries

| Input/message | State transition | Possible effect |
| --- | --- | --- |
| `OpenPath(path)` | Route directory and file paths differently; record pending intent if needed. | Read a file or directory. |
| `FileOpened { path, result }` | Insert a buffer and create/retarget the view on success; report failure without corrupting state. | None. |
| `ActivateBuffer(id)` | Retarget the active view and restore its document/view state. | Schedule or start saving the previously active dirty buffer. |
| `Editor(Move/Insert/Delete/Undo/Redo)` | Update only the active document through editor commands. | Auto-save after Normal-mode changes or when leaving Insert. |
| `SaveBuffer(id)` | Capture the target buffer ID and revision. | Save a consistent document snapshot. |
| `SaveFinished { id, revision, result }` | Update saved state only for the completed revision; preserve newer dirty edits and report errors. | Save again if newer edits arrived while saving. |
| `CloseBuffer(id)` | Save first when needed, then select another view or the empty state. | Save effect; failure cancels closure. |
| `Resize(terminal_size)` | Recalculate layout/view dimensions and keep the cursor visible. | None. |

### Keymap integration

Normal-mode bindings resolve to semantic commands. In Insert mode, unbound printable input becomes an editor insert command; individual characters should not be configured as keymap actions.

Directional focus commands use logical component relationships and panel visibility, not document cursor coordinates or rendered rectangles. The single-view layout only moves between the editor and explorer horizontally; vertical directions remain no-ops until vertical panes exist. Focus helpers own target availability and input-context transitions. Current bindings are centralized in [Key input, sequences, and hints](key-input.md).

Built-in common commands use a Space leader and bottom-right key-hints popup, not the configurable direct-key map. Leader actions feed back into the normal message/update path; they do not have separate command implementations.

All mutations go through the editor command API so undo, dirty tracking, viewport updates, and future AI-generated transactions share the same path.

### Word motion semantics

Word motions are cursor-only operations: `ActiveEditor::move_cursor_word` uses Helix's Unicode-aware word primitives, then collapses the result to a cursor point instead of exposing Helix's selection-oriented motion. Next-word-end uses the resulting block-cursor edge so it lands on the final character. Helix's boundary policy, including line-ending boundaries, is retained rather than reimplemented as Vim compatibility. These motions never modify text.

Only the counted first/last-line commands accept counts; word motions and all other motions are uncounted. Operators, inner/around objects, and a broader Vim/Helix grammar remain outside the word-motion slice. Visual selection is the intended workflow for larger ranges instead of adding more text-object motions. Visual mode reuses these motions with explicit move/extend behavior. Eggplant exposes exactly one cursor/selection and intentionally does not support user-facing multi-cursor behavior. Current bindings are centralized in [Key input, sequences, and hints](key-input.md).

### Counted line jumps and pending input (M8.1)

Uncounted first-line and last-line commands move to the corresponding document boundary. A count beginning with `1`–`9` and continuing with digits selects that one-based line for either command; out-of-range values clamp to the document boundary. Only these commands accept counts. `ActiveEditor::move_cursor_to_line` targets the first non-blank character, falling back to column zero, and uses the existing viewport-following path.

`line_jump` owns the count/prefix state machine through registered text input and `line_jump_cancel`. The status bar renders its derived `Model.pending_input` separately from persistent status messages, so pending counts/prefixes do not replace save or error feedback. Completion, Escape, and invalid input clear the pending state without unexpected movement.

### Viewport alignment (M8.2)

Viewport-alignment commands center, top-align, or bottom-align the cursor line. Starting alignment opens a pending `ViewAlign` context and publishes the prefix through `Model.pending_input`; registered text input accepts only the three completion keys. Escape and invalid input clear pending state without moving the viewport.

`ActiveEditor::align_cursor_line` maps the feature-neutral `ViewAlignment` enum to `helix_view::align_view`. Helix performs visual-row and document-boundary calculations. Cursor, selection, text, revision, dirty state, and save state remain unchanged, including for empty viewports.

### Insert variants and open-line editing (M8.3)

Insert variants position the cursor after the current grapheme, at line end, or at the first non-blank character before entering Insert. `ActiveEditor::move_cursor_to_insert` owns these document positions; the editing feature owns mode/context transitions.

Open-line commands use `ActiveEditor::open_line` to insert a newline below/above with `indent_for_newline`, set the new cursor, and enter Insert. Helix's current indentation heuristic and detected indent style are retained, without comment continuation or multi-selection commands. The open-line transaction and subsequent typing share one Insert history group, so one undo removes the whole accepted edit. Auto-save follows the existing policy when leaving Insert; no manual Save path is added.

### Line-boundary motions (M8.4)

Line-start and line-end commands use `move_line_start`/`move_line_end` and `ActiveEditor::move_cursor_to_line_boundary`. Line start targets column zero. Line end targets the final grapheme before the line ending, with empty lines safely at column zero. These are cursor-only motions with normal viewport following and no document/revision/save changes.

Direct line-start binding takes precedence over unbound text input. An active line-jump count has no line-start binding, so the zero digit continues the count as intended. Visual mode reuses the same core boundary operation with selection extension.

### Characterwise Visual selection (M8.5)

Characterwise Visual mode starts with a selection at the current grapheme. `ActiveEditor` receives `SelectionBehavior::Move` in Normal/Insert and `Extend` in Visual, so existing directional, word, line-boundary, and counted line-jump commands share one motion path. Helix's `put_cursor` controls forward/backward selection edges and reversal; no second Visual navigation implementation exists.

Linewise Visual mode starts with the full current line, including an existing line ending. `SelectionBehavior::ExtendLines` maps every moving target back to whole-line ranges while preserving the original anchor line, including direction reversal. The status bar distinguishes characterwise and linewise modes. Existing Visual delete/change/yank commands operate on those line selections without a separate operator implementation.

`EditorSession::enter_normal_mode` collapses either Visual variant at the moving cursor without changing text. `VISUAL`/`V-LINE` appear in the status bar. Active snapshots project only the primary selection into per-line UTF-8 ranges, excluding non-printable line-ending cells, and `TextView` applies the existing selection style after syntax rendering with Unicode/horizontal clipping. Eggplant still exposes exactly one cursor/selection and no multi-cursor behavior.

### Delete/change commands (M8.6)

Delete and change commands cover the grapheme under the cursor, the current line, the current Visual selection, and the cursor-to-next-word-start range. Current-line change preserves the established indentation policy, while word-range capture and deletion reuse Helix's next-word-start primitive. No general operator-motion grammar is added; current bindings are centralized in [Key input, sequences, and hints](key-input.md).

These operations are implemented once and composed through `editing::EditingTarget`. `MainEditingTarget` and `CommitEditingTarget` provide the active `EditorSession`, Normal/Insert contexts, register policy, mutation permission, and after-edit behavior. The main target adds auto-save and language synchronization; the commit target edits the scratch session without file persistence. `features/editing`, `features/change`, and `features/yank` all use this adapter, so a page needs semantic key bindings and an adapter—not copied editing logic. Default Normal and Commit keymaps extend the same `SHARED_EDITING_BINDINGS` list, keeping common input composition aligned as well.

Pending delete/change prefixes use the shared transient pending-input display. Every accepted command uses the existing undo/redo, dirty-revision, viewport, and auto-save paths. Normal deletes commit history immediately; Visual and line/word change commands commit when their Insert session ends, preserving one-step undo. Clipboard/register synchronization is centralized separately, and all commands operate on the single cursor/selection.

### Yank and clipboard synchronization (M8.7)

Selection yanking returns to Normal mode, while current-line yanking preserves line-ending semantics and uses the shared transient pending input. One automatic unnamed register stores yanks and accepted delete/change operations, so users never select a register or run a clipboard command.

`RegisterState` owns the unnamed register and a cloned Helix `ClipboardProvider`. Clipboard writes run in a blocking feature effect, because Helix's providers may wait on external processes. An unavailable provider leaves the internal register intact and reports a concise non-blocking status. `Effect::combine` lets delete commands run register/clipboard work alongside auto-save; change commands retain save-on-Insert-exit. Yanks never change text, revision, or dirty state and do not trigger auto-save. Paste commands, named registers, and register UIs remain separate work.

### Update and rendering responsibilities

Movement, editing, activation, and resize handling update viewport state. Rendering only reads the result and draws visible content. The one-row status bar fills the page width and uses `Flex::Start`: fixed-width edit mode and bounded-width optional-addon segments, a growing project-relative active-file path (or standalone path) with status messages, a fixed-width pending-key segment immediately before a right-aligned local `HH:MM` clock. Editor buffers and independent previews share one viewport policy: Eggplant owns the gutter and statusline, while Helix receives exactly the resolved text dimensions plus its internally required statusline compensation. Cursor placement is suppressed for empty or out-of-bounds text rectangles. After drawing, the runtime applies a steady bar in Insert mode and a steady block in Normal/Visual modes through Crossterm; normal and panic cleanup restore `DefaultUserShape`. Buffer tabs use filename-only `BufferInfo` summaries with space-padded active-background and dirty-marker styling, separated by spaces rather than vertical bars, and occupy the editor's top row. The layout tree is root vertical main/status, main horizontal explorer/editor/tools with display/hidden states, and editor vertical tabs/content. The editor is borderless, and shared Flex layouts allow future panels without component-specific box offsets.

Use a shared pure layout calculation when cursor-following needs the editor area before rendering: update derives the area from terminal size and layout state, while Ratatui rendering uses the same calculation. Avoid mutating `View.area` or scroll offsets as a hidden side effect of `Widget::render`.

Effects may execute synchronously when sufficient; saves use owned asynchronous tasks. File-picker scans, fuzzy filtering, and read-only document/view previews use revisioned blocking requests on Tokio's blocking pool. Current-buffer grep searches an owned `BufferTextSnapshot` containing a `BufferId`, Helix document version, and text; query and document revisions reject stale completions before results or cursor movement are applied. Match-centered previews use `active_preview_at_line`, which projects the active document with its current Helix syntax highlights rather than reparsing or reading the file during rendering. Document opening uses the same boundary: workers prepare `Rope`, language configuration, and optional `Syntax`, while the main update constructs Helix's non-`Send` `Document` and applies buffer/explorer changes. The completion-message boundary keeps I/O and parsing out of state transition logic and keeps terminal input responsive. See [Filesystem work and the file picker](filesystem.md) for the data flow and future mutation rules.

## 6. Auto-save policy

Eggplant **always auto-saves** after accepted content changes. There is no manual Save command, debounce timer, or auto-save configuration. Git remains responsible for intentional version history; auto-save keeps the working file current and provides crash protection.

### Trigger behavior

- Normal-mode content changes and undo/redo start saving directly.
- Insert-mode changes are saved as one session when leaving Insert, avoiding a write per keystroke and preserving a useful undo group.
- Close and quit save dirty buffers first and finish only after successful persistence.
- A scratch/untitled buffer is never written without an explicit path. An attempted save reports the missing-path error and leaves the buffer open/dirty.
- Clean buffers are not rewritten during buffer switches, focus loss, or Insert exit; save effects start only for modified file-backed buffers.
- Saving does not implicitly run formatters, code actions, or other content-changing hooks.

### Save flow

```text
Normal-mode edit / ExitInsert / Close / Quit
                    │
                    ▼
              SaveBuffer(id)
                    │
                    ▼
        save a consistent async snapshot
                    │
                    ▼
SaveFinished { id, saved_revision, result }
```

### Ownership and correctness rules

- The application/effect runtime executes writes; `EditorSession` owns document state and revision transitions.
- Every save targets a stable `BufferId` and a specific document revision.
- A save completion marks only its saved revision clean. If the document has advanced, the buffer remains dirty and another save starts.
- Allow at most one in-flight save per buffer. An edit received during saving is saved by completion follow-up rather than overlapping writes.
- Save errors keep the buffer dirty and remain visible. Close/quit actions are canceled on failure rather than discarding content.
- A missing file-backed path is created on its first successful save; no manual Save command is required.
- Use atomic/replace-safe writes where supported and detect external file changes before overwriting them. Readonly permissions and a newer external mtime fail the save without overwriting either local edits or the external version; there is no automatic merge or force-overwrite workflow.
- A `notify` watcher reports external create/modify/remove events for the workspace or initial file's parent. Matching clean buffers reload through a background `DocumentOpenRequest`; completion checks the expected document version and applies Helix's diff so cursor/history remain valid. Dirty buffers and in-progress saves are left unchanged, and missing files never become empty reloads. Structural create/delete/rename events also refresh the explorer through its existing selection-preserving reload path; content-only changes do not rescan the tree.

With standalone `helix-view::Document`, `Document::save` returns a future and completion event; the caller applies `set_last_saved_revision`. Helix's auto-save handlers live in `helix-term`, so Eggplant directly triggers its shared save path at the events listed above.

## 7. Choosing the Helix boundary

### Option A: `helix-core` with an Eggplant document adapter

Reuse rope text, `Selection`/`Range`, transactions, movement, and history where appropriate. Eggplant owns file loading/saving, path and dirty metadata, document IDs, and viewport behavior.

**Advantages:** smaller product boundary and freedom to keep the first runtime synchronous.

**Costs:** Eggplant must correctly implement encoding/file handling, history commit policy, dirty tracking, and viewport math.

### Option B: selective `helix-view::Document + View`

Helix already provides document opening, transaction application, history/undo/redo, dirty tracking, asynchronous save, per-view selections, and viewport calculations.

**Advantages:** reuses subtle and well-tested editing behavior.

**Costs:** constructors require Helix configuration and syntax-loader values; saving uses futures/Tokio; and the types contain broader language/tooling concerns.

The spike must also verify ID ownership. In the current Helix source, `Editor::new_document` assigns `DocumentId`, while `Document` construction alone does not provide a general external ID-allocation API. Similarly, Helix's `Tree` normally assigns view IDs. Eggplant can use its own IDs at the adapter boundary, but multi-buffer/save/event behavior must not accidentally assume that standalone documents already receive unique Helix IDs.

### Decision criteria

Choose Option B only if the spike demonstrates all of the following without adopting `helix_view::Editor`:

- Multiple documents have a safe Eggplant-owned identity strategy.
- Per-view initialization and cursor/scroll behavior work correctly.
- Configuration/runtime initialization is understandable and proportionate.
- Save completion and revision handling fit the Elm effect boundary.
- Unused language/tooling behavior does not dictate the application runtime.

Otherwise choose Option A. In either case, keep Helix types behind Eggplant's document/view adapter rather than spreading them through application messages and widgets.

## 8. Helix viewport support

Helix has viewport support, but it does **not** provide a separate generic `ScrollView` widget. The main types are `helix_view::View` and `ViewPosition`.

`ViewPosition` contains:

- `anchor`: the document text anchor used for visual-position calculations.
- `horizontal_offset`.
- `vertical_offset`.

The offset is stored per `ViewId` inside `Document`, while `View` stores its ID, document ID, area, gutters, and related state.

Relevant APIs include:

- `Document::ensure_view_init`, `view_offset`, and `set_view_offset`.
- `View::ensure_cursor_in_view` and `ensure_cursor_in_view_center`.
- `View::is_cursor_in_view`.
- `View::screen_coords_at_pos` for terminal cursor placement.
- `View::text_pos_at_screen_coords` for pointer-to-text conversion.
- Visual-line and last-visible-line helpers.

### Integration cautions

- Initialize per-view document state before querying selection or offsets.
- Set the view area whenever layout or terminal dimensions change.
- `View::inner_area` reserves gutter space and a statusline row; verify that this matches Eggplant's layout.
- Convert between Helix graphics types and Ratatui types at the adapter/widget boundary.
- Helix viewport math considers wrapping, formatting, and annotations. Avoid mixing it with an unrelated line-based scroll implementation.
- Perform cursor-following during update/layout handling, not as hidden rendering mutation.

For a core-only first version without soft wrap, a small Eggplant viewport containing dimensions and line/column offsets is reasonable. If visual-line wrapping becomes a requirement, reusing Helix's calculations may be safer than expanding a partial custom implementation.

## 9. Buffer lifecycle rules

1. **Open file:** canonicalize the path; activate an existing buffer or load and insert a new one. Create/retarget the view only after successful loading.
2. **Switch buffer:** retarget the active view, restore the target document's saved view state, and schedule/flush auto-save for the previous dirty buffer.
3. **Edit:** resolve the active buffer through the view and apply a transaction. Never create a document implicitly when no view is active. Content changes auto-save after Normal-mode edits or when leaving Insert.
4. **Save:** route automatic requests through one stable buffer-ID/revision-aware path. A late completion must not mark newer edits as saved.
5. **Close:** a dirty file-backed buffer is saved and then closed only after success. An untitled buffer without a path or any failed save cancels closure with an error; there is no silent-discard workflow.
6. **Close buffer groups:** group-close operations keep the active buffer, close only later tabs, or close every buffer. Dirty targets save first, and `EditorSession::close_buffers` validates every target before closing any, so failures cannot leave a partially closed group. Close-all includes the active buffer and finishes in the empty-editor state.
7. **Close last buffer:** remove the active view and render the empty-editor state.

## 10. Implementation sequence

1. Build a throwaway spike for Option B: create two documents, establish IDs, edit, undo, save, initialize a view, and exercise scrolling.
2. Compare the spike with Option A's required document/file/viewport work and record the choice.
3. Implement `EditorSession`, stable buffer identity, empty state, and open/switch/close unit tests.
4. Connect explorer file-opening messages and completion effects.
5. Add visible-text rendering, cursor placement, and resize behavior.
6. Add the small Normal/Insert command set and revision-aware asynchronous save path.
7. Auto-save after edits and Insert exit, save before close/quit, handle in-flight follow-ups, and surface failures.

## 11. Essential tests

- Folder startup has no active view and does not create a buffer.
- Opening two files produces distinct stable IDs; reopening a path activates rather than duplicates it.
- Switching buffers preserves each buffer's cursor and scroll position.
- Editing changes only the active document and updates dirty state.
- Save completion for an older revision does not clear newer dirty edits.
- All automatic saving uses the same save/completion path; clean buffers are not rewritten.
- Normal-mode edits and leaving Insert trigger auto-save, and an edit during an in-flight save starts one appropriate follow-up.
- Auto-save skips untitled buffers and leaves dirty state intact after errors.
- Buffer switching, focus loss, and clean exit trigger the documented save behavior.
- Closing the last buffer restores a safe empty state.
- Movement, editing, activation, and resize keep the active cursor visible.
- Input with no active document neither panics nor creates an accidental buffer.

## 12. First-edition API review and future automation

Reviewed for future command-driven automation without implementing an agent or a new plugin API.

### Existing safe seams

- `MessageManager` exposes semantic user intents rather than terminal widgets or raw Helix commands.
- `BufferId` is stable across activation order, and `EditorSession` owns buffer/view/editing state.
- `DocumentFactory` and document-open effects keep file acceptance and non-`Send` Helix construction behind the editor boundary.
- Save requests and completions use stable buffer IDs and revision-aware persistence, so newer edits are not marked clean by stale completions.
- Read-only snapshots expose rendering data without allowing components or future tools to mutate documents accidentally.

### Rules for later automated edits

1. Do not synthesize keyboard input, mutate widgets, access Helix documents directly, or write files outside the existing effects path.
2. Current insert/move/undo commands target the **active UI buffer**. They are appropriate for user commands, not sufficient as a safe unattended-agent API.
3. A future agent mutation must name an explicit `BufferId` and expected document revision, apply a transaction through the editor boundary, and reject stale revisions without overwriting newer user edits.
4. Batch/review semantics should be designed when an actual agent workflow needs them. No speculative revisioned arbitrary-transaction API is added in the first edition.
5. Automated edits must use the same undo history, dirty tracking, automatic save, and error-reporting invariants as interactive edits.
6. Milestone 6 should expose stable command IDs and capability-based effects before any external extension host is considered.

The ownership, contribution, and migration contract for that work is defined in [Second-edition feature architecture](feature-architecture.md). Agent-specific session/tool boundaries are documented in [AI agent architecture](agent.md) and [Agent tools](agent-tools.md).
