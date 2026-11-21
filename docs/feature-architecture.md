# Feature architecture

This document defines the high-level ownership and dependency rules for Eggplant features. It is not a milestone log or a feature-specific implementation guide.

Detailed contracts live in:

- [Command and effect routing](command-effects.md)
- [UI contributions](ui-contributions.md)
- [Milestone history](milestone-history.md)
- [VCS addon](vcs.md)
- [Global `Model` coupling audit and incremental refactor](global-model-refactor.md)

## 1. Purpose

The architecture keeps feature growth local and reviewable without destabilizing editing, persistence, filesystem, rendering, or terminal behavior. It is an **internal composition architecture**, not a third-party plugin platform.

The main goals are to:

- Add a feature through its own module and explicit composition-root registration.
- Keep unrelated shell code and other features unchanged.
- Give commands stable identities usable by keymaps, leader menus, tests, and future automation.
- Keep updates pure and route I/O through registered effects and completion messages.
- Preserve explicit ownership of state, focus, modal precedence, layout, editor safety, and terminal lifecycle.

## 2. Non-goals

- No native Rust dynamic-library ABI or unrestricted in-process third-party code.
- No WebAssembly/runtime plugin host in this edition.
- No global service locator, singleton registries, or arbitrary `HashMap<TypeId, Box<dyn Any>>` application state.
- No universal `Feature` trait with dozens of lifecycle and rendering methods.
- No big-bang rewrite of editor, explorer, picker, themes, and keymaps.
- No speculative third-party agent/plugin API. Internal agent boundaries are documented in [AI agent architecture](agent.md) and [Agent tools](agent-tools.md), then implemented only through concrete reviewed slices.

One registration per feature is allowed and desirable. “Open for extension” means unrelated logic stays closed; it does not mean pretending that static Rust composition requires zero wiring.

## 3. Responsibility boundaries

| Responsibility | Owner |
| --- | --- |
| Terminal initialization/restoration and the single event loop | Shell runtime |
| Configuration loading and startup diagnostics | Shell startup |
| Running state, quit/close coordination, and deferred lifecycle actions | Shell lifecycle |
| Focus, input-context stack, modal precedence, and cursor visibility policy | Shell UI coordination |
| Layout rectangles and built-in panel/popup/status slots | Shell pages |
| Command registration, keymap/leader resolution, and conflict detection | Shell command platform |
| Effect scheduling and completion delivery | Shell runtime |
| Documents, buffers, revisions, editing, viewport, and save invariants | Editor core and editor feature adapter |
| UI-neutral application capabilities shared with future frontends | `app` crate |
| Workspace root and shared filesystem acceptance rules | Shell/editor capabilities |
| Git discovery/HEAD operations and Gitoxide/Helix VCS types | Optional `vcs` crate |
| Feature-specific state, messages, updates, effects, components, and tests | Each feature |

Within the editor crate, `EditorSession` owns buffer lifecycle, mode, snapshots, and persistence/reload coordination. `ActiveEditor` is the narrow active-document capability for navigation, viewport, and editing operations.

The TUI source tree mirrors these responsibilities:

- `features/`: user workflows.
- `capabilities/`: shared document, filesystem, search, register, and similar services.
- `shell/`: buffer, menu, focus, and lifecycle coordination.
- `lib.rs`: stable re-exports rather than flattening implementation back into the root.

A feature may use shared capabilities, but it must not mutate another feature's private state directly. Cross-feature behavior goes through a semantic command or shell capability, such as “open this document” or “reveal this buffer.”

The VCS integration is optional. Core editor and persistence invariants do not depend on it.

## 4. One Elm loop

Feature registration changes dispatch ownership, not the fundamental application flow:

```text
terminal input / worker completion
              │
              ▼
       semantic message
              │
              ▼
      application coordinator
              │
              ├──► feature update
              │         │
              │         ├── changed feature state
              │         └── optional EffectRequest
              │                    │
              │                    ▼
              │          registered effect handler
              │                    │
              │                    ▼
              │           completion Message
              │                    │
              └────────────────────┘
                            │
                            ▼
                    view(&ApplicationModel)
                            │
                            ▼
               shell layout + feature contributions
```

There is one owner of mutable application state and one terminal owner. Features do not create independent event loops, spawn work directly from `update`, or render as a hidden side effect of mutation.

## 5. Focused contribution points

Avoid replacing a large dispatcher with a large `Feature` super-trait. A feature registers only the capabilities it uses:

| Contribution | Purpose |
| --- | --- |
| State/update | Own feature-local state transitions and internal messages. |
| Commands | Expose stable semantic actions with metadata for keymaps and leader hints. |
| Effects/completions | Execute feature-owned background work and route results back to the feature. |
| UI slots | Contribute a panel, popup, or status item under shell layout/modal rules. |
| Tests | Verify feature behavior and registration conflicts close to the feature. |

Small registries and focused contracts are preferred over one trait every feature must implement. See [Command and effect routing](command-effects.md) and [UI contributions](ui-contributions.md) for these boundaries.

## 6. State and message ownership

Keep feature state explicit and typed in the application model during internal composition:

```rust
// Illustrative relationships, not required final syntax.
struct ApplicationModel {
    editor: EditorState,
    explorer: ExplorerState,
    file_picker: FilePickerState,
    theme_picker: ThemePickerState,
    shell: ShellState,
}
```

Adding a field during static composition is acceptable. A feature update should receive its own state plus a narrow context/capability object, not unrestricted `&mut ApplicationModel`. This prevents theme code from accidentally editing buffers or picker code from changing unrelated focus state.

Do not introduce a dynamic `Any` state bag merely to avoid explicit fields. Reconsider that decision only for a real external-plugin requirement with unknown state types.

Messages fall into two categories:

1. **Command invocations** represent user or automation intent and use stable `CommandId`s.
2. **Feature messages/completions** represent internal transitions and worker results, using feature-specific types where practical.

The shell routes completions to their owners without understanding domain payloads. Stable request IDs, buffer IDs, and feature revisions reject stale results.

## 7. Capabilities and cross-feature behavior

Features depend on narrow shell/editor capabilities rather than each other's concrete state:

- File picker asks the document-open capability to open a selected path; it does not insert a Helix document or reveal the explorer itself.
- Buffer switching asks the explorer-reveal capability to synchronize the active file; buffer state remains editor-owned.
- Theme selection replaces only resolved theme state; it does not touch editor or explorer state.
- Search may request read-only snapshots and document-open navigation, but cannot bypass text-file acceptance or auto-save.
- Future automated editing must name an explicit `BufferId` and expected revision; active-buffer UI commands are not a safe unattended-agent API. See [Agent tools](agent-tools.md) for the proposed approval and mutation contract.

Introduce capabilities at existing seams, not as a preemptive service layer covering every possible future operation.

## 8. Migration and review boundaries

Every step must preserve existing behavior unless a new feature is explicitly being added. Stop for review after each vertical slice.

The proven sequence is:

1. Establish command registration and conflict diagnostics.
2. Prove a pure feature through the theme picker.
3. Prove effect-rich work through the file picker.
4. Decompose remaining shell coordination.
5. Generalize UI contributions after concrete examples exist.
6. Prove composition with a test-only feature and an author checklist.

Completed implementation details are recorded in [Milestone history](milestone-history.md), not in this contract.

## 9. Testing strategy

- Keep existing behavior and smoke tests as characterization coverage.
- Colocate feature unit tests with feature state/update/effect code.
- Test duplicate command IDs, keybinding conflicts, unknown commands/effects, and missing handlers at startup.
- Test modal precedence, cursor hiding, layout clamping, and registration ordering at the shell boundary.
- Test stale completions using feature revisions and stable buffer/request IDs.
- Prove that a test-only feature works through composition-root registration alone.
- Run `cargo test --workspace`, Clippy with warnings denied, formatting, and `git diff --check` after every slice.

A migration is not complete if it merely moves match arms into another central file. Review should verify changed ownership and dependency direction, not only file size.

## 10. SOLID interpretation

- **Single responsibility:** features own domain behavior; the shell owns coordination; the editor owns document invariants.
- **Open/closed:** new behavior is added through registration, not edits to unrelated dispatch and feature modules.
- **Liskov substitution:** handlers and contributions must honor the same effect, revision, modal, and lifecycle contracts.
- **Interface segregation:** commands, updates, effects, and UI slots are separate contracts instead of one mandatory fat feature interface.
- **Dependency inversion:** features depend on narrow capabilities and semantic messages; the shell composes registrations without features depending on each other.

SOLID does not require maximal abstraction or zero composition code. It requires clear ownership and stable dependency rules.

## 11. Future editions

Dynamic user themes and broader visual work are separate from feature composition. External plugins should be evaluated only after internal contracts have real consumers and stable tests. Prefer a versioned, capability-based WebAssembly or out-of-process host over arbitrary native Rust ABIs or unrestricted in-process third-party code.

External capabilities must explicitly cover filesystem access, editor mutation, terminal/UI contributions, and any future AI operations. External code must not bypass automatic saving, revision checks, text-only file acceptance, or terminal/modal ownership.
