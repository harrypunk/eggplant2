# Key input, sequences, and hints

This document defines keyboard terminology and keeps terminal input compatible with the Elm-style message/update/effect architecture.

## Default keymap

Bindings are grouped by active context and workflow. A slash separates alternatives or related pairs shown in the action column. `[count]` denotes an optional numeric prefix where supported.

### Application and tools

Available in Normal and Explorer contexts unless noted otherwise.

| Action | Default binding |
| --- | --- |
| Quit | `Space q q` |
| Show/hide explorer | `Space e` |
| Find/open a file | `Space Space` or `Space f f` |
| Select a UI theme | `Space u t` |
| Open inline AI agent | `Ctrl-I` |
| Toggle editor text wrapping | `Space u w` |
| Toggle cursor-line background | `Space u c` |
| Toggle agent tool panel | `Space a t` |
| Open provider login | `Space a i` (`Space a p` alias) |
| Open provider logout | `Space a o` |
| Focus Agent panel input | `Ctrl-L` |
| Return from Agent panel to editor | `Ctrl-H` |
| Go to definition | `Space c d` |
| Find references | `Space c r` |
| Go to implementation | `Space c i` |
| Show diagnostics | `Space c l` |
| Jump in the visible buffer | `Space s j` |
| Search the current file | `Space s c` |
| Search the project | `Space s p` |
| Open full-screen Git status | `Space g g` |

### Shared text inputs

Available in full `TextInput` contexts unless a direct workflow binding takes precedence. Character-only prefix inputs do not receive these editing operations.

| Action | Default binding |
| --- | --- |
| Move to line start | `Ctrl-A` |
| Move to line end | `Ctrl-E` |
| Delete from cursor to line start | `Ctrl-U` |
| Move one scalar left/right | `Left` / `Right` |
| Delete previous scalar | `Backspace` |

### Welcome command menu

When the focused editor has no buffer, it shows the Eggplant logo and command buttons instead of an empty text area. Startup focuses Welcome when no file was opened, including project startup; opening a file focuses the normal editor instead. `Up`/`Down` or `Ctrl-N`/`Ctrl-P` select a button through the shared list-navigation policy, and `Enter` runs its existing command. The selected button uses the theme's focused selection style; it becomes dimmed when focus moves elsewhere. This is a view and selection state, not a document buffer. Welcome input context is derived from page, focus, and buffer state rather than synchronized into the context stack. Command identities are shared with the normal dispatcher so future mouse hit-testing can activate the same targets.

### Buffers

| Context | Action | Default binding |
| --- | --- | --- |
| Normal / Explorer | Next / previous buffer | `Space b n` / `Space b p` |
| Normal | Next / previous buffer | `]b` / `[b` |
| Normal / Explorer | Close current buffer | `Space b c` |
| Normal / Explorer | Close other buffers / buffers to the right | `Space b o` / `Space b r` |
| Normal / Explorer | Close all buffers | `Space b a` |

### Focus and scrolling

Available in Normal and Explorer contexts.

| Action | Default binding |
| --- | --- |
| Focus left / right | `Ctrl-H` / `Ctrl-L` |
| Focus up / down (reserved) | `Ctrl-K` / `Ctrl-J` |
| Scroll down / up | `Ctrl-F` / `Ctrl-U` |

### Normal mode: movement and viewport

| Action | Default binding |
| --- | --- |
| Move cursor | `h`, `j`, `k`, `l` |
| Next word / previous word / word end | `w` / `b` / `e` |
| Line start / line end | `0` / `$` |
| First line / line N | `gg` / `[count]gg` |
| Last line / line N | `G` / `[count]G` |
| Center / top / bottom cursor line | `zz` / `zt` / `zb` |
| Start visible-buffer jump | `s` |

### Normal mode: editing and selection

| Action | Default binding |
| --- | --- |
| Enter Insert | `i` |
| Insert after cursor / line end / first non-blank | `a` / `Shift-A` / `Shift-I` |
| Open indented line below / above | `o` / `Shift-O` |
| Start characterwise / linewise Visual selection | `v` / `V` |
| Delete grapheme / line / word | `x` / `dd` / `dw` |
| Change line / word and enter Insert | `cc` / `cw` |
| Yank current line | `yy` |
| Paste after / before cursor | `p` / `P` |
| Undo / redo | `u` / `Ctrl-R` |

Paste uses the shared register and reads the system clipboard when available.

### Insert mode

| Action | Default binding |
| --- | --- |
| Move cursor | Arrow keys |

### Visual mode

| Action | Default binding |
| --- | --- |
| Extend selection with movement keys | `h`/`j`/`k`/`l`, `w`/`b`/`e`, `0`/`$`, `[count]gg`/`G` |
| Delete selection | `x` or `d` |
| Change selection | `c` |
| Yank selection and return to Normal | `y` |

### Explorer

| Action | Default binding |
| --- | --- |
| Move selection | `j` / `k` |
| Jump to first / last visible entry | `gg` / `G` (no counts) |
| Open file / unfold folder | `Enter` |
| Fold / unfold folder | `h` / `l` |
| Refresh tree | `r` |
| Show/hide ignored entries | `Shift-I` |
| Open keymap help | `?` |
| Add file / directory | `a` |
| Delete file / directory | `d` |
| Move / copy file / directory | `m` / `c` |

The help popup closes with `?` or Escape. Add opens a workspace-relative path prompt prefilled from the selected directory or the selected file's parent. Enter creates a regular file and opens it, or creates and reveals a directory when the path ends in `/`. Missing parent directories are created automatically. Absolute paths, parent traversal, existing targets, and symlink-parent escapes are rejected. Delete asks for `y` confirmation and supports Escape cancellation. Files and links are removed directly; directories are deleted recursively. Affected clean buffers close after success. Move opens a destination field prefilled with the selected path; Enter moves without overwriting, and Escape cancels. Missing destination parents are created, directories cannot move inside themselves, and source buffers follow the same modified/saving rejection and clean close-after-success policy. Copy uses the same destination editor and path safety but preserves the source and leaves all source buffers open, including modified buffers. Both operations copy or move directories recursively and never follow links.

On an expanded directory, `h` folds that directory. On a file or collapsed directory, `h` folds the nearest visible parent and selects it. Root-level entries cannot fold the workspace root.

### File picker

| Action | Default binding |
| --- | --- |
| Next / previous result | `Down` / `Ctrl-N`; `Up` / `Ctrl-P` |

### Theme picker

| Action | Default binding |
| --- | --- |
| Next / previous theme | `Down` / `Ctrl-N`; `Up` / `Ctrl-P` |

### Buffer jump

| Action | Default binding |
| --- | --- |
| Enter smart-case pattern, then label | Printable input |

### Agent dialog

| Action | Default binding |
| --- | --- |
| Open inline Agent dialog | `Ctrl-I` |
| Submit prompt | `Enter` |
| Dismiss dialog without cancelling the run | `Esc` |
| Edit prompt / delete previous character | Printable input / `Backspace` |
| Interrupt the active run | `Ctrl-F7` |
| Toggle the shared Agent tool panel | `Space a t` |
| Focus the Agent panel input | `Ctrl-L` |
| Return from the Agent panel to the editor | `Ctrl-H` |

The inline dialog requires an open project but no active buffer. An active workspace file supplies an editable/deletable `workspace/relative/path:line` prefill; otherwise the prompt starts empty. External buffers do not supply prefill and do not prevent chatting. Dismissal preserves the run and conversation. The project panel uses the same project requirement and no-buffer availability, accepts freeform text and `@path` references, and focuses its bottom input when opened. The fake provider streams a simulated response for this first slice; no credentials are needed.

Panel input starts in Normal mode: `i` enters Insert, `?` opens a modal help popup, the configured leader opens the shared command menu, and `Ctrl-H`/`Ctrl-L` move focus. The help popup closes with `?` or `Esc` without activating the panel underneath. Insert accepts text (including Space), Backspace edits, Enter submits, and `Esc` returns to Normal without leaving the panel.

Agent slash commands:

- A leading `/` opens the filtered command list above the prompt-active Agent input.
- Enter executes a selected command or an exact typed command after trimming whitespace; incomplete and unknown commands remain unchanged.
- Tab quick-fills the first match; Ctrl-N/Ctrl-P move selection with wrapping and quick-fill each command.
- `/provider` opens provider login and restores the originating Agent input when closed.
- `/model` opens the grouped model selector; `/logout` opens provider logout; `/thinking` remains a placeholder.

Provider login:

- Up/Down or Ctrl-N/Ctrl-P navigate menus; Enter opens the selected page; Escape steps back before closing.
- Account lists `openai`, `qwen.com`, and `kimi.com` as placeholders.
- Compatible API-key providers and their endpoints are listed in [AI agent architecture](agent.md#openai-compatible-forms).
- API-key drafts use Backspace and Left/Right editing; Enter saves through the configured credential store.
- Compatible-provider forms have wrappable Endpoint and API-key fields. Tab loops between fields, and Ctrl-V pastes through `ClipboardReader`.
- Clipboard completions are discarded after focus or edits change, preventing stale pastes.
- Durable saves use the default SQLite database at `~/.eggplant/ai/data.sqlite`; an absolute `session_db` can override it.
- `/model` opens grouped cached catalogs for all logged-in compatible providers and refreshes each in the background; Up/Down or Ctrl-N/Ctrl-P navigate across provider sections and Enter activates one provider/model identity.
- After a compatible-provider save, its endpoint returns available models. Up/Down or Ctrl-N/Ctrl-P navigate the catalog and Enter activates the selected model.
- Catalog failures preserve the form for retry. Activation persists the selected model and restores it on the next startup.
- Other providers can store keys independently without changing the active provider or model.

Provider logout:

- `Space a o` or `/logout` lists stored providers; empty storage produces a notice instead of a blank menu.
- Up/Down and Ctrl-N/Ctrl-P navigate; Enter deletes the selected credential; Escape closes and ignores late completions.
- Removing the active provider clears the runtime selection without switching implicitly. Other credentials remain unchanged.

The focus controller itself remains geometry-only; keyboard maps decide which mode emits focus commands.

### Agent write approval

| Action | Default binding |
| --- | --- |
| Approve one immutable exact-text write | `y` |
| Reject the write and resume the model | `n` |
| Hide the unchanged approval without resolving it | `Esc` |
| Cancel the run without applying or resuming the write | `Ctrl-F7` |

Approval mode has no printable prompt input and no cursor. Reopening with `Ctrl-I` restores the pending approval. Revision/exact-text conflicts return an error to the model without changing user text.

### Current-buffer grep

| Action | Default binding |
| --- | --- |
| Next / previous result | `Down` / `Ctrl-N`; `Up` / `Ctrl-P` |

### Project grep

| Action | Default binding |
| --- | --- |
| Next / previous result | `Down` / `Ctrl-N`; `Up` / `Ctrl-P` |

#### Shared picker and search navigation

File/theme pickers and both live-grep lists wrap from first to last and last to first. Their rendered list window follows the selection so the selected row remains visible, including after wraparound and in small terminals.

`Ctrl-N`/`Ctrl-P` are built-in aliases for Down/Up navigation in those pickers and in the VCS branch, merge, and reset selectors. Only contexts explicitly registered for list navigation compose these aliases, including both Agent inputs; ordinary editor input does not inherit them. An explicit `Ctrl-N`/`Ctrl-P` binding for a custom action in the same context takes priority.

### VCS status page

| Action | Default binding |
| --- | --- |
| Select previous / next changed file | `k` / `j` |
| Open selected changed file | `Enter` |
| Stage / unstage selected file | `s` / `u` |
| Stage all / unstage all | `Ctrl-S` / `U` |
| Toggle inline diff for selected file | `Tab` |
| Open history page | `l` |
| Open commit-message page for staged changes | `c` |
| Open bottom branch actions | `b` |
| Open switch-and-fast-forward picker (requires clean status) | `m` |
| Open reset actions | `r` |
| Discard unstaged changes in the selected tracked file | `x` |

### VCS soft reset

| Action | Default binding |
| --- | --- |
| Open soft-reset target picker | `r`, then `s` |
| Filter targets / enter a commit hash | Printable characters |
| Select target / submit / cancel | `Down`/`Up` or `Ctrl-N`/`Ctrl-P`; `Enter`; `Escape` |

The picker lists local `main`/`master`, existing remote-tracking branches, and the newest five commits from `HEAD`. No fetch is performed. A hexadecimal query of at least four characters is submitted as a commit hash. Soft reset moves `HEAD` or its current branch to the resolved commit without changing the index or working tree; differences from the selected target consequently appear in staged/unstaged status as appropriate.

### VCS branch menu

| Action | Default binding |
| --- | --- |
| Create new branch / switch existing branch | `c` / `s` |

### VCS branch selector

| Action | Default binding |
| --- | --- |
| Filter branches or enter a short/full commit start point | Printable characters |
| Select next / previous start point or branch | `Down` / `Up` or `Ctrl-N` / `Ctrl-P` |
| Accept selection or submit new branch name | `Enter` |

### VCS merge selector

| Action | Default binding |
| --- | --- |
| Type to filter branches | Printable characters |
| Select next / previous branch | `Down` / `Up` or `Ctrl-N` / `Ctrl-P` |
| Submit selected target | `Enter` |

### VCS history and commit detail

| Context | Action | Default binding |
| --- | --- | --- |
| History | Select previous / next commit | `k` / `j` |
| History / detail | Jump to first / last commit or detail file | `gg` / `G` (no counts) |
| History / detail | Toggle commit files / selected detail-file diff | `Tab` |
| History | Open selected commit's right-side detail | `Enter` |
| Ready commit detail | Open selected working-tree file and close history | `Enter` |
| Ready commit detail | Copy short / full hash to system clipboard | `c` / `C` |
| History | Load the next 20 commits when available | `n` |

### VCS commit-message editor

| Action | Default binding |
| --- | --- |
| Commit staged changes from Normal or Insert | `Ctrl-S` |
| Close in Normal mode and preserve the draft | `q` |
| Discard the draft from Normal or Insert | `Ctrl-C` |
| Normal movement | `h`/`j`/`k`/`l`, `w`/`b`/`e`, `0`/`$` |
| Normal insert variants | `a`/`A`/`I`, `o`/`O` |
| Delete grapheme / line / word | `x` / `dd` / `dw` |
| Change line / word and enter Insert | `cc` / `cw` |
| Yank current line / paste after / paste before | `yy` / `p` / `P` |
| Undo / redo | `u` / `Ctrl-R` |

## Common input behavior

Enter accepts, opens, or submits; arrows navigate lists or move the insertion cursor; Escape cancels or closes; Backspace edits single-line input; `y` confirms destructive Explorer deletion. These common behaviors are fixed in input routing and cannot be overridden through configuration.

Every single-line popup `TextInput` supports `Left`/`Right` movement at Unicode-scalar boundaries, including insertion and Backspace at the current cursor. `Up`/`Down` remains list navigation. Normal/Insert editor buffers and the VCS commit editor retain their existing Helix cursor behavior. `Space u w` independently toggles visual wrapping for the main editor viewport; popup inputs keep their own single-line or wrapped layout policy.

## Configuration

Custom `[keymap]` entries merge with defaults for application-specific actions, so only overrides need to be specified. Common keys and confirmation/cancellation are not configurable. The root `leader` setting configures the opening chord (`leader = "space"` by default); leader submenu entries remain built-in. Explicit unbinding is not implemented.

See [`config.example.toml`](../config.example.toml) for every configurable action name and its default binding.

## Terminology

| Term | Meaning | Example |
| --- | --- | --- |
| **Key chord** | One key press plus its modifiers. | `Ctrl-L`, `Shift-I`, `Space` |
| **Key sequence** | An ordered list of key chords resolved to one command. | `Space q q` |
| **Leader** | A prefix that opens a group of common commands. It is not the popup or the keymap. | `Space` |
| **Keymap** | The configured or built-in mapping from input to semantic commands. | Normal `h` → cursor left |
| **Key hints** | Contextual candidates for the currently pending sequence. | The bottom-right `Key Hints` popup |
| **Keymap reference** | A future complete, searchable list of commands and bindings, comparable to a full which-key UI. | Search all buffer commands |

The current `KeyHints` component deliberately does not claim to be the complete keymap reference. Both UIs should eventually read the same keymap metadata, but they have different interaction models.

## Current single-key flow

Direct bindings use one `KeyChord`. `Registry` resolves the chord in the active `Context` and returns an `InputAction`. Feature-specific commands become `Message::InvokeCommand(CommandId)`; common modal actions become `Message::GeneralAction`. For example, Normal `h` becomes `Message::InvokeCommand(CommandId("move_left"))`, while a picker Down becomes `Message::GeneralAction(GeneralAction::Next)`.

```text
Crossterm Event::Key
        │
        ▼
normalize KeyCode + modifiers → KeyChord
        │
        ▼
active input Context + Registry
        │
        ├── direct binding ──────────────► semantic Message
        ├── leader start/continuation ───► Message::CommandMenuKey
        └── registered text input ───────► Message::TextInput
                                                │
                                                ▼
                                  MessageManager::update(Model, Message)
                                                │
                                  ┌─────────────┴─────────────┐
                                  ▼                           ▼
                            changed Model               optional Effect
                                  │                           │
                                  ▼                           ▼
                            page/components          completion Message
```

Input translation is pure. It does not change the model, execute commands, sleep, or perform I/O. Only `MessageManager::update` changes application state. Rendering reads the resulting model and never resolves commands itself.

Direct bindings and leader targets share `CommandRegistry`: each stable command ID has metadata and a handler. Resolved keyboard input produces a `RoutedInput { context, message }`, preserving the context that selected the binding. `TranslatedEvent` keeps this context-owned input separate from system messages such as resize and focus loss. Resolved commands emit `InvokeCommand(CommandId)`; unmigrated commands use compatibility messages, while the theme picker, file picker, buffer switching, editing commands, explorer state commands, lifecycle commands, and focus commands already use feature-owned handlers. Duplicate command registrations and duplicate chords in one input context are startup errors. Later feature migrations replace compatibility handlers without changing keybindings.

All 37 input contexts use feature-owned `ContextKeymap` values assembled into a `KeymapCatalog` at `composition/keymaps.rs`. Each declaration owns its stable configuration name, typed `Context`, direct commands, and semantic capabilities such as `ListNavigation`, `ModalActions`, `LeaderMenu`, or `FormActions`. The legacy `common_bindings`, central membership lists, and string-to-context parser are gone. `Registry` validates merges and uses a runtime hash map for lookup, but does not own catalog membership. Duplicate feature contexts are startup errors, and custom commands merge without losing direct-binding precedence. The shell-owned Normal declaration remains empty because its direct defaults already come from `Config::default()`; VCS and Explorer declarations likewise merge with their configured defaults.

Common modal keys follow a separate publish/subscribe path. `GeneralAction::{Next, Previous, Activate, Back}` carries semantic behavior without a feature prefix. `GeneralActionDispatcher` selects a registered subscription from `RoutedInput.context`, and that subscriber operates directly on its workflow instead of translating the action back into a feature command ID or rediscovering the context through `Model`. Features own their subscriptions; `composition/actions.rs` assembles them without giving the dispatcher concrete feature dependencies. This lets file picker, theme picker, grep, Agent, and provider-login workflows share Down/Up, Ctrl-N/P, Enter, and Escape without adding `yet_another_next` bindings. Custom direct commands retain their own command IDs and bindings.

Form-capable contexts compose `KeymapCapability::FormActions` to add the shared `FocusNext` (Tab) and `Paste` (Ctrl-V) general actions. Their chord definitions live in `manager/form_input.rs`, outside any feature keymap. ProviderLogin opts into this capability and handles those actions through its existing subscription; unrelated handlers explicitly ignore form-only actions. A future form can reuse the capability without defining provider-style focus or paste commands.

Backspace does not participate in general actions. It remains a semantic text-editing event through `TextInputRegistry`, alongside character insertion and cursor movement. The registry also receives `RoutedInput.context` explicitly. `MessageManager::update` retains a compatibility path for direct semantic-message callers and tests; terminal input uses `update_input` so the originally captured context is authoritative throughout dispatch.

The configured leader (`Space` by default) opens a small data-driven sequence state machine. `EventManager` validates the configured chord once, converts terminal events to `KeyChord`, and delegates start/pending decisions to `manager::context::leader_input`. `CommandMenuStart` opens the root without the menu knowing the physical leader chord; `CommandMenuKey(KeyChord)` continues an active sequence. Each `CommandMenuContext` owns its semantic title and items, and each item targets either a submenu or a semantic `CommandId`. `KeyHints` renders the current context's candidates and composes its title by joining active context titles, such as `Leader > buffer`.

Contexts opt into opening the menu through `KeymapCapability::LeaderMenu`; `EventManager` collects those contexts from the catalog instead of using a central feature-membership list. Normal, Welcome, Explorer, and Agent panel Normal currently opt in. Insert contexts do not, so Space remains ordinary text. Once a menu is active, its pending sequence takes precedence independently of the originating context.

## Context ownership

`InputRouting` resolves input in three layers at the composition root. `ApplicationInputRouting` gives the active `ModalController` session priority, followed by the top of `context_stack`, then delegates to the active page. `EventManager` and `MessageManager` receive this router through the same injected interface; the manager layer no longer owns page, focus, buffer, or tool policy.

A modal session owns both presentation and input identity through `ModalController::active_context`. Every `ModalController` workflow uses this single owner without duplicating its context in the stack. Agent dialog ↔ approval uses `ModalController::replace` to change phases without releasing input ownership.

The stack provides transient LIFO routing underneath the active modal only for genuine stack-owned workflows: `BufferNavigation`, `LineJump`, `ViewAlign`, `EditorChange`, `EditorYank`, `VcsResetMenu`, and `VcsBranchMenu`. The VCS menu entries are page-local menus, not `ModalController` popups; transitioning from one to its popup removes the menu entry before acquiring the modal. Push a stack-owned workflow's context on entry and pop that same context on exit. `ContextStack` owns the LIFO mechanics and `pop_if` removes only a matching top frame; it never removes an older workflow or synchronizes shell state. Features consume explicit `RoutedInput`, their own workflow state, or narrow owner policies such as the editing target's Normal-input availability. There is no global `input_context` compatibility function. Composition adapters may resolve route-origin metadata through `InputRouting`.

Page policy is feature-owned rather than table-driven in the manager. The Editor page selects its focused region and derives Welcome from buffer availability. A pure editing mapper derives Insert/Visual from `EditMode`. `ToolsPanelState` delegates to the active tool, and `AgentState` supplies panel Normal/Insert. VCS and history expose static page contexts, while the commit page derives routing from its own `EditorSession`. Only the composition root selects among these page policies. `ModalController` is the authority for modal visibility and modal input identity.

Shell and editor-mode contexts are derived, not pushed: `AppNormal`, `Welcome`, `EditorInput`, `EditorVisual`, `VcsCommitInput`, `Explorer`, `AgentChat`, `AgentChatInsert`, `VCS`, `VcsHistory`, and `VcsCommit`. Changing focus, opening or closing the last buffer, switching pages, changing editor mode, or changing Agent panel mode must not synchronize these variants into the stack. Direct editor-session transitions therefore cannot leave stale Insert or Visual frames. Workflow-specific restoration data, such as the file picker's opening focus, belongs to that workflow rather than being encoded as a shell stack entry.

## Design rules

1. **Normalize once.** Convert terminal-specific events into application key chords at the input boundary. Sequence matching should use complete chords, not only `KeyCode`, so `Ctrl-X` and `x` remain distinct. Text inputs use semantic `Insert`, `DeleteBackward`, `DeleteToLineStart`, and `Cursor` events carried by `Message::TextInput`. `EventManager` owns one ordered `InputResolverChain`; `DirectInputResolver` precedes `TextInputResolver`, which composes `TextInputKeymap` with `TextInputRegistry`. `TextInputRegistry::builtin` is the single checked registration table: character-only prefix parsers accept `Insert`, while editable components accept every event and react to the actual content-change result. Features do not register per-feature editing commands or character-message dispatcher arms.
2. **Keep pending input in `Model`.** A sequence in progress is application state. It must participate in update, cancellation, focus changes, and rendering like any other state.
3. **Resolve to semantic messages.** Editing, explorer, and application updates should not know which physical keys triggered a command.
4. **Compile bindings at startup.** Parse configured chords or sequences once and report invalid definitions before entering the terminal loop. Do not parse binding strings for every key event.
5. **Make context explicit.** Normal, Insert, Explorer, and future tool contexts have separate bindings and fallback behavior.
6. **Define incomplete and invalid input behavior.** Escape cancels the current leader sequence. Invalid leader input cancels it. Future generalized matching must specify whether an unmatched key is discarded or reprocessed in the base context.
7. **Keep timers outside update.** If disambiguation needs a deadline, return an effect and route expiration back as a message. Never block the event loop waiting for another key.

## Migrating to general key sequences

The direct-key registry coexists with the configured leader state machine while the command set is small. When more multi-key bindings are needed, replace the special-case routing with one sequence-capable matcher rather than adding more independent state machines.

A conceptual resolver API is:

```rust
enum SequenceResolution {
    Matched(Message),
    Pending,
    NoMatch,
}
```

For each key chord, the resolver considers the active context and any `PendingSequence` stored in the model:

- `Matched(message)` clears pending input and feeds `message` through the normal update path.
- `Pending` stores the matched prefix and exposes candidates for `KeyHints`.
- `NoMatch` clears the prefix and applies the context's documented fallback policy.

The model might contain:

```rust
struct PendingSequence {
    context: Context,
    chords: Vec<KeyChord>,
    generation: u64,
}
```

A small vector of bindings with prefix checks is sufficient initially. A trie becomes useful when binding count or lookup cost justifies it; it is not a prerequisite for correct sequence semantics. The important boundary is that matching produces a resolution, while update owns the resulting state transition.

Direct one-chord bindings can remain a fast path internally, but they should share the same context and command metadata as sequences. This also allows the future searchable keymap reference to list direct and multi-key commands together.

## Double-press prefixes

A keyboard "double click" is better described as a **double press**. Terminals normally deliver two independent key events; there is no universal double-key event.

A double-backslash leader is therefore a two-chord sequence:

```text
first \  → Pending
second \ → Matched(leader/start-hints)
```

If a single `\` is also bound, the first press is ambiguous. The resolver must wait to distinguish the single command from the double press:

1. Store the pending first chord and return a timeout effect.
2. A second `\` completes the double-press sequence and invalidates the pending timeout.
3. Timeout emits `Message::SequenceTimedOut { generation }`.
4. Update accepts the timeout only if its generation still matches the pending sequence, then runs the single-key command or opens the pending leader hints according to the chosen UX.
5. A different key follows the documented cancellation/fallback rule and invalidates the timeout.

Generation checks prevent an old timeout from completing or cancelling a newer sequence. If single `\` has no binding, no timer is needed: the first press can remain pending until the second press, Escape, or another sequence-policy action.

This timing mechanism is specific to input disambiguation and is unrelated to auto-save, which remains always-on and has no debounce configuration.

## Testing expectations

Key input tests should cover:

- Chord parsing, modifiers, shifted characters, and invalid definitions.
- Direct binding resolution in every supported context.
- Pending prefixes, completed sequences, Escape, and invalid continuations.
- Insert-mode printable fallback without treating control chords as text.
- Context isolation and leader/popup precedence.
- Repeated-key sequences and, when needed, timeout and stale-generation behavior.
- Hint contents, intrinsic size, bottom-right placement, and narrow-terminal clamping.
