# Feature composition with React-style interfaces

Eggplant uses Elm's `Model → Message → Update → View → Effect` loop. Pages and components should still feel like React composition: small units with explicit inputs and no hidden side effects. This document describes how to keep features reusable without building a component or dependency-injection framework.

## Mental model

| React concept | Eggplant equivalent |
| --- | --- |
| Page | `page/*/mod.rs`: route-level layout and component composition. |
| Presentational component | A component or page-local type that turns read-only state into Ratatui output. |
| Hook/capability | A narrow interface such as `EditingTarget`, with page-specific adapters. |
| Effect description | `Effect`, `FeatureEffect`, and owned completion values. |
| Context/provider | Explicit adapter construction from `Model`; no hidden global service locator. |

`Model` is the application root, not a required dependency of every reusable operation. Adapters may know about `Model`; reusable command logic should depend only on the capabilities it actually uses.

## 1. Compose pages from layout and components

A page should primarily calculate named regions and render components:

```text
VcsHistory
├── VcsHistoryLayout
├── HistoryList
└── CommitDetail
```

Components own their view-model derivation, selection geometry, and presentation rules. They do not run repository work, mutate application state, or construct background tasks. Page-local components belong in the page's directory module; genuinely shared components belong in `component/`.

Use directory roots consistently:

```text
page/feature/
├── mod.rs          # page composition and layout
└── component.rs    # page-local reusable unit
```

Do not move presentation formatting into backend capabilities merely to shorten a page.

## 2. Program to narrow interfaces

An interface should list the inputs an operation needs, no more. `EditingTarget` is the example:

```rust
trait EditingTarget {
    fn session(&mut self) -> &mut EditorSession;
    fn input_context(&self) -> Context;
    fn push_context(&mut self, context: Context);
    fn pop_context(&mut self, context: Context);
    fn is_editing_enabled(&self) -> bool { true }
    fn register_text(&self) -> String { String::new() }
    fn record_deleted(&mut self, text: String) -> Option<Effect> { None }
    fn record_yanked(&mut self, text: String) -> Option<Effect> { self.record_deleted(text) }
    fn after_edit(&mut self) -> Option<Effect>;
}
```

Reusable editing functions accept `&mut impl EditingTarget`, not `&mut Model`. They can move a cursor, insert text, or open a line without knowing which page owns the editor.

Adapters provide concrete policies:

- `MainEditingTarget` selects `model.editor`, uses `EditorInput`, and auto-saves file-backed buffers.
- `CommitEditingTarget` selects the scratch session, uses `VcsCommitInput`, checks submission state, and performs no message-file save.

`record_deleted` applies the same policy separation to operators: `x`, `dd`, and `dw` reuse the `ActiveEditor` text-object operations in both contexts. The main editor records deleted text in its unnamed register; the commit scratch target does not pollute that register.

`register_text` and `record_yanked` complete the same capability for `yy`, `p`, and `P`. `ActiveEditor::paste_text` owns characterwise and linewise placement, while the target owns register and persistence policy. Explicit yanks intentionally share the register across editor and commit pages; commit deletions still do not overwrite it. When a system clipboard provider exists, paste reads it in a background effect, updates the register on completion, and applies only if the original page, buffer, and document revision are unchanged. Local-register paste remains the fallback when no provider exists.

The page dispatcher is the only place that needs to select an adapter. Static generics are normally sufficient; avoid `dyn Trait` and global registries unless dynamic behavior is a real requirement.

### Interface design rules

- Extract when at least two consumers need different policies, not merely to make a trait.
- Include only operations used by the reusable core.
- Keep domain invariants in their owning crate; an interface must not bypass `EditorSession` invariants.
- Return data rather than reaching for unrelated model fields.
- Do not turn every helper or feature into a trait. Narrow structs and pure functions are also interfaces.

### Example: vertical navigation with policy

`VerticalNavigation` keeps movement independent of the concrete selection model:

```rust
trait VerticalNavigation {
    fn item_count(&self) -> usize;
    fn move_to(&mut self, index: usize);
    fn accepts_count(&self) -> bool { false }
}
```

`EditorSession`, `ExplorerState`, and the history adapter provide their own index clamping and selection writes. `LineJumpState::update` parses counts and `g` prefixes into explicit actions without `Model`; the input adapter owns pending display/context updates and dispatches only the completed destination. One closed `NavigationTarget` adapter supplies both count policy and movement, so target-selection rules cannot drift between input and completion. The editor opts into counts; explorer and history keep the default `false`, so `gg`/`G` is shared without accidentally enabling counted jumps everywhere.

This is the interface boundary to prefer: shared behavior in the operation, varying state and policy in adapters.

### Example: page scroll without application context

`capabilities/page_scroll.rs` owns percentage validation and row calculation. Its only runtime dependency is the narrow `ScrollViewport` interface; it does not import `Model`, `AppPage`, the editor crate, configuration modules, or status policy. Unit tests use a small fake viewport.

`features/page_scroll.rs` is the composition adapter: it implements `ScrollViewport` for `EditorSession`, maps command IDs to scroll directions, and invokes the configured `PageScroll` value. Startup converts the raw configured percentage with `PageScroll::from_step`.

Feature availability comes from composition—the editor's Normal keymap and adapter—not from page checks inside the reusable capability. A page that does not need scrolling simply does not compose it.

### Example: a short-lived theme-picker adapter

`ThemePickerAdapter` is constructed per command by destructuring only `ThemePickerState`, `Theme`, the context stack, and status from `Model`. Its methods perform open/close/select/apply transitions, then the adapter is discarded. Pure theme commands return no effect, and picker visibility is private behind `is_open`. The registered `handle_command` is the only function that needs the global model; the widget continues to receive only picker state and theme.

Use this pattern for synchronous composition when a narrow concrete adapter is sufficient. It does not require a new trait, global service, or long-lived controller.

### Example: reuse typed workflows instead of re-entering command dispatch

`buffer::switch(BufferMovement)` is the shared typed workflow for direct next/previous commands and bracket-prefixed navigation. It preserves outgoing save, buffer activation, explorer reveal, and VCS synchronization. `BufferNavigationState` owns prefix begin/finish/clear transitions and prompt text; its adapter passes the parsed `BufferMovement` directly to `buffer::switch` instead of converting it back into a string `CommandId` and invoking another feature's root handler.

Cross-feature reuse should target a semantic capability or workflow like this. Re-entering string dispatch obscures dependencies and makes it easier to accidentally drop required follow-up behavior.

### Example: a one-shot prefix session

`ViewAlignSession` exists in `Model` only as `Option<ViewAlignSession>` while viewport alignment is pending. Starting the prefix creates the session; `finish(self, character)` consumes it and returns an optional `ViewAlignment`. Invalid input therefore finishes the session without alignment, and explicit cancellation drops it. The adapter still owns pending text, input context, and editor application.

Use an owned session when a workflow must survive between messages but has a clear completion boundary. This keeps transient state encapsulated without a long-lived controller or hidden `Drop` side effects. `ChangeState` applies the same action-oriented pattern to `dd`, `dw`, `cc`, and `cw`: taking its pending operator makes each prefix one-shot, while the adapter executes the returned typed action. `YankState` similarly owns `yy` begin/finish/clear transitions without mixing that prefix lifecycle into asynchronous paste state.

## 3. Encapsulate effects as composable policy

Effects are values, not hidden work performed inside rendering or text manipulation. `EditingTarget::after_edit` demonstrates the smallest useful policy hook:

```rust
fn insert(target: &mut impl EditingTarget, text: &str) -> Option<Effect> {
    target
        .session()
        .active_editor()
        .and_then(|mut editor| editor.insert_text(text))
        .ok()?;
    target.after_edit()
}
```

The same operation therefore supports auto-save on the editor page and ephemeral editing on the commit page. The adapter owns persistence policy; the reusable command owns only the text mutation and effect trigger.

Background work follows the same rule:

1. Clone all required inputs into an owned request.
2. Execute through `FeatureEffect` outside rendering and input handling.
3. Return a typed completion.
4. Validate target and revision/generation before applying it.
5. Compose independent effects with `Effect::combine`; do not use a batch as a completion barrier.

A save-dependent workflow must use an explicit completion state machine, as the commit save barrier does.

## 4. Keep ownership boundaries clear

- `editor` owns document text, cursor, revisions, view geometry, and persistence invariants.
- Feature state owns workflow state, loading phases, selections, and request generations.
- Backend capabilities own external systems such as Git and return owned domain values.
- Adapters select state and policy from `Model`.
- Pages and components read state and render it.
- Runtime executes effects and routes completions.

A scratch feature session must not enter the main buffer tabs, save-all workflow, or close/quit persistence policy. Buffer identities from separate sessions must never be mixed.

## 5. Test at both levels

Use a small fake interface for reusable operations. It should verify behavior without constructing `Model`:

- Editing changes the expected session text and contexts.
- The persistence hook is called exactly when expected.
- Unavailable targets reject input safely.

Keep integration tests for real adapters:

- Main editing still auto-saves file-backed documents.
- Commit editing remains ephemeral and respects submission locks.
- Existing command and rendering behavior does not change.

Selection and viewport tests should use actual rendered rows, including expanded content, rather than assuming a model index equals a visible row.

## Feature review checklist

When reviewing another feature, ask:

1. Does its reusable core need `Model`, or only one state/capability input?
2. Which policies vary by page: persistence, contexts, availability, targets, or effect execution?
3. Can those policies be explicit adapter methods instead of page checks in leaf functions?
4. Are effects owned, revisioned, and independent of rendering?
5. Does the page mostly compose layout and components?
6. Can tests use a small fake while real adapters retain integration coverage?
7. Is the abstraction justified by current consumers? If not, prefer a pure function or narrower struct.
