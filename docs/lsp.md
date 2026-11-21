# Lightweight language services

Eggplant's LSP support is intentionally limited to source navigation and diagnostics. It is not a full IDE integration.

## Scope

- Go to definition, references, and implementation.
- Full-text document open/change/close synchronization.
- Error, warning, information, and hint diagnostics.
- Server messages and work-progress begin/report/end notifications.
- One workspace root and ordered providers for the current file type.

At startup Eggplant loads `~/.eggplant/languages.toml`; if it is absent, community defaults configure external `rust-analyzer`, `typescript-language-server` (TS/TSX/JS/JSX), and `pyright-langserver` commands. A present file replaces those defaults and uses owned specifications through the injected `LanguageServerRegistry` interface. See [External language-server configuration](language-configuration.md) for the supported format. No server binaries are bundled or downloaded automatically. Unsupported files and unavailable servers leave ordinary editing intact and report feedback through notifications.

Non-goals include completion, hover, rename, code actions, formatting, semantic tokens, workspace symbols, multiple workspace folders, incremental document synchronization, bundled server executables, and an exhaustive language catalog.

## Ownership

| Module | Responsibility |
| --- | --- |
| `app::language` | UI-neutral requests, locations, diagnostics, document events, owned server metadata, the `LanguageServerRegistry` lookup interface, and the `LanguageService` port. |
| `app::lsp` | Stdio framing, JSON-RPC envelopes, navigation/diagnostics mapping, process lifecycle, and the split runtime. |
| `editor` | Buffer identity, text, revisions, logical cursor positions, and document opening. It has no LSP dependency. |
| `capabilities/document` | Generic document opening and the injected `DocumentCloseHandler` port; shell lifecycle has no language-service dependency. |
| `features/code_navigation` | Latest-request state, service ownership, command handling, effect completion, and the shared location popup. |
| `features/diagnostics` | Diagnostic storage, stale-version checks, typed completions, and side-panel visibility. |
| `component/diagnostics_panel` | Severity/path/position/message rendering without protocol or process knowledge. |

The runtime splits process reading from writing. A reader thread routes diagnostics, server messages, and work progress to an event callback and other messages to navigation. Document-map locks cover only short lookups, never blocking transport I/O. The child is terminated/reaped and the reader/writer threads are joined during teardown.

Server-initiated requests receive minimal responses rather than being dropped: capability registration, progress creation, refresh requests, configuration items, and the fixed workspace folder are acknowledged. Workspace edit requests are explicitly rejected, and unknown methods receive JSON-RPC `MethodNotFound`.

## Commands and UX

| Action | Binding |
| --- | --- |
| Definition | `Space c d` |
| References | `Space c r` |
| Implementation | `Space c i` |
| Diagnostics panel | `Space c l` |

Navigation uses the first configured provider, so primary navigation servers should precede lint-only providers. Navigation results behave as follows:

- No locations: a transient notification.
- One location: open it through the existing document capability.
- Multiple locations: one shared popup with Up/Down, Enter, and Escape.

Locations outside the fixed workspace open as read-only buffers and show `[read-only]` in the status bar. Cursor movement, visual selection, yanking, and further navigation remain available; insert, delete/change, undo/redo, and paste are rejected. Returning to a workspace buffer restores normal editing. Opening a standalone file without a workspace keeps the existing editable behavior.

The diagnostics panel is non-modal and does not move editor focus. It displays severity, workspace-relative path, line/column, and message. It is shown on demand rather than opening automatically whenever diagnostics arrive.

Transient server information, missing results, startup failures, and protocol failures use the existing notification path. Starting/restarting, successful initialization, and startup/runtime failures produce non-modal `LSP` notifications. Server `window/showMessage` notifications also appear there with error, warning, or information severity. `$/progress` begin/report/end events show the task title, optional details, and percentage; report/end events reuse the title registered by begin. Lifecycle and progress feedback are transient, with no persistent progress panel.

## Synchronization and stale safety

- Synchronization uses owned full-text events with document versions.
- Opening a supported document starts or reuses every configured provider; ordinary edits synchronize current text with all of them.
- Services are retained by stable server ID, with one worker operation active across providers. Queue entries retain only the newest text per provider/path and never coalesce close events away.
- Successful buffer closes send `didClose` to existing providers and clear the path's diagnostics through the generic document-close port.
- Navigation completions require the latest request ID plus matching buffer identity and revision.
- Diagnostic batches carry provider ID, path, optional version, and provider generation. Older document versions and old-server batches are ignored.
- Empty diagnostic batches clear only that provider/path contribution without reading document text or erasing other providers.
- Terminal reader/transport failures invalidate only that provider and clear its old diagnostics; the next edit or navigation starts a replacement. Failures from older generations are ignored, while malformed individual notifications report an error without invalidating a live service.
- LSP positions are converted between UTF-16 code units and editor character offsets, including non-ASCII text.

There is no application-level request cancellation yet. Stale results are discarded rather than cancelled on the server.

## Validation

Automated coverage uses fake services and fake stdio servers. No test requires `rust-analyzer` to be installed.

For a real-terminal check, create a small Cargo project with a function definition and a call, then open it with `rust-analyzer` on `PATH`:

1. Use `Space c d` on the call and confirm the definition opens.
2. Use `Space c r` on a symbol with multiple references and confirm the shared location popup opens the selected result.
3. Introduce a compile error and warning, then use `Space c l` to inspect them in the side panel.
4. Edit and close documents and confirm diagnostics update or clear without stale entries.
5. Remove the server from `PATH` and confirm an ordinary notification rather than a panic or loss of editing behavior.
