# Eggplant Editor

Eggplant is an **AI-first, keyboard-driven terminal editor** written in Rust. Its long-term direction is an editor where exploration, editing, and AI-assisted workflows share the same application commands rather than being separate, tightly coupled features.

The first edition deliberately stays small:

- Linux only.
- A file explorer and basic text editing.
- A simplified, Vim-inspired interaction model.
- [Ratatui](http://git.z.lan/fork/rataui) for the terminal UI.
- [Helix](http://git.z.lan/fork/helix) only for low-level text, document, and editing primitives.

The running first-edition application supports workspace exploration, multiple file buffers, Normal/Insert editing, revision-safe automatic saving, fuzzy file opening with previews, and built-in UI themes. The `tui` crate contains an Elm-style event/update/view loop, application coordination, and Ratatui components; the `editor` crate owns document and editing state.

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

The workspace has three logical responsibilities provided by two crates: `editor` and `tui`, with application coordination currently inside `tui`.

```text
crates/
├── app/       # Application state, explorer, messages, effects, and input routing
├── editor/    # Eggplant-owned editing model and narrow Helix adapter
└── tui/       # Binary, terminal runtime, Ratatui pages, and widgets
```

Application state currently lives in `tui`. Avoid adding more crates until a real dependency or testing boundary requires them.

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

`EditorSession` owns the single authoritative `EditMode`. Application focus and input contexts remain separate, so focus loss or panel changes cannot silently create a second mode value.

## Editing model

The first edition uses a deliberately small, Vim-inspired model:

- Start with **Normal** and **Insert** modes.
- Normal mode handles movement and basic editing commands.
- Insert mode accepts text and returns to Normal on Escape.
- Represent a simple cursor with Helix's selection primitives if useful, without exposing Helix's selection-first interaction model.
- Route all text mutations through editor commands and transactions so undo, dirty tracking, rendering, and future AI edits share one path.
- Add Visual mode, registers, macros, complex text objects, and generic on-next-key behavior only when a concrete product need justifies them.

The first-edition command table lives in `PLAN.md`, with configurable direct bindings in `config.example.toml` and built-in leader sequences documented in `docs/key-input.md`. Avoid inheriting either Helix's or Vim's complete command surface by default.

## Event and update flow

The application uses an Elm-style flow that separates state transitions from side effects:

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
             view(&Model)
```

- **Messages** describe input, something that happened, or a semantic user intent.
- **Updates** change state and return effects; they do not perform terminal or network I/O.
- **Effects** perform work such as reading a directory, opening a file, or saving a document.
- **Completion messages** return results to the single application loop.

The runtime uses Tokio for asynchronous saves and blocking document/picker work. Mutable application/editor state remains on the main update loop; workers communicate through completion messages and channels rather than shared mutable state.

Every future asynchronous edit or AI result should carry a document ID and revision so stale results cannot overwrite newer user edits.

## Input routing

Input is context-aware: overlays handle input first, followed by the focused feature and its Normal/Insert mode. Direct bindings are configurable and merge with defaults by context and action.

| Common action | Default binding |
| --- | --- |
| Quit | `Space q q` |
| Next / previous buffer | `Space b n` / `Space b p` |
| Show/hide explorer | `Space e` |
| Find/open a file | `Space f f` |
| Select a theme | `Space u t` |
| Jump in the visible buffer | `Space c j`, two characters, then a label |
| Search the current file | `Space s c` |
| Scroll down / up | `Ctrl-F` / `Ctrl-U` |
| Focus left / right | `Ctrl-H` / `Ctrl-L` |

See [Key input, sequences, and hints](docs/key-input.md) for the complete editing, explorer, and popup keymap table. Page-scroll uses `[features.page_scroll] step` (default `75%`).

Only regular UTF-8 text files open successfully. The explorer honors root/nested `.gitignore` files and also hides `.git`, `node_modules`, and `__pycache__` by default.

## Rendering

Ratatui rendering is a projection of state, not a source of editing behavior:

- The root page computes layout and delegates to explorer, editor, and status widgets.
- The editor widget reads the active document snapshot and draws only visible rows.
- Selection/cursor styling and terminal cursor placement are derived from editor state.
- Explorer rendering uses the application snapshot and does not read the filesystem during drawing.
- Scrolling and document mutations happen during updates, not as hidden rendering side effects.

Syntax highlighting uses Helix grammars and queries through the shared `DocumentFactory`. Snapshots retain exact, layered Helix highlights. The TUI resolves semantic UI and syntax styles through built-in themes: `default`, `tokyonight`, and `catppuccin_mocha`. The latter two use unmodified Helix-compatible TOML resources with retained source notices. A Helix-compatible grammar runtime enables language highlighting; files still open as plain text when it is unavailable.

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

Run the application and tests with:

```sh
cargo run -p tui
cargo test --workspace
```

The app reads `~/.eggplant/config.toml` when present. See `config.example.toml` for the current options; missing configuration falls back to defaults. Custom keymap entries merge with built-in defaults by context and action, so only overrides need to be specified.

## Current design documents

- `docs/editor-design.md`: implemented editor, viewport, save, and automation boundaries.
- `docs/feature-architecture.md`: second-edition feature ownership and command/effect/UI contribution contracts.
- `docs/feature-authoring.md`: feature registration and contribution checklist.
- `docs/key-input.md`: direct chords, leader sequences, and popup contexts.
- `docs/filesystem.md`: opening, previews, background work, and mutation rules.
- `docs/theme.md`: resolved themes, Helix-compatible resources, and theme selection.
- `docs/smoke-test.md`: automated checks and the real-terminal Linux smoke procedure.
