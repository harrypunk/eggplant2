# Milestone history

This document records completed architecture and editing slices without mixing them into the high-level feature contract. Current VCS work is documented separately in [VCS addon](vcs.md).

## M6 — Feature composition

### M6.1 — Command registry foundation

`CommandRegistry` owns current IDs and descriptions. Direct keymaps and leader targets resolve through it, and duplicate command IDs or duplicate chords in one context fail at startup. Initial handlers resolved to existing messages as a compatibility boundary before narrower feature-owned handlers were introduced.

### M6.2 — Theme picker vertical slice

Resolved bindings emit `InvokeCommand(CommandId)`, and `MessageManager` invokes either compatibility messages or a feature handler. `features/theme_picker/` owns state, command handling, component, and workflow tests; the shell retains placement and cursor policy.

### M6.3 — File picker vertical slice

`FeatureEffect` provides feature-owned blocking-pool execution and completion-message delivery. `FeatureCompletion` routes typed worker results to their owner through one generic shell arm. Scan/filter/preview revisions and completion behavior remain unchanged, while save and document-open effects remain compatibility variants for later shell migration.

### M6.4 — Remaining shell decomposition

- `shell/buffer.rs`: next/previous commands plus shared dirty-save and explorer-reveal capabilities.
- `features/editing.rs`: Insert transitions, movement, insertion, deletion, and history commands.
- `features/explorer/commands.rs`: selection, expansion, refresh, and ignore behavior.
- `shell/lifecycle.rs`: quit, close, deferred lifecycle actions, and save completion.
- `capabilities/document.rs`: open requests, stale-completion protection, insertion, and explorer reveal.
- `shell/focus.rs`: explorer visibility and logical focus relationships.
- `features/file_picker/commands.rs`: picker commands and completion coordination.

`MessageManager` became a small coordinator with explicit compatibility messages for remaining shell concerns.

### M6.5 — Generalized UI contributions

`UiRegistry` registers panel/status renderers and overlay providers. Contributions carry semantic kind, placement, and render functions. The page computes rectangles, renders hints before modals, and suppresses editor cursors when overlays are present. The topmost overlay may publish its own cursor.

### M6.6 — Architecture proof and author guide

`test_utils/example_feature.rs` registers typed state, a command, an overlay, and an effect. Its integration test verifies rendering, typed completion delivery, and visibility cleanup without runtime changes. `docs/feature-authoring.md` records the contribution and testing checklist.

## M7 — Editing and workspace workflows

- **M7.1:** `features/page_scroll.rs` registers page-scroll commands and validates `[features.page_scroll] step` before converting it to viewport-relative movement.
- **M7.2:** `features/buffer_jump/` owns visible two-character matching, adaptive labels, cancellation, and final cursor movement. `TextInputRegistry` handles text input, and editor overlays render dimming/labels from snapshots.
- **M7.3:** `features/buffer_grep/` owns current-buffer regex search, revision checks, result selection, match-centered previews, and jump acceptance through typed completions.
- **M7.4:** `features/project_grep/` owns fixed-workspace search, revisioned previews, and safe result acceptance through `document::open_file_at`.
- **M7.5:** `capabilities/file_watcher.rs` forwards file events. `features/external_sync.rs` schedules revision-checked clean-buffer reloads while preserving dirty/saving buffers and refreshing the explorer for structural changes.
- **M7.6:** the editing feature registers common word motions through `ActiveEditor::move_cursor_word`, retaining Unicode and boundary behavior without edits.

## M8 — Common editing workflows

- **M8.1:** `line_jump` owns count/prefix state and cancellation. `ActiveEditor::move_cursor_to_line` supplies clamped first-nonblank navigation, and `Model.pending_input` provides transient status display.
- **M8.2:** `view_align` registers viewport-alignment commands and second-key input through Helix alignment primitives without changing document state.
- **M8.3:** editing commands provide Insert variants and open-line behavior through the existing transaction, auto-save, and one-step-undo paths.
- **M8.4:** line-boundary commands provide grapheme-safe start/end behavior, with direct-key precedence preserving counted input.
- **M8.5:** Normal, characterwise Visual, and linewise Visual modes share motion paths and one cursor/selection. Snapshots and `TextView` provide selection styling.
- **M8.6:** `change` owns Visual deletion/change, Normal grapheme/line/word deletion and change, pending state, and status cleanup. Normal deletes commit history immediately; changes commit on Insert exit.
- **M8.7:** `yank` owns Visual and line yanks plus cancellation. `RegisterState` mirrors the unnamed register through Helix clipboard support in a blocking effect; failures remain non-blocking.

## Buffer navigation

`features/buffer_navigation.rs` owns only the focused next/previous buffer prefix state, input context, registration, and cancellation. Accepted sequences delegate to existing buffer-switch commands, preserving save-before-switch and explorer reveal without adding a general bracket/unimpaired grammar.

## M9 — VCS

See [VCS addon](vcs.md) for the optional branch display, status page, staging, and inline diff implementation and its ownership rules.
