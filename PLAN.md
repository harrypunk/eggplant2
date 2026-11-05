# Eggplant first-edition plan

Last updated: 2026-11-05  
Current baseline: `main` at `9aa350e`

## Purpose

The first edition is a small Linux editor with an explorer, multiple buffers, simplified Vim-inspired editing, and safe persistence. It establishes the application/editor boundaries needed for future AI workflows without implementing AI features yet.

Detailed architecture and save semantics live in [Editor design with Helix and Elm](docs/editor-design.md). This file tracks implementation progress and the next reviewable milestones.

## Current state

**The running application now accepts an optional file or folder path.** File and missing-file startup open an editable buffer; folder startup initializes a workspace and focuses the explorer without creating a buffer. Saving has been validated in the Helix spike but is not yet exposed through `EditorSession` or the TUI. The workspace root is fixed after startup. The explorer renders a flattened filesystem tree with selection, file opening, `h`/`l` fold/unfold, Enter to unfold folders, refresh, and `Ctrl-E` focus toggling. Expansion changes only visible tree entries, never the workspace root. Explorer failures appear in a shared bottom status bar, and focused panels use a double border.

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
- [x] TUI model integration and input-driven editing messages.
- [x] Initial/terminal resize propagation and focus-aware cursor placement.
- [x] Optional startup file/folder path with pre-terminal initialization and startup tests.
- [x] Bottom mode status bar and explorer/editor layout with the tools panel hidden.
- [x] Explorer directory loading, directories-first sorting, selection, and list rendering.
- [x] Explorer navigation, file opening, path deduplication, and editor/explorer focus switching.
- [x] Fixed-root filesystem tree with fold/unfold, nested visible entries, and explicit refresh.
- [x] Shared status/error bar and focus-aware panel borders.
- [x] Workspace tests and Clippy with warnings denied.

### Current default commands

| Context | Command | Binding |
| --- | --- | --- |
| Normal | Quit | `q` |
| Normal | Enter Insert | `i` |
| Normal | Move | `h`, `j`, `k`, `l` |
| Normal | Undo / redo | `u` / `Ctrl-R` |
| Insert | Return to Normal | `Esc` |
| Insert | Backspace / newline | `Backspace` / `Enter` |
| Insert | Move | Arrow keys |
| Normal / Explorer | Toggle explorer/editor focus | `Ctrl-E` |
| Explorer | Move selection | `j`, `k` |
| Explorer | Open file / unfold folder | `Enter` |
| Explorer | Fold / unfold folder | `h` / `l` |
| Explorer | Refresh tree | `r` |

See `config.example.toml` for the configurable action names.

### Known gaps and limitations

- Tools panel is hidden and remains a placeholder for future agent/search/file-info panels.
- No interactive open, buffer-switch, close, new-file, or Save As workflow.
- No production `EditorSession` save method or manual Save binding.
- No auto-save scheduler; only its design is documented.
- The status bar handles explorer errors; save/quit and other editor errors are not wired yet.
- Dirty-close protection exists in the core, but quit/close UX is not wired.
- No soft-wrap rendering, syntax highlighting, LSP, or split views.
- Existing keymap configuration replaces the configured keymap section rather than offering sophisticated default merging/unbinding.
- Horizontal rendering and Unicode behavior need broader edge-case testing.

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

**Goal:** provide one correct save path before adding automation.

- [ ] Add `EditorSession` save support using stable buffer IDs and document revisions.
- [ ] Execute Helix save futures through the application effect/runtime boundary.
- [ ] Apply `set_last_saved_revision` on successful completion.
- [ ] Keep edits made during an in-flight save dirty.
- [ ] Add manual Save with a configurable binding; `Ctrl-S` is the current candidate.
- [ ] Add a small status/error model and display open/save failures.
- [ ] Define close/quit behavior for dirty, untitled, saving, and failed-save buffers.
- [ ] Test successful save, save failure, stale completion, and close/quit safety.

**Exit criteria:** open → edit → manual save → reopen verifies persisted content, and failed/stale saves cannot silently lose or clean newer edits.

### Milestone 4: Auto-save and multiple-buffer workflows

**Goal:** implement the documented modern persistence behavior and make existing multi-buffer support reachable.

- [ ] Add the 800 ms configurable edit debounce.
- [ ] Auto-save only dirty, file-backed buffers; never auto-write untitled buffers.
- [ ] Save on buffer switch, terminal focus loss, and clean exit.
- [ ] Allow at most one in-flight save per buffer and schedule a follow-up for newer edits.
- [ ] Route manual and automatic saving through the same implementation.
- [ ] Expose buffer switching and safe closing through commands/UI.
- [ ] Preserve cursor/scroll state and surface save failures across switches.
- [ ] Test debounce restart, disabled/untitled behavior, in-flight edits, focus transitions, and exit flushing.

**Exit criteria:** normal editing persists without manual Save, manual Save remains available, and switching/closing buffers never silently discards changes.

### Milestone 5: First-edition hardening and review

**Goal:** make the narrow feature set reliable rather than expanding scope.

- [ ] Finalize the first-edition command table and update `config.example.toml`.
- [ ] Test narrow terminals, resize, Unicode/graphemes, wide characters, horizontal scrolling, and empty documents.
- [ ] Test large files, permissions, missing files, external modification handling, and save failures.
- [ ] Verify panic-safe terminal restoration and clean shutdown with pending saves.
- [ ] Run an end-to-end explorer → open → edit → auto/manual save → switch/close test suite.
- [ ] Review public editor/application APIs against future AI command use without implementing agents.
- [ ] Update README, editor design, and this plan to match actual behavior.

**Exit criteria:** the definition of done below is covered by tests and a documented Linux smoke test.

## First-edition definition of done

The first edition is complete when a user can, on Linux:

1. Start with no path, a file, or a folder.
2. Navigate the explorer and open files without duplicate buffers.
3. Move, insert, delete, undo, and redo using the documented simplified keymap.
4. Switch and safely close multiple buffers while retaining cursor/scroll state.
5. Rely on revision-safe auto-save and use manual Save when needed.
6. See actionable open/save/filesystem errors without terminal corruption or silent data loss.
7. Resize the terminal and work with ordinary Unicode text predictably.
8. Run the complete workspace test suite and documented smoke tests successfully.

## Deferred beyond the first edition

- AI agent execution/chat and automated editing.
- LSP, DAP, syntax highlighting, formatting, and code actions.
- Full Vim or Helix command compatibility, Visual mode, macros, and registers.
- Split views and multiple cursors as user-facing features.
- Soft-wrap visual rendering and advanced annotations.
- Plugins, remote workspaces, and non-Linux support.

## Working agreement

- Implement one reviewable milestone or vertical slice at a time; stop for review between steps.
- Keep Helix behind the editor adapter and Ratatui out of document/editing state.
- Keep one terminal owner and one application event/update loop.
- Route file I/O, timers, and future AI work through explicit messages/effects.
- Require `cargo test --workspace`, Clippy with warnings denied, formatting, and `git diff --check` before milestone commits.
- Update `PLAN.md` and relevant design docs when behavior or priorities change.
