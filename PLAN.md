# Eggplant first-edition plan

Last updated: 2026-11-12
Current baseline: implemented first-edition feature set in final hardening

## Purpose

The first edition is a small Linux editor with an explorer, multiple buffers, simplified Vim-inspired editing, and safe persistence. It establishes the application/editor boundaries needed for future AI workflows without implementing AI features yet.

Detailed architecture and save semantics live in [Editor design with Helix and Elm](docs/editor-design.md). This file tracks implementation progress and the next reviewable milestones.

## Current state

**The running application now accepts an optional file or folder path.** File and missing-file startup open an editable buffer; folder startup initializes a workspace and focuses the explorer without creating a buffer. `EditorSession` now supports owned asynchronous save requests and revision-aware completion, including stale-save and untitled-buffer handling. Accepted edits automatically execute non-blocking save effects and apply completion messages through the application loop. Close/quit saves dirty buffers and waits for in-flight writes, completing only after successful persistence; failures keep the current state open. Auto-save runs after Normal-mode edits and when leaving Insert, with no manual Save command or timer configuration. The workspace root is fixed after startup. The explorer renders a flattened filesystem tree with selection, file opening, `h`/`l` fold/unfold, Enter to unfold folders, refresh, and `Space e` visibility toggling. Normal and Explorer contexts support logical editor/explorer focus movement with `Ctrl-H`/`Ctrl-L`; `Ctrl-J`/`Ctrl-K` are reserved for future vertical panes. Expansion changes only visible tree entries, never the workspace root. Root and nested `.gitignore` files plus built-in `.git`, `node_modules`, and `__pycache__` entries are ignored by default, and Explorer-context `Shift-I` toggles all ignored entries. `Space f f` opens a background-scanned fuzzy file picker with query/results, ranked matching, and capped UTF-8 text previews. Explorer failures appear in a shared bottom status bar, and focused panels use a double border.

### Completed

- [x] Initial Ratatui shell with explorer/editor/tools layout.
- [x] Elm-style event → message → update → view loop.
- [x] Config loading and validated context-specific keymaps.
- [x] Key parsing with Ctrl/Alt/Shift modifiers.
- [x] Editor architecture documented, including empty-folder state and save design.
- [x] Standalone Helix `Document + View` spike:
  - In-memory and file-backed documents.
  - Transactions, undo, cursor-following, and asynchronous file save.
  - Confirmed that Eggplant must own buffer IDs because standalone Helix documents do not receive distinct IDs.
- [x] `EditorSession` foundation:
  - Stable `BufferId` and multiple buffers.
  - Optional active view and valid empty state.
  - File opening with canonical-path deduplication.
  - Buffer activation and modified-buffer close rejection.
  - Cursor/scroll preservation across buffer switches.
- [x] Basic editing operations:
  - Normal/Insert mode ownership.
  - Left/right/up/down movement.
  - Text insertion and grapheme-aware Backspace.
  - Insert-session history grouping, undo, and redo.
- [x] Read-only `EditorSnapshot` for rendering without exposing Helix internals.
- [x] Ratatui buffer rendering, visible-line limits, horizontal scrolling, and empty state.
- [x] Absolute line numbers with document-width gutters and aligned viewport/cursor calculations.
- [x] Helix-backed exact syntax highlighting with layered scopes and shared semantic UI themes in editor and picker previews.
- [x] Built-in `default`, `tokyonight`, and `catppuccin_mocha` themes using resolved styles and curated Helix-compatible resources, with a `Space u t` selection popup.
- [x] TUI model integration and input-driven editing messages.
- [x] Initial/terminal resize propagation and focus-aware cursor placement.
- [x] Optional startup file/folder path with pre-terminal initialization and startup tests.
- [x] Bottom mode status bar and explorer/editor layout with the tools panel hidden.
- [x] Explorer directory loading, directories-first sorting, selection, and list rendering.
- [x] Explorer navigation, file opening, path deduplication, and editor/explorer focus switching.
- [x] Fixed-root filesystem tree with fold/unfold, nested visible entries, and explicit refresh.
- [x] Root/nested `.gitignore` and common built-in ignore filtering with an Explorer-context `Shift-I` visibility toggle.
- [x] `Space f f` file picker with fast top-level startup, on-demand recursive search, persistent entry cache, Nucleo ranking, and shared read-only viewport previews.
- [x] Unified text-only document opening with canonical-path explorer reveal, ancestor expansion, and selection.
- [x] Background document opening with single-read syntax preparation and stale-completion protection.
- [x] Shared status/error bar and focus-aware panel borders.
- [x] Revision-aware `EditorSession` save requests/completion with in-flight edit and untitled-buffer tests.
- [x] Automatic save after edits/Insert exit through spawned effects and completion messages with status feedback.
- [x] Save-before-close/quit with deferred actions, serial dirty-buffer saves, and in-flight lifecycle tracking.
- [x] `Space b n`/`Space b p` buffer cycling with outgoing dirty-buffer auto-save and explorer synchronization.
- [x] Editor-top buffer tabs with active/dirty styling and a borderless editor.
- [x] Focus-loss auto-save without changing editor mode or focus.
- [x] Built-in Space leader with bottom-right key hints for quit, buffer switching, and file visibility.
- [x] Workspace tests and Clippy with warnings denied.

### Current default commands

| Context | Command | Binding |
| --- | --- | --- |
| Normal / Explorer | Quit | `Space q q` |
| Normal | Enter Insert | `i` |
| Normal | Move | `h`, `j`, `k`, `l` |
| Normal | Undo / redo | `u` / `Ctrl-R` |
| Insert | Return to Normal | `Esc` |
| Insert | Backspace / newline | `Backspace` / `Enter` |
| Insert | Move | Arrow keys |
| Normal | Close active buffer (saves first when dirty) | `Ctrl-W` |
| Normal / Explorer | Move focus left / right | `Ctrl-H` / `Ctrl-L` |
| Normal / Explorer | Move focus up / down (reserved for future panes) | `Ctrl-K` / `Ctrl-J` |
| Normal / Explorer | Next / previous buffer | `Space b n` / `Space b p` |
| Normal / Explorer | Show/hide files (focus when shown) | `Space e` |
| Normal / Explorer | Find/open a file with fuzzy ranking and preview | `Space f f` |
| Normal / Explorer | Select a UI theme | `Space u t` |
| File picker | Edit query | Printable input / `Backspace` |
| File picker | Select | `Down` / `Up` |
| File picker | Open / close | `Enter` / `Esc` |
| Theme picker | Select | `Down` / `Up` |
| Theme picker | Apply / close | `Enter` / `Esc` |
| Explorer | Move selection | `j`, `k` |
| Explorer | Open file / unfold folder | `Enter` |
| Explorer | Fold / unfold folder | `h` / `l` |
| Explorer | Refresh tree | `r` |
| Explorer | Show/hide Git-ignored entries | `Shift-I` |

See `config.example.toml` for the configurable action names. Leader sequences are built-in. Supplying a `[keymap]` table replaces the entire built-in keymap table, so custom configurations must include every required context and binding.

### Known gaps and limitations

- Tools panel is hidden and remains a placeholder for future agent/search/file-info panels.
- No new-file or Save As workflow.
- No soft-wrap rendering, LSP, or split views.
- Existing keymap configuration replaces the configured keymap section rather than offering sophisticated default merging/unbinding.

## Roadmap

Each milestone should remain small enough to review independently. Update this file when scope or completion status changes.

### Milestone 1: Open a real path at startup

**Goal:** make the running app display an actual file or an empty-folder workspace.

- [x] Accept an optional startup path using the smallest appropriate CLI handling.
- [x] Opening a file creates/activates its file-backed buffer.
- [x] Opening a folder initializes workspace/explorer state without creating a buffer.
- [x] No path starts with the explicit empty state.
- [x] Startup/open errors restore the terminal and report clearly.
- [x] Add file, folder, missing-path, and no-path tests.

**Exit criteria:** `cargo run -p tui -- <file>` displays and edits the file; `<folder>` shows an explorer with no active document.

### Milestone 2: Functional explorer and focus workflow

**Goal:** open files without relying on startup arguments.

- [x] Load immediate directory entries, sort directories first, and model selection/empty states.
- [x] Surface in-app directory loading errors through the shared status model.
- [x] Fold/unfold directories in a visible tree without changing the fixed workspace root.
- [x] Navigate entries and open the selected file through application messages/effects.
- [x] Switch focus between explorer and editor with configured commands.
- [x] Opening an already-open file activates its existing buffer.
- [x] Refresh directories after relevant filesystem changes or on explicit request.
- [x] Render sorted entries and the selected row with Ratatui.
- [x] Render focus and in-app filesystem errors.
- [x] Add temporary-directory and `TestBackend` tests for the initial directory listing.
- [x] Add navigation/open/focus integration tests.

**Exit criteria:** start with a folder, navigate the explorer, open a file, edit it, and return to the explorer without losing buffer state.

### Milestone 3: Production save and dirty-buffer safety

**Goal:** provide one correct automatic save path and safe close/quit behavior.

- [x] Add `EditorSession` save support using stable buffer IDs and document revisions.
- [x] Execute Helix save futures through the application effect/runtime boundary.
- [x] Apply `set_last_saved_revision` on successful completion.
- [x] Keep edits made during an in-flight save dirty.
- [x] Auto-save after Normal-mode content changes and when leaving Insert; no manual command.
- [x] Add a small status/error model and display open/save failures.
- [x] Save before close/quit for dirty, untitled, saving, and failed-save buffers; never auto-discard.
- [x] Test core save success, stale completion, and untitled-buffer failure.
- [x] Test application save failures and close/quit safety.

**Exit criteria:** open → edit → automatic save → reopen verifies persisted content, and failed/stale saves cannot silently lose or clean newer edits.

### Milestone 4: Auto-save and multiple-buffer workflows

**Goal:** implement the documented modern persistence behavior and make existing multi-buffer support reachable.

- [x] Auto-save dirty file-backed buffers after edits; never auto-write untitled buffers.
- [x] Save on buffer switch and clean exit.
- [x] Save on terminal focus loss without changing mode or focus.
- [x] Allow at most one in-flight save per buffer and start a follow-up for newer edits.
- [x] Expose buffer switching and safe closing through commands/UI.
- [x] Preserve cursor/scroll state and surface save failures across switches.
- [x] Test untitled behavior, in-flight edits, buffer switching, and exit flushing.
- [x] Test terminal focus-loss saving.

**Exit criteria:** normal editing persists automatically, and switching/closing buffers never silently discards changes.

### Milestone 5: First-edition hardening and review

**Goal:** make the narrow feature set reliable rather than expanding scope.

- [x] Finalize the first-edition command table and update `config.example.toml`.
- [x] Test narrow terminals, resize, Unicode/graphemes, wide characters, horizontal scrolling, and empty documents.
- [x] Test large files, permissions, missing files, external modification handling, and save failures.
- [x] Harden terminal cleanup paths and test clean shutdown with pending saves.
- [x] Execute the real-terminal panic/restoration checks in `docs/smoke-test.md`.
- [x] Run an end-to-end explorer → open → edit → automatic save → switch/close test suite.
- [x] Review public editor/application APIs against future AI command use without implementing agents.
- [x] Update README, editor design, and this plan to match actual behavior.

**Exit criteria:** the definition of done below is covered by tests and a documented Linux smoke test.

## First-edition definition of done

The first edition is complete when a user can, on Linux:

1. Start with no path, a file, or a folder.
2. Navigate the explorer and open files without duplicate buffers.
3. Move, insert, delete, undo, and redo using the documented simplified keymap.
4. Switch and safely close multiple buffers while retaining cursor/scroll state.
5. Rely on revision-safe automatic saving after edits and before close/quit.
6. See actionable open/save/filesystem errors without terminal corruption or silent data loss.
7. Resize the terminal and work with ordinary Unicode text predictably.
8. Run the complete workspace test suite and documented smoke tests successfully.

## Second edition: sustainable feature growth

### Milestone 6: Feature architecture and command platform

**Goal:** add application features through registered modules and explicit contribution points instead of growing `MessageManager`, the global message/keymap parser, runtime effect matching, and page conditionals for every feature. Establish the internal extension contract before considering a third-party plugin runtime.

This is not a promise that new code requires zero wiring. A new feature may be registered explicitly at the composition root; domain coordinators and unrelated features must not need modification. Prefer explicit typed state over a global `Any` map, and introduce abstractions only when a migration or new feature demonstrates their need.

#### Architecture rules

- A feature owns its state, semantic commands/messages, update logic, effects, components, and tests.
- The application shell owns lifecycle, focus/modal precedence, layout slots, command/effect registration, and shared editor/filesystem capabilities.
- Commands have stable IDs such as `editor.undo`, `file.open`, and `theme.select`. Configured keys and built-in leader sequences both resolve command IDs; adding a command must not require another arm in `parse_message` or the leader update match.
- Updates remain pure. Feature-owned effect handlers are registered at the runtime boundary rather than adding every request type to one central `Effect` match.
- Pages continue to own layout and modal precedence. Features contribute panels, popups, or status items through explicit slots; they do not perform rendering-side I/O or take over the terminal.
- Preserve the existing editor boundary: stable `BufferId`/revision checks, main-thread construction of non-`Send` Helix documents, text-only file acceptance, and always-on auto-save are core invariants, not feature options.

#### Work items

- [x] Record the current shell/feature responsibilities and command/effect contribution contract in [Second-edition feature architecture](docs/feature-architecture.md) before moving code.
- [x] Introduce a command registry with stable IDs, metadata, and compatibility message handlers. Migrate direct key bindings and built-in leader menus to shared resolution and detect registration/binding conflicts at startup.
- [x] Migrate the theme picker end-to-end as the first small feature: owned state/update, feature command handler, popup component, and colocated tests.
- [x] Migrate the file picker as the effect-rich proof: scanning/filtering/preview/open behavior uses feature-owned effect objects and completion routing without central picker-specific runtime arms.
- [x] Migrate buffer switching and shared dirty-save/explorer-reveal capabilities into a buffer feature with colocated tests.
- [x] Migrate Insert-mode, movement, editing, and history commands into an editing feature with colocated tests.
- [x] Migrate explorer selection, expansion, refresh, and ignore commands into the explorer module with colocated tests.
- [x] Migrate quit, close-buffer, deferred lifecycle actions, and save completion into a lifecycle feature with colocated tests.
- [x] Extract document-open request/completion capabilities with colocated stale-completion and rejected-file tests.
- [x] Migrate explorer visibility and logical focus commands into a focus feature with colocated tests.
- [x] Migrate the remaining file-picker commands and completion coordination into the file-picker feature. `MessageManager` is now a small application coordinator rather than a growing feature match.
- [ ] Replace centralized feature conditionals in pages with explicit panel/popup/status contribution points while keeping focus, cursor hiding, and modal precedence shell-owned.
- [ ] Implement dynamic Helix-compatible user themes through the new platform: discover `~/.eggplant/themes/*.toml`, support runtime theme IDs/catalog entries, resolve inheritance and name collisions, load through effects, and preserve the active theme on failures.
- [ ] Add a test-only example feature proving that command, state/update, popup, and effect contributions work through registration without changes to unrelated domain modules or runtime dispatch.
- [ ] Document a feature-authoring checklist: registration, commands/keymaps, effects/completions, UI slots, errors, capabilities, and required tests.

**Exit criteria:** existing user behavior and regression tests remain intact; central update, keymap parsing, and runtime dispatch contain no growing per-feature command/effect matches; a new example feature works through composition-root registration alone; dynamic themes use the same extension path rather than special-case core logic.

### After Milestone 6: external extensions only if justified

Do not add a native Rust dynamic-library ABI or unrestricted in-process third-party code. If external plugins become a real requirement, evaluate a versioned, capability-based WebAssembly or out-of-process extension host against the now-stable internal command/effect API. Filesystem, editor mutation, terminal, and future AI capabilities must be explicit, and plugins must not bypass save safety, revision checks, or text-file acceptance rules.

## Deferred beyond the second edition

- AI agent execution/chat and automated editing.
- LSP, DAP, formatting, and code actions.
- Full Vim or Helix command compatibility, Visual mode, macros, and registers.
- Split views and multiple cursors as user-facing features.
- Soft-wrap visual rendering and advanced annotations.
- Remote workspaces and non-Linux support.

## Working agreement

- Implement one reviewable milestone or vertical slice at a time; stop for review between steps.
- Keep Helix behind the editor adapter and Ratatui out of document/editing state.
- Keep one terminal owner and one application event/update loop.
- Route file I/O, timers, and future AI work through explicit messages/effects.
- Require `cargo test --workspace`, Clippy with warnings denied, formatting, and `git diff --check` before milestone commits.
- Update `PLAN.md` and relevant design docs when behavior or priorities change.
