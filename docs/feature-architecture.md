# Second-edition feature architecture

Status: accepted direction for Milestone 6; implementation has not started. This document defines boundaries and review steps, not final Rust trait signatures.

## 1. Why this edition exists

The first edition established correct editing, persistence, filesystem, rendering, and theme behavior. Its main growth limitation is coordination code rather than document logic:

- `MessageManager` dispatches every feature and mutates the central `Model`.
- `Message` and `parse_message` grow for new commands and input contexts.
- `Effect` and its runtime match grow for new background work.
- The editor page gains feature-specific popup and cursor conditions.
- Feature tests accumulate beside the central dispatcher instead of the feature they exercise.

Milestone 6 should make feature growth local and reviewable without destabilizing first-edition behavior. It is an **internal architecture edition**, not a third-party plugin release.

## 2. Goals and non-goals

### Goals

- Add a feature through its own module and one explicit composition-root registration.
- Keep unrelated shell code and other features unchanged when a feature is added.
- Give commands stable identities usable by keymaps, leader menus, tests, and future automation.
- Keep updates pure and route all I/O through registered effects and completion messages.
- Preserve explicit ownership of state, focus, modal precedence, layout, editor safety, and terminal lifecycle.
- Prove the architecture with both a simple feature and an effect-rich feature before generalizing further.

### Non-goals

- No native Rust dynamic-library ABI or unrestricted in-process third-party code.
- No WebAssembly/runtime plugin host in this edition.
- No global service locator, singleton registries, or arbitrary `HashMap<TypeId, Box<dyn Any>>` application state.
- No universal `Feature` trait with dozens of lifecycle and rendering methods.
- No big-bang rewrite of editor, explorer, picker, themes, and keymaps in one commit.
- No speculative agent API. The existing revisioned-automation requirements remain future work.

One registration per feature is allowed and desirable. “Open for extension” means unrelated logic stays closed; it does not mean pretending that static Rust composition requires zero wiring.

## 3. Shell and feature responsibilities

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
| Workspace root and shared filesystem acceptance rules | Shell/editor capabilities |
| Feature-specific state, messages, updates, effects, components, and tests | Each feature |

A feature may use shared capabilities, but it must not mutate another feature's private state directly. Cross-feature behavior goes through a semantic command or shell capability, such as “open this document” or “reveal this buffer.”

## 4. Preserve one Elm loop

Milestone 6 changes dispatch ownership, not the architecture's fundamental flow:

```text
terminal input / worker completion
              │
              ▼
       semantic message
              │
              ▼
      application coordinator
              │
              ├──► feature update(&mut FeatureState, Message, Context)
              │              │
              │              ├── changed feature state
              │              └── optional EffectRequest
              │                         │
              │                         ▼
              │               registered effect handler
              │                         │
              │                         ▼
              │                completion Message
              │                         │
              └─────────────────────────┘
                            │
                            ▼
                    view(&ApplicationModel)
                            │
                            ▼
               shell layout + feature contributions
```

There remains one owner of mutable application state and one terminal owner. Features do not create independent event loops, spawn work directly from `update`, or render as a hidden side effect of mutation.

## 5. Use focused contribution points

Avoid replacing the large `MessageManager` with a large `Feature` super-trait. A feature should register only the capabilities it uses:

| Contribution | Purpose |
| --- | --- |
| State/update | Own feature-local state transitions and internal messages. |
| Commands | Expose stable semantic actions with metadata for keymaps and leader hints. |
| Effects/completions | Execute feature-owned background work and route results back to the feature. |
| UI slots | Contribute a panel, popup, or status item under shell layout/modal rules. |
| Tests | Verify feature behavior and registration conflicts close to the feature. |

A registration function at the composition root can assemble these pieces. The exact Rust representation should be proven during migration: small registries and focused contracts are preferred over one trait that every feature must implement.

## 6. State and message ownership

### Explicit state first

Keep feature state explicit and typed in the application model during the internal edition:

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

### Two message categories

1. **Command invocations** represent user or automation intent and use stable `CommandId`s.
2. **Feature messages/completions** represent internal state transitions and worker results. These can remain feature-specific enums during the internal edition.

The shell should route a completion to its owning feature without understanding the completion's domain payload. Stable request IDs, buffer IDs, and feature revisions continue to reject stale results. Dynamic type erasure is not required until static feature composition becomes an actual external-interface problem.

## 7. Command platform

A command is a stable semantic identity plus metadata and a handler, for example:

```text
editor.undo
buffer.next
buffer.previous
file.open
file.find
ui.theme.select
```

Configured direct chords and built-in leader sequences must resolve the same command ID:

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

The built-in Space leader remains fixed product behavior; this edition does not make arbitrary leader sequences user-configurable. The change is that menu entries and direct bindings share command resolution instead of maintaining two command tables.

Exact handler signatures should emerge from the theme migration. Do not start with a generic command bus that carries arbitrary strings and untyped payloads throughout the application.

## 8. Effects and completion routing

Updates remain pure. A feature returns an effect request; the runtime locates the registered handler and executes it with the appropriate scheduling semantics.

Existing behavior demonstrates why one generic `spawn_blocking` wrapper is insufficient:

| Work | Required behavior |
| --- | --- |
| Save | Async owned future; stable `BufferId` and saved revision. |
| Picker scan/filter/preview | Blocking pool; feature revision and latest-result checks. |
| Document open | Blocking read/syntax preparation; main-thread construction of non-`Send` Helix `Document`. |
| Theme selection | Pure update; no effect. |
| Dynamic theme discovery/load | Filesystem effect with request ID, complete validation, and previous-theme preservation on failure. |

An effect registration must connect request execution to completion routing. The runtime should not need a new domain-specific match arm for every feature. It must still distinguish async, blocking, and main-thread requirements rather than treating all effects as arbitrary closures.

Effect handlers may use shared capabilities, but they must not capture mutable application state or bypass editor invariants. Unknown effect kinds and missing handlers are startup/registration errors, not silent no-ops.

## 9. UI contribution model

The shell page continues to own layout and composition:

- Explorer/editor/tools rectangles and visibility.
- Popup placement and clamping.
- Modal/input precedence and the context stack.
- Focus relationships and terminal cursor suppression.
- Status area and shared theme propagation.

Features own concrete components and receive resolved props. They register contributions only in explicit slots:

```text
PanelSlot.Explorer / PanelSlot.Tools
OverlaySlot.Modal
StatusSlot.Message
```

The shell resolves overlay ordering by semantic priority, not accidental registration order. Feature components never query terminal size, perform filesystem work, or install global key handlers.

Ratatui `Widget` lifetimes and object safety should not drive a premature trait-object framework. Start with concrete components and small render-contribution adapters, and erase types only where the page boundary demonstrably requires it.

## 10. Capabilities and cross-feature behavior

Features depend on narrow shell/editor capabilities rather than each other's concrete state. Examples:

- File picker asks the document-open capability to open a selected path; it does not insert a Helix document or reveal the explorer itself.
- Buffer switching asks the explorer-reveal capability to synchronize the active file; buffer state remains editor-owned.
- Theme selection replaces only resolved theme state; it does not touch editor or explorer state.
- Future search may request read-only snapshots and document-open navigation, but cannot bypass text-file acceptance or auto-save.
- Future automated editing must name an explicit `BufferId` and expected revision; active-buffer UI commands are not a safe agent API.

Capabilities should be introduced at existing seams, not as a large preemptive service layer covering every possible future operation.

## 11. Migration sequence and review boundaries

Every step must preserve existing behavior unless a new feature is explicitly being added. Stop for review after each vertical slice.

### M6.1 — Command registry foundation

Introduce stable command IDs, registration metadata, duplicate/conflict diagnostics, and shared direct-key/leader resolution. Migrate a small command set first. Keep characterization tests for current bindings and hints.

Implemented in the first slice: `CommandRegistry` owns all current IDs and descriptions, direct keymaps and leader targets resolve through it, and duplicate command IDs or duplicate chords in one context fail at startup. Handlers currently resolve to existing `Message`s as a compatibility boundary; later feature slices replace them with narrower feature-owned handlers without changing bindings again.

### M6.2 — Theme picker vertical slice

Move theme-picker state/messages/update and its popup contribution behind the new registration path. This proves commands and a pure feature without conflating the work with asynchronous effects.

Implemented: resolved bindings emit `InvokeCommand(CommandId)`, and `MessageManager` invokes either compatibility messages or a feature handler. `theme_picker/` now owns its state, command handler, component, and workflow tests; the page consumes the feature component while retaining placement and cursor policy. The handler still uses a small `Model` adapter until the remaining shell migration defines a narrower context.

### M6.3 — File picker vertical slice

Migrate scanning, filtering, preview, and open completions to registered effects. This proves scheduling, revisions, completion routing, and interaction with the document-open capability.

Implemented: `FeatureEffect` is a feature-owned effect object with default blocking-pool execution and completion-message delivery. `FilePickerRequest` implements it, so the runtime no longer has a picker-specific effect arm. Scan/filter/preview revisions and completion behavior are unchanged, and an async channel test covers execution. Save and document-open effects remain compatibility variants for the later shell migration.

### M6.4 — Remaining shell decomposition

Split lifecycle, buffers, editor, explorer, and open/save coordination into focused modules. `MessageManager` becomes a small coordinator. Preserve save-before-close/quit, dirty follow-ups, and explorer synchronization tests.

Implemented slices: `buffer.rs` owns next/previous commands plus shared dirty-save and explorer-reveal capabilities. `editing.rs` owns Insert-mode transitions, movement, insertion, deletion, and history commands. `explorer/commands.rs` owns selection, expansion, refresh, and ignore behavior. `lifecycle.rs` owns quit, close, deferred lifecycle actions, and save completion. `document.rs` owns document-open requests, stale-completion protection, insertion, and explorer reveal, with open-specific tests colocated. `focus.rs` owns explorer visibility and logical focus relationships. `file_picker/commands.rs` owns picker commands and scan/filter/preview completion coordination, while picker state owns its `DocumentFactory`. Other update paths consume shared capabilities rather than duplicating logic. `MessageManager` is now a small coordinator with explicit compatibility messages for remaining shell concerns; those can be narrowed by later UI/capability work without further feature-specific runtime arms.

### M6.5 — Generalize UI contributions

After two concrete popup examples exist, replace remaining page feature conditionals with explicit panel/popup/status contributions. Keep modal precedence and cursor policy shell-owned.

Implemented: `UiRegistry` registers panel/status renderers and overlay providers at the composition root. Overlay contributions contain semantic kind, placement, and a render function; the page computes all rectangles, renders hints before modals, and suppresses the editor cursor whenever an overlay is present. The topmost overlay may instead publish its own cursor through `with_cursor`; the file picker uses this with the shared Unicode-aware `TextInput` component. File picker, theme picker, and command-menu hints use providers, while duplicate panel/status slots and overlay IDs fail during registration.

### M6.6 — Architecture proof and author guide

Add a test-only example feature that registers state/update, a command, a popup, and an effect. The test must not modify unrelated feature modules or runtime dispatch. Document the author checklist and registration/conflict tests.

Implemented: `test_utils/example_feature.rs` registers typed state, two commands, an overlay, and an effect. Its integration test verifies popup rendering, effect completion through `InvokeCommand`, and visibility cleanup without runtime changes. `docs/feature-authoring.md` records the contribution and testing checklist. M6 is complete; M7 remains a separate edition.

### M7 — Core editing workflows after M6

Do not expand M6 with theme or plugin work. Once the contribution architecture is proven, use it for high-priority editing features in this order:

1. **Page-scroll:** semantic page/half-page commands using the shared viewport, with boundary and resize coverage.
2. **Current-buffer visible jump:** feature-owned smart-case two-character matching and stable adaptive labels over the active visible snapshot, with accept/cancel and generic absolute cursor navigation. Incremental quick-search and workspace-wide grep remain separate features.
3. **Text-object foundation:** document selection/operator semantics first, then add a deliberately small set of motions and inner/around objects through existing transaction/undo/auto-save paths. Do not turn this into full Visual mode or a complete Vim grammar.

These are real feature consumers of the M6 contracts, not reasons to defer M6 or add a separate editing architecture.

Implemented M7.1: `page_scroll.rs` registers `page_scroll_down`/`page_scroll_up` with default `Ctrl-F`/`Ctrl-U` bindings. `[features.page_scroll] step = "75%"` is validated at startup, converted to rows from the active viewport height, and executed through counted Helix vertical movement. This is built-in feature configuration, not an external extension API.

Implemented M7.2: `buffer_jump/` owns the `Space c j` workflow, smart-case two-character matching over visible snapshot lines, stable one-key labels for up to 26 matches and two-key labels above that, cancellation/backspace, and final cursor movement. `TextInputRegistry` replaces feature-specific character messages for Insert mode, the file picker, and jump. `UiRegistry` editor overlays receive the existing snapshot and resolved text rectangle for dimming/labels without render-time scanning. The editor core exposes only generic `DocumentPosition` navigation and viewport width; it contains no jump policy.

## 12. Testing strategy

- Keep all first-edition behavior and smoke tests as characterization coverage.
- Colocate feature unit tests with feature state/update/effect code.
- Test duplicate command IDs, keybinding conflicts, unknown commands/effects, and missing handlers at startup.
- Test modal precedence, cursor hiding, layout clamping, and registration ordering at the shell boundary.
- Test stale completions using feature revisions and stable buffer/request IDs.
- Prove that a test-only feature works through composition-root registration alone.
- Run `cargo test --workspace`, Clippy with warnings denied, formatting, and `git diff --check` after every slice.

A migration is not complete if it merely moves match arms into another central file. Review should verify changed ownership and dependency direction, not only file size.

## 13. SOLID interpretation for this codebase

- **Single responsibility:** features own domain behavior; the shell owns coordination; the editor owns document invariants.
- **Open/closed:** new behavior is added through registration, not edits to unrelated dispatch and feature modules.
- **Liskov substitution:** handlers and contributions must honor the same effect, revision, modal, and lifecycle contracts; a plugin must not bypass safety by implementing an interface differently.
- **Interface segregation:** commands, updates, effects, and UI slots are separate contracts instead of one mandatory fat feature interface.
- **Dependency inversion:** features depend on narrow capabilities and semantic messages; the shell composes registrations without features depending on each other.

SOLID does not require maximal abstraction or zero composition code. It requires clear ownership and stable dependency rules.

## 14. TBD — much later editions

Dynamic Helix-compatible user themes and broader visual style/theming work are unscheduled. Existing built-in themes and semantic style boundaries remain unchanged and are not prerequisites for M7.

Only evaluate external plugins after the internal contracts have real consumers and stable tests. Prefer a versioned, capability-based WebAssembly or out-of-process host. Do not expose arbitrary Rust layouts as a native ABI or add unrestricted in-process third-party code.

External capabilities must explicitly cover filesystem access, editor mutation, terminal/UI contributions, and any future AI operations. External code must not bypass automatic saving, revision checks, text-only file acceptance, or terminal/modal ownership. Those rules are part of the architecture, not optional policy layered on afterward.
