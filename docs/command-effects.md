# Command and effect routing

This document details the semantic command platform and background completion boundaries introduced by the feature architecture.

## Command platform

A command is a stable semantic identity plus metadata and a handler, for example:

```text
editor.undo
buffer.next
buffer.previous
file.open
file.find
ui.theme.select
```

Configured direct chords and built-in leader sequences resolve the same command ID:

```text
"ctrl-r" in Normal ──► editor.undo ──► registered undo handler
Space u t ───────────► ui.theme.select ──► registered theme-picker handler
```

The command registry owns:

- Unique command IDs and duplicate-registration errors.
- Human-readable descriptions used by leader hints and future command search.
- Handler invocation through a narrow command context.
- Keybinding conflict diagnostics at startup.
- Built-in leader menu contributions without a separate hardcoded action-to-message match.

The built-in Space leader remains fixed product behavior; arbitrary leader sequences are not user-configurable. Menu entries and direct bindings share command resolution instead of maintaining two command tables.

Exact handler signatures should remain narrow. Do not use a generic command bus carrying arbitrary strings and untyped payloads throughout the application.

## Effects and completion routing

Updates remain pure. A feature returns an effect request; the runtime locates the registered handler and executes it with the appropriate scheduling semantics.

| Work | Required behavior |
| --- | --- |
| Save | Async owned future; stable `BufferId` and saved revision. |
| Picker scan/filter/preview | Blocking pool; feature revision and latest-result checks. |
| Document open | Blocking read/syntax preparation; generated `OpenRequestId` carried through the TUI request/completion envelope; main-thread construction of non-`Send` Helix `Document`. |
| Theme selection | Pure update; no effect. |
| Dynamic theme discovery/load | Filesystem effect with request ID, complete validation, and previous-theme preservation on failure. |

An effect registration connects request execution to completion routing. The runtime should not need a new domain-specific match arm for every feature, but it must distinguish async, blocking, and main-thread requirements rather than treating all effects as arbitrary closures.

Effect handlers may use shared capabilities, but they must not capture mutable application state or bypass editor invariants. Unknown effect kinds and missing handlers are startup/registration errors, not silent no-ops.

## Message categories

1. **Command invocations** represent user or automation intent and use stable `CommandId`s.
2. **Feature messages/completions** represent internal state transitions and worker results. These can remain feature-specific enums during the internal edition.

The shell routes a completion to its owning feature without understanding the domain payload. Stable request IDs, buffer IDs, and feature revisions reject stale results. Feature-owned completions such as `FilePickerCompletion`, `ProjectGrepCompletion`, and `BufferGrepCompletion` apply directly to their feature state with explicit inputs; the file picker additionally needs only an optional workspace root, while buffer search receives the active buffer identity. Picker-specific completion variants are absent from the global `Message` enum and shell coordinator. Dynamic type erasure is not required until static feature composition becomes an actual external-interface problem.
