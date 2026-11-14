# Eggplant first-edition plan

Last updated: 2026-11-14
Current baseline: Milestone 8 complete

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

The complete context/action/binding table lives in [Key input, sequences, and hints](docs/key-input.md). See `config.example.toml` for configurable action names. Leader sequences are built-in. Custom `[keymap]` entries merge with built-in defaults by context and action, so users only need to specify overrides. Explicit unbinding remains a separate feature.

### Known gaps and limitations

- Tools panel is hidden and remains a placeholder for future agent/search/file-info panels.
- No new-file or Save As workflow.
- No soft-wrap rendering, LSP, or split views.
- Explicit keymap unbinding and advanced rule editing are not implemented; custom entries otherwise merge with defaults.

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
- [x] Replace centralized feature conditionals in pages with explicit panel/popup/status contribution points while keeping focus, cursor hiding, and modal precedence shell-owned.
- [x] Add a test-only example feature proving that command, state/update, popup, and effect contributions work through registration without changes to unrelated domain modules or runtime dispatch.
- [x] Document a feature-authoring checklist: registration, commands/keymaps, effects/completions, UI slots, errors, capabilities, and required tests.

**Exit criteria:** existing user behavior and regression tests remain intact; central update, keymap parsing, and runtime dispatch contain no growing per-feature command/effect matches; a new example feature works through composition-root registration alone; panel/popup/status contributions preserve shell-owned focus and modal rules. Dynamic themes and third-party extension hosts are not part of M6.

## Third edition: core editing workflows

### Milestone 7: Editing workflows, buffer/project grep, and external buffer sync

**Status: complete (2026-11-14).**

**Goal:** deliver frequently used editing features through M6's command/effect/UI contribution boundaries. Finish M6 before starting these slices; no dynamic-theme or third-party-plugin work is a prerequisite.

#### M7.1 — Page-scroll

- [x] Finalize `page_scroll_down`/`page_scroll_up` with `Ctrl-F`/`Ctrl-U` and configurable `[features.page_scroll] step` percentages.
- [x] Add feature-owned page-scroll commands that preserve the desired cursor column and keep the cursor visible while clamping to document boundaries.
- [x] Reuse the shared Helix/Eggplant viewport policy; no render-time state mutation or independent offset calculation.
- [x] Test empty documents, documents shorter than a page, first/last pages, narrow terminals, resize, and wide Unicode text.

**Exit criteria:** configurable percentage-based scrolling works with the existing cursor and viewport, without changing auto-save, editing history, or focus behavior.

#### M7.2 — Current-buffer visible jump

Start with a two-character, Leap-style jump limited to the active buffer's visible snapshot. Workspace-wide grep remains a separate later feature.

- [x] Start with `Space c j`, accept a smart-case two-character pattern, assign one-key labels for up to 26 matches or two-key labels above that, and jump when a complete label is entered.
- [x] Keep pattern, label, targets, and active-buffer identity in the `buffer_jump` extension; use generic absolute cursor navigation only on acceptance.
- [x] Register text input, prompt UI, and editor-label overlays without feature-specific message/runtime/page dispatch.
- [x] Dim visible text while labels are active; Escape cancels without movement and Backspace edits the current input.
- [x] Test no matches, Unicode display columns, horizontal clipping, narrow resize, accept/cancel, and cursor/dimming behavior.

**Exit criteria:** users can jump to any labeled match in the current visible viewport without scanning offscreen text, changing the document, or adding jump policy to the editor core.

#### M7.3 — Current-buffer live grep

- [x] Use the pinned LAN ripgrep `grep-regex`/`grep-searcher` crates so current and future project search share the same matching/searching foundation.
- [x] Open a picker-style extension with `Space s c`; provide a shared text input with cursor, smart-case regex results, match-centered syntax-highlighted context preview, yellow match blocks, and a selected preview-line style.
- [x] Search an immutable active-buffer text snapshot in a revisioned blocking effect and return results through typed `FeatureCompletion` routing.
- [x] Reject stale query and document revisions; support Up/Down selection, Enter to jump to the match line, and Escape to cancel without movement.
- [x] Test smart case, Unicode, invalid regexes, stale completions, preview context, and the leader/search/selection/jump workflow.

**Exit criteria:** live grep searches current in-memory buffer text without filesystem access or blocking terminal input, while extension-owned state/effects/UI preserve document and cursor invariants.

#### M7.4 — Project live grep

- [x] Reuse the shared ripgrep `TextSearcher`, workspace ignore traversal, and regular-file/NUL-free UTF-8 acceptance rules; compile each query once and search in a blocking effect.
- [x] Open with `Space s p`; show path/line results with yellow match blocks and selected syntax-highlighted file previews using the shared picker layout and input cursor.
- [x] Route search and preview completions through typed `FeatureCompletion`, with independent query/preview revisions and a 1,000-result cap.
- [x] Open the selected file through generic `document::open_file_at` and move to its match line after safe insertion; close/cancel without project-specific shell dispatch.
- [x] Test workspace ignore/binary rules, invalid regexes, stale query/preview completions, syntax/match highlighting, and the leader/search/preview/open workflow.

**Exit criteria:** users can live-search fixed-workspace files and open a selected match without blocking terminal input, bypassing file acceptance rules, or adding project-grep policy to central dispatch.

#### M7.5 — External buffer synchronization

- [x] Watch the fixed workspace or initial file's parent with `notify` and forward create/modify events as ordinary messages.
- [x] Reuse background document preparation for changed open files; check buffer identity/version and dirty/save state before applying a Helix diff reload.
- [x] Reload clean buffers with valid cursor/history/syntax state; preserve dirty buffers, active saves, and missing-file contents without changing auto-save behavior.
- [x] Test clean/same-content reloads, dirty conflicts, deletion, stale completions, watcher event filtering, and the message/effect workflow.

**Exit criteria:** external tools update clean open buffers automatically without user save/reload commands, while local edits and save conflict protections remain intact.

#### M7.6 — Common word motions

- [x] Document cursor-only selection semantics and exclude operators, counts, Visual mode, and inner/around-object grammar.
- [x] Add Normal-mode `w`, `b`, and `e` for next word start, previous word start, and next word end through semantic editing commands.
- [x] Use Helix word primitives and collapse motion results to cursor points without modifying text; retain Helix's Unicode and line-ending boundary policy.
- [x] Test empty documents, Unicode, repeated motion, document boundaries, and command-level cursor behavior.

**Exit criteria:** common word motions work predictably with the existing single-cursor editor, without introducing a selection/operator system or a parallel text-object implementation.

### Milestone 7 review rule

Implement and review M7.1, M7.2, M7.3, M7.4, M7.5, and M7.6 independently. If text-object semantics require a larger selection-model change, stop and document it before adding operators or visual-mode behavior.

### Milestone 8: Common editing workflows

**Status: complete (2026-11-14).**

**Goal:** add frequent navigation and insert workflows through the existing editor/session, command, input-context, and status/UI boundaries. Keep the implementation Vim-inspired, not a general Vim compatibility project.

#### M8.1 — Counted `gg`/`G` and visible pending input

**Status: complete (2026-11-14).**

- [x] Support `gg` to the first line and `G` to the last line; an optional count selects that one-based line for either command.
- [x] Accept counts beginning with `1`–`9` and continuing with any digit. Only `gg`/`G` accept a count: forms such as `3w`, `2j`, and counted operators are unsupported. Keep existing word motions limited to the common, uncounted `w`/`b`/`e`; do not add a general count/operator grammar.
- [x] Move to the target line's first non-blank character, falling back to column zero for blank lines, clamp out-of-range lines to document boundaries, and keep the cursor visible.
- [x] Show the active count and pending `g` prefix in the status bar as transient input, separate from persistent save/error messages. Clear it on completion, Escape, or an invalid sequence; invalid input must not edit or move unexpectedly.
- [x] Test empty/single-line documents, blank lines, Unicode, repeated digits, out-of-range counts, cancellation, invalid keys, viewport scrolling, and status cleanup.

#### M8.2 — `zz`/`zt`/`zb` viewport alignment

**Status: complete (2026-11-14).**

- [x] `zz` centers the cursor line, `zt` aligns it to the top, and `zb` aligns it to the bottom of the visible editor viewport.
- [x] Reuse the shared Helix visual-viewport policy; change only the viewport while preserving the cursor, selection, text, revision, and dirty/save state.
- [x] Reuse M8.1's transient pending-input display for `z`, with the same completion/Escape/invalid-key cleanup.
- [x] Clamp alignment at document start/end and test short documents, first/last lines, narrow/empty viewports, Unicode/visual rows, and resize.

#### M8.3 — Common insert variants

**Status: complete (2026-11-14).**

- [x] Keep existing `i`; add `a` after the current grapheme, `A` at line end, and `I` at the first non-blank character of the current line.
- [x] Add `o`/`O` to insert an indented empty line below/above the current line and enter Insert mode. Use Helix's indentation/transaction primitives where useful and keep behavior predictable on blank or empty lines.
- [x] Route open-line edits through the existing transaction, dirty-revision, auto-save, and undo paths; subsequent typing remains in the same Insert session/history group.
- [x] Test empty documents, blank lines, first/last lines, Unicode/grapheme boundaries, indentation, undo/redo, and auto-save without adding a manual Save workflow.

#### M8.4 — Line start/end with `0`/`$`

**Status: complete (2026-11-14).**

- [x] `0` moves to column zero and `$` moves to the final grapheme before the line ending; empty lines use column zero.
- [x] Keep these as cursor-only Normal-mode motions with existing viewport following and no text/revision/dirty-state changes.
- [x] Keep count parsing unambiguous: `0` invokes line start when no count is active, while digits after a count has started continue that count.
- [x] Reuse the same motions to extend the selection after M8.5; test empty/blank lines, Unicode/graphemes, first/last lines, and count interaction.

#### M8.5 — Characterwise Visual selection

**Status: complete (2026-11-14).**

- [x] Add `v` in Normal mode to start a characterwise selection anchored at the current cursor. Existing movement commands extend the primary selection while in Visual mode.
- [x] Render the selection with the existing theme selection style, show `VISUAL` in the status bar, and keep the moving cursor visible in both selection directions.
- [x] Escape returns to Normal mode and collapses the selection at the current cursor without changing text. Reuse existing movement adapters with explicit move/extend behavior rather than duplicating motion rules.
- [x] Test empty documents, Unicode/graphemes, forward/backward selection, direction reversal, line crossings, viewport scrolling, mode display, and cancellation.
- [x] Use Visual mode for selections larger than the supported word/line motions instead of expanding the text-object motion catalog. Keep Visual-mode operators, clipboard/register integration, and other selection grammars for separate slices.

#### M8.6 — Delete/change with `x`/`d`/`dd`/`c`/`cc`

**Status: complete (2026-11-14).**

- [x] In Visual mode, `x` and `d` delete the characterwise selection, while `c` deletes it and enters Insert mode at the selection start.
- [x] In Normal mode, `x` deletes the grapheme under the cursor, `dd` deletes the current line, and `cc` clears the current line and enters Insert mode using the established indentation policy.
- [x] Normal `d` and `c` wait specifically for the second `d`/`c`; show the pending key through M8.1's transient status display and clear it on completion, Escape, or invalid input. Do not add general operator-motion combinations such as `dw` or `cw`.
- [x] Route every deletion/change through the existing transaction, undo/redo, dirty-revision, viewport, and auto-save paths. Each command is one undoable edit; M8.7 centralizes register/clipboard synchronization rather than duplicating it in each command.
- [x] Test empty documents, empty selections, Unicode/graphemes, first/last lines, line-ending boundaries, pending-key cancellation, Insert entry, undo/redo, and auto-save.

#### M8.7 — Yank and automatic system clipboard synchronization

**Status: complete (2026-11-14).**

- [x] Add Visual `y` to yank the characterwise selection and return to Normal mode, and Normal `yy` to yank the current line including its line-ending semantics. Pending Normal `y` uses the shared transient status display and cancellation rules.
- [x] Maintain one automatic unnamed register for Visual/Normal yanks and accepted M8.6 deletes/changes. Users never select a register or manage a low-level clipboard command.
- [x] Mirror the unnamed register to the system clipboard through a shared background-capability adapter, reusing Helix clipboard support where practical. Clipboard failure leaves the internal register successful and reports a concise non-blocking status; it must not change text or interrupt the workflow.
- [x] Yanks never modify text, dirty state, or revision and therefore do not trigger auto-save. Test visual/line yanks, delete/change register updates, pending-key cancellation, Unicode, empty selections, and clipboard-unavailable fallback.
- [x] Keep paste commands, named/register selection, macros, and register UIs as separate work.

### Milestone 8 review rule

Implement and review M8.1–M8.7 independently. Update the current keymap table only as each binding becomes available. Eggplant exposes exactly one cursor and one selection; Helix's internal multi-selection support must not be used to create user-facing multi-cursor behavior. Counts are accepted only by `gg`/`G`; existing `w`/`b`/`e` and all other motions are uncounted. General operator-motion grammar, macros, named/register-selection workflows, additional text-object motion catalogs, and complete Vim/Helix compatibility remain outside M8.

## TBD — much later editions

- Dynamic Helix-compatible user themes and broader visual style/theming expansion. Existing built-in themes and semantic style boundaries remain unchanged.
- Third-party plugin/extension hosts. Do not add a native Rust dynamic-library ABI or unrestricted in-process third-party code; evaluate versioned, capability-based WebAssembly or out-of-process hosting only if justified by real requirements and stable internal contracts.
- AI agent execution/chat and automated editing.
- LSP, DAP, formatting, and code actions.
- Full Vim or Helix command compatibility, general Visual-mode operator grammar, macros, and named/register-selection workflows.
- Split views as a user-facing feature.
- Soft-wrap visual rendering and advanced annotations.
- Remote workspaces and non-Linux support.

## Working agreement

- Implement one reviewable milestone or vertical slice at a time; stop for review between steps.
- Keep Helix behind the editor adapter and Ratatui out of document/editing state.
- Expose exactly one cursor/selection; multi-cursor features are intentionally unsupported.
- Keep one terminal owner and one application event/update loop.
- Route file I/O, timers, and future AI work through explicit messages/effects.
- Require `cargo test --workspace`, Clippy with warnings denied, formatting, and `git diff --check` before milestone commits.
- Update `PLAN.md` and relevant design docs when behavior or priorities change.
