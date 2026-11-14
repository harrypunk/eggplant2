# Key input, sequences, and hints

This document defines keyboard terminology and keeps terminal input compatible with the Elm-style message/update/effect architecture.

## Default keymap

| Context | Action | Default binding |
| --- | --- | --- |
| Normal / Explorer | Quit | `Space q q` |
| Normal / Explorer | Next / previous buffer | `Space b n` / `Space b p` |
| Normal / Explorer | Show/hide explorer | `Space e` |
| Normal / Explorer | Find/open a file | `Space f f` |
| Normal / Explorer | Select a UI theme | `Space u t` |
| Normal / Explorer | Jump in the visible buffer | `Space c j` |
| Normal / Explorer | Search the current file | `Space s c` |
| Normal / Explorer | Scroll down / up | `Ctrl-F` / `Ctrl-U` |
| Normal / Explorer | Focus left / right | `Ctrl-H` / `Ctrl-L` |
| Normal / Explorer | Focus up / down (reserved) | `Ctrl-K` / `Ctrl-J` |
| Normal | Enter Insert | `i` |
| Normal | Move | `h`, `j`, `k`, `l` |
| Normal | Undo / redo | `u` / `Ctrl-R` |
| Insert | Return to Normal | `Esc` |
| Insert | Backspace / newline | `Backspace` / `Enter` |
| Insert | Move | Arrow keys |
| Explorer | Move selection | `j` / `k` |
| Explorer | Open file / unfold folder | `Enter` |
| Explorer | Fold / unfold folder | `h` / `l` |
| Explorer | Refresh tree | `r` |
| Explorer | Show/hide ignored entries | `Shift-I` |
| File picker | Edit query | Printable input / `Backspace` |
| File picker | Select | `Down` / `Up` |
| File picker | Open / close | `Enter` / `Esc` |
| Theme picker | Select | `Down` / `Up` |
| Theme picker | Apply / close | `Enter` / `Esc` |
| Buffer jump | Enter smart-case two-character pattern, then label | Printable input |
| Buffer jump | Edit input / cancel | `Backspace` / `Esc` |
| Buffer grep | Edit smart-case regex | Printable input / `Backspace` |
| Buffer grep | Select | `Down` / `Up` |
| Buffer grep | Jump to match / close | `Enter` / `Esc` |

Custom `[keymap]` entries merge with defaults by context and action, so only overrides need to be specified. Built-in Space leader sequences are not configured in TOML. Explicit unbinding is not implemented.

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

Direct bindings use one `KeyChord`. `Registry` resolves the chord in the active `Context` and returns a semantic command invocation; for example, Normal `h` becomes `Message::InvokeCommand(CommandId("move_left"))` and is handled by the editing feature.

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

Direct bindings and leader targets share `CommandRegistry`: each stable command ID has metadata and a handler. Resolved input emits `InvokeCommand(CommandId)`; unmigrated commands use compatibility messages, while the theme picker, file picker, buffer switching, editing commands, explorer state commands, lifecycle commands, and focus commands already use feature-owned handlers. Duplicate command registrations and duplicate chords in one input context are startup errors. Later feature migrations replace compatibility handlers without changing keybindings.

The built-in Space leader is already a small data-driven sequence state machine. Each `CommandMenuContext` owns its semantic title and items, and each item targets either a submenu or a semantic `CommandId`. `CommandMenuState` stores the active context stack and returns `Pending`, `Cancelled`, or a command action. `KeyHints` renders the current context's candidates and composes its title by joining active context titles, such as `Leader > buffer`. Titles therefore grow with the menu definitions rather than requiring a separate menu-specific title match.

## Design rules

1. **Normalize once.** Convert terminal-specific events into application key chords at the input boundary. Sequence matching should use complete chords, not only `KeyCode`, so `Ctrl-X` and `x` remain distinct.
2. **Keep pending input in `Model`.** A sequence in progress is application state. It must participate in update, cancellation, focus changes, and rendering like any other state.
3. **Resolve to semantic messages.** Editing, explorer, and application updates should not know which physical keys triggered a command.
4. **Compile bindings at startup.** Parse configured chords or sequences once and report invalid definitions before entering the terminal loop. Do not parse binding strings for every key event.
5. **Make context explicit.** Normal, Insert, Explorer, and future tool contexts have separate bindings and fallback behavior.
6. **Define incomplete and invalid input behavior.** Escape cancels the current leader sequence. Invalid leader input cancels it. Future generalized matching must specify whether an unmatched key is discarded or reprocessed in the base context.
7. **Keep timers outside update.** If disambiguation needs a deadline, return an effect and route expiration back as a message. Never block the event loop waiting for another key.

## Migrating to general key sequences

The direct-key registry can coexist with the hard-coded leader while the command set is small. When more multi-key bindings are needed, replace the special-case routing with one sequence-capable matcher rather than adding more independent state machines.

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
