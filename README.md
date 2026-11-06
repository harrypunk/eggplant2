# Eggplant Editor

Eggplant is an **AI-first, keyboard-driven terminal editor** written in Rust. Its long-term direction is an editor where exploration, editing, and AI-assisted workflows share the same application commands rather than being separate, tightly coupled features.

The first edition deliberately stays small:

- Linux only.
- A file explorer and basic text editing.
- A simplified, Vim-inspired interaction model.
- [Ratatui](http://git.z.lan/fork/rataui) for the terminal UI.
- [Helix](http://git.z.lan/fork/helix) only for low-level text, document, and editing primitives.

The repository is currently an early prototype. The existing `tui` crate demonstrates an Elm-style event/update/view loop and configurable key contexts; it does not yet provide real document editing.

## Product decisions

| Decision | Direction |
| --- | --- |
| Product identity | AI-first editor, not a Helix clone or a terminal-only front end for Helix. |
| Helix responsibility | Low-level text/document/edit operations only. Eggplant owns product behavior. |
| Editing style | Vim-inspired and customized for simplicity; not Helix's selection-first UI or full Vim compatibility. |
| First release | File explorer plus basic open/edit/save workflows. |
| Persistence | Always auto-save after editing and before close/quit; no manual Save command or debounce settings. |
| Initial platform | Linux. Keep platform-specific terminal code isolated, but do not design for other platforms yet. |
| Deferred features | LSP, DAP, advanced syntax UI, full Helix command compatibility, macros, plugins, and agent execution. |

AI-first is an architectural direction, not a requirement to ship agent chat immediately. The first edition should establish safe, reusable command and effect boundaries that later AI features can use.

## Architecture decision

Build a **custom Eggplant application over a thin editor core**, rendered by Ratatui:

```text
┌─────────────────────────────────────────────────────────┐
│ TUI                                                     │
│ Ratatui pages/widgets · terminal lifecycle · input      │
├─────────────────────────────────────────────────────────┤
│ Application                                             │
│ focus · explorer · overlays · messages · effects        │
│ simplified Vim-inspired key routing · configuration     │
├─────────────────────────────────────────────────────────┤
│ Editor core                                             │
│ EditorSession · documents · cursor/selection · viewport │
│ editing commands · transactions · undo · file I/O       │
├─────────────────────────────────────────────────────────┤
│ Selected low-level Helix APIs                           │
│ rope/text · selections · transactions · movement        │
│ optional document/view primitives after a spike         │
└─────────────────────────────────────────────────────────┘
```

### Helix boundary

Start **`helix-core`-first**:

- Use Helix data structures and operations for text, selections/ranges, transactions, movement, and editing history where practical.
- Evaluate `helix-view::Document` and `View` during the initial spike if they reduce file-I/O and viewport work.
- Do **not** adopt `helix_view::Editor` as Eggplant's application controller.
- Do **not** depend on `helix-term`, `helix-tui`, Helix keymaps, or Helix commands for the first-edition architecture.
- Do not pull in LSP/DAP/VCS behavior just to obtain basic editing.

`helix-view` is a stateful editor layer with language-tooling and workspace concerns, not a lightweight document crate. If using only `Document`/`View` still forces unwanted global setup or behavior, use `helix-core` with a small Eggplant-owned document wrapper instead. This is the main technical question for the architecture spike.

This boundary means Eggplant intentionally implements its own small command set. That is simpler and more appropriate here than porting Helix's compositor, jobs, handlers, prompts, and terminal widgets.

## Workspace boundaries

The target workspace has three meaningful responsibilities. Crate names can change when implementation starts; there is no need to rename the prototype immediately.

```text
crates/
├── app/       # Application state, explorer, messages, effects, and input routing
├── editor/    # Eggplant-owned editing model and narrow Helix adapter
└── tui/       # Binary, terminal runtime, Ratatui pages, and widgets
```

The existing placeholder `api` crate can become `app`, or it can later hold a stable automation/API contract if external AI processes need one. Avoid adding more crates until a real dependency or testing boundary requires them.

### `app`

Owns product-level state and coordination:

- Running state, focused component, overlay/context stack, and panel visibility.
- File explorer state and actions.
- Application messages, updates, and effects.
- Routing between global, explorer, and editor input.
- Coordination with future AI features through the same public commands used by the UI.

It should not depend on Ratatui or terminal setup. Normalize Crossterm input into application-owned key types at the TUI boundary so tests can construct input directly.

### `editor`

Owns editing state and exposes a narrow API to the application:

- Open, close, read, and save documents.
- Active document identity, path, dirty state, and revision.
- Cursor/selection, simplified edit mode, viewport, and editing history.
- Basic movement, insertion, deletion, and undo/redo commands.
- Conversion of commands into Helix transactions or document operations.
- Read-only document/view snapshots needed by the renderer.

`DocumentFactory` owns Helix configuration/loaders and constructs or opens documents outside `EditorSession`. The session inserts those documents and manages buffer/view state; runtime resources do not belong in the central model.

Do not re-export broad Helix APIs throughout the application. Keep Helix types inside the adapter where practical so upstream changes do not spread into the explorer, widgets, or future AI tools.

Ratatui and Crossterm types do not belong in this crate.

### `tui`

Is the composition root and only owner of the terminal:

- Initialize and reliably restore the terminal, including on panic.
- Translate terminal events and drive the application loop.
- Execute effects and route completion events back into the application.
- Lay out and render the explorer, editor, status, and dialog widgets.
- Translate editor snapshots into Ratatui cells and place the terminal cursor.

There is one event loop, one terminal lifecycle owner, and no `helix-tui` renderer.

The intended dependency direction is:

```text
tui ───► app ───► editor ───► selected low-level Helix crates
tui ───► editor  (read-only rendering snapshots)
```

No lower-level crate should depend on `tui`.

## State ownership

Each piece of state has one authoritative owner.

| State | Owner | Notes |
| --- | --- | --- |
| Focus, overlays, panel visibility | `app` | Focus is independent from normal/insert mode. |
| Explorer entries, expansion, selected row | `app` | Filesystem results arrive through messages. |
| Key contexts and pending application keys | `app` | Only implement sequences required by Eggplant's command set. |
| Documents, active document, dirty state | `editor` | Never maintain a second copy of document text in `app`. |
| Cursor/selection, viewport, undo history | `editor` | Backed by Helix primitives where useful. |
| Normal/insert mode | `editor` | Eggplant-owned mode, not Helix's modal command engine. |
| List/widget state and render caches | `tui` | Only genuinely presentation-specific state. |
| Configuration | Loaded at startup, updated explicitly | Keep app and future AI settings separate from editor internals. |

The prototype's application-level `EditMode` should move into the editor session when real editing is introduced. It should not remain as a second, potentially inconsistent mode value.

## Editing model

The first edition uses a deliberately small, Vim-inspired model:

- Start with **Normal** and **Insert** modes.
- Normal mode handles movement and basic editing commands.
- Insert mode accepts text and returns to Normal on Escape.
- Represent a simple cursor with Helix's selection primitives if useful, without exposing Helix's selection-first interaction model.
- Route all text mutations through editor commands and transactions so undo, dirty tracking, rendering, and future AI edits share one path.
- Add Visual mode, registers, macros, complex text objects, and generic on-next-key behavior only when a concrete product need justifies them.

Exact keybindings should be specified as a small command table before implementation. Avoid inheriting either Helix's or Vim's complete command surface by default.

## Event and update flow

Retain the prototype's Elm-style direction, while separating state transitions from side effects:

```text
terminal / filesystem / background task
                    │
                    ▼
                 Message
                    │
                    ▼
          update(app, message)
             │             │
       changed state     Effects
             │             │
             │       execute outside update
             │             │
             │       completion/result event
             │             │
             │             └──► new Message
             ▼
      render when dirty
```

- **Messages** describe input, something that happened, or a semantic user intent.
- **Updates** change state and return effects; they do not perform terminal or network I/O.
- **Effects** perform work such as reading a directory, opening a file, or saving a document.
- **Completion messages** return results to the single application loop.

The first edition can use a synchronous runtime where that is sufficient. Do not add a complex async runtime solely for anticipated features. When background work becomes necessary, keep one owner of mutable app/editor state and communicate with workers through channels.

Every future asynchronous edit or AI result should carry a document ID and revision so stale results cannot overwrite newer user edits.

## Input routing

Input precedence should be explicit:

1. Normalize the Crossterm key code and modifiers into Eggplant's input type.
2. Give the topmost modal/overlay first opportunity to handle it.
3. Route to the focused explorer or editor.
4. Apply global bindings only as a fallback.
5. When the editor is focused, dispatch according to its Normal/Insert mode; forward unbound insert-mode character input as text.

Common commands use a built-in Space leader and which-key menu rather than the config file: `Space q q` quits, `Space b n/p` switches buffers, and `Space e` shows/hides the explorer (focusing it when shown). The configurable keymap is reserved for frequent direct keys.

The prototype's `HashMap<KeyCode, Message>` can remain until real commands require more. Extend it for modifiers and short sequences when needed rather than importing Helix's complete keymap/modal machinery.

## Rendering

Ratatui rendering is a projection of state, not a source of editing behavior:

- The root page computes layout and delegates to explorer, editor, and status widgets.
- The editor widget reads the active document snapshot and draws only visible rows.
- Selection/cursor styling and terminal cursor placement are derived from editor state.
- Explorer rendering uses the application snapshot and does not read the filesystem during drawing.
- Scrolling and document mutations happen during updates, not as hidden rendering side effects.

Basic editing does not require Helix grammars, queries, themes, or language servers. If a selected Helix document API requires runtime initialization, isolate and report that setup in `editor`; do not silently make syntax tooling a first-edition dependency.

## AI-first extension points

No agent implementation is needed for the first edition, but the architecture should preserve these seams:

- AI features request **semantic application/editor commands**, not direct mutable access to widgets or Helix internals.
- Agent work uses the same **message/effect** path as filesystem and user actions.
- Proposed text changes are transactions associated with a document ID and expected revision.
- Potentially destructive or broad edits can be presented as reviewable diffs before application.
- AI output and tool status are application state that any Ratatui panel can render.

These constraints avoid both a UI-bound agent and an agent that bypasses undo, dirty-state, focus, and user-review behavior.

## Fork repositories

Use these Gitea repositories as the canonical fork locations across development machines:

| Repository | Role |
| --- | --- |
| [Ratatui fork](http://git.z.lan/fork/rataui) | Ratatui source; the repository/directory name is `rataui`, while the Rust package is `ratatui`. |
| [Helix fork](http://git.z.lan/fork/helix) | Helix source, especially `helix-core` and selected document/view APIs. |

Clone the forks wherever appropriate on each machine. Documentation and committed configuration should refer to the repository URLs, not a developer-specific checkout path.

Start with ordinary crate dependencies and use Cargo `[patch.crates-io]` or path dependencies only when actively testing a fork change. Avoid committing machine-specific sibling paths as the long-term dependency strategy. Pin fork revisions once a local change becomes required.

The current Helix checkout requires Rust 1.90 or newer, so selected Helix dependencies set the workspace's effective minimum Rust version. Track upstream revisions and local patches, and respect Ratatui's MIT and Helix's MPL-2.0 licenses.

## First-edition delivery plan

1. **Document/editing spike**
   - Compare `helix-core` plus an Eggplant document wrapper against selective use of `helix-view::Document`/`View`.
   - Prove open, insert, delete, move, undo, dirty tracking, and save without adopting `helix_view::Editor`.
   - Confirm that initialization and dependencies remain appropriate for a basic editor.
2. **Minimal editing UI**
   - Ratatui text widget, visible-line rendering, scrolling, and cursor placement.
   - Normal/Insert modes and the agreed small command table.
   - Clear open/save error reporting and unsaved-change handling.
3. **Explorer workflow**
   - Directory listing, navigation, selection, and opening a file in the editor.
   - Focus switching between explorer and editor.
   - Refresh and filesystem-error states.
4. **First-release hardening**
   - Configuration diagnostics, Unicode/width tests, narrow terminals, resize, and panic-safe terminal restoration.
   - Integration tests for explorer → open → edit → save.
   - Review the command/effect API against future AI use without implementing agent features yet.

## Testing strategy

- **`app` tests:** focus precedence, explorer transitions, mode-aware routing, messages, and effect creation without terminal I/O.
- **`editor` tests:** temporary-file open/edit/save, transactions, movement, undo/redo, dirty state, and revision checks.
- **Rendering tests:** Ratatui `TestBackend` for layouts, visible text, cursor placement, and narrow terminals.
- **Integration tests:** scripted input with a fake effect executor, plus real temporary-directory explorer workflows.
- **Linux smoke tests:** resize, panic restoration, large files, Unicode, permissions, and save failures.

## Development

Run the current prototype and tests with:

```sh
cargo run -p tui
cargo test --workspace
```

The app reads `~/.eggplant/config.toml` when present. See `config.example.toml` for the current options; missing configuration falls back to defaults.

## Next design artifact

Before coding the editor integration, write a one-page **first-edition command table** covering:

- Explorer navigation and file opening.
- Normal-mode movement and basic edits.
- Entering/leaving Insert mode.
- Save, quit, and unsaved-change behavior.
- Focus switching and application-level commands.

That table, together with the document-ownership spike, is enough to begin implementation without accidentally designing a full Vim or Helix clone.
