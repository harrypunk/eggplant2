# Global `Model` coupling audit and incremental refactor

Status: proposed migration plan; no behavior change is implied by this document.

Baseline reviewed: `487edc3` on 2026-11-17. The audit covers the TUI application layer, especially `crates/tui/src`. The `editor` and `vcs` crates are included when evaluating dependency direction, but free functions that already accept narrow domain inputs are not treated as problems.

Related contracts:

- [Feature architecture](feature-architecture.md)
- [Feature composition](feature-composition.md)
- [Command and effect routing](command-effects.md)
- [UI contributions](ui-contributions.md)

## Implementation checkpoint

The following slices have landed and are covered by the workspace test suite:

- Single-flight VCS index mutation with authoritative status reconciliation.
- Explicit merge phases and feature-owned command/completion transitions.
- Commit attempts owning repository, message, revision, and save/Git phases.
- History-detail request identity and explicit List/Detail command routing.
- Dedicated active-buffer diff-base state and stale-generation validation.
- `ChangesController` ownership of changed files, selection, refresh generations, trailing-dirty state, status completion, and single-flight index operation identity.
- Independent page-scroll capability, short-lived theme adapter, typed buffer switching, and model-free prefix parsers.
- Owner-tagged pending input with active-context rendering.
- Typed file/project/buffer-search completions and feature-owned buffer-search previews.
- Buffer-jump source revision validation.
- Generated document-open request IDs, origin contexts, and picker-session continuation guards.
- Save continuation routing at the application root; generic lifecycle no longer imports VCS.
- Single-owner fail-fast save policy: active commits reject close/quit, and pending close/quit rejects commit submission without changing drafts or owners.
- Worktree/index/save status synchronization with one-in-flight-plus-trailing-read coalescing.
- File-picker entry/preview invalidation on external changes.
- History, working-change, and repository workflows have separate implementation modules; `vcs/mod.rs` is reduced to composed state, registration/dispatch, shared selection math, and API re-exports.
- Typed VCS list/detail/change/header/popup props and shared presentation-owned `DiffState`.

Remaining structural work:

- Move VCS working-tree diff state into `ChangesController`; split history and repository/HEAD state into controller-sized modules.
- Replace remaining broad editing/page adapters with composed targets rather than wildcard fallback behavior.
- Introduce shell-owned modal/session coordination and finish registry/page cleanup.
- Move explorer tree traversal behind effects if responsiveness measurements justify it.


## 1. Executive summary

The concern is valid, but the precise problem is **unrestricted authority over global mutable state**, not the use of free functions by itself.

`Model` is a reasonable Elm composition root. Keeping one owned application state and one update loop is not a god-object anti-pattern by itself. The maintainability problem appears when a leaf operation receives `&mut Model` and can therefore read or modify the editor, lifecycle, focus, VCS, pickers, registers, and status even when it needs only two of them.

A conservative textual scan, excluding test modules and test-support files, finds at least **171 `&mut Model` and 54 `&Model` occurrences across 44 production files**. These counts include function-pointer types and trait methods, so they are coupling indicators, not counts of independent defects. `Model` itself has **26 production fields, all public**. The largest concentrations are:

| Area | Evidence | Main risk |
| --- | --- | --- |
| VCS UI | `features/vcs/mod.rs` has 33 mutable-`Model` occurrences; `VcsState` has 42 fields | Repository refresh, history, changes, diffs, caches, commit, and merge evolve as one aggregate; concurrent index mutations can also leave UI and Git disagreeing. |
| Save/lifecycle | `shell/lifecycle.rs`, `shell/buffer.rs`, `features/vcs/commit.rs` | Shell save completion directly knows about VCS commit policy; save, close, quit, and commit continuations are coupled. |
| Operators | `features/change.rs`, `yank.rs`, `line_jump.rs`, `buffer_navigation.rs`, `view_align.rs` | A small prefix session is spread across global feature state, `pending_input`, and `context_stack`. |
| Document/synchronization | `capabilities/document.rs`, `features/external_sync.rs`, `manager/message_manager.rs` | Core operations directly invoke picker, focus, explorer, and VCS policy. |
| Completion routing | `manager/effect.rs` and feature `Completion` implementations | Every worker result receives authority to mutate the whole application. |
| Rendering | VCS page components and picker/search overlays | Presentational leaf components query the global root instead of resolved props. |

The existing architecture documents already prescribe the desired direction:

- `docs/feature-architecture.md:128` says a feature update should receive its own state and narrow capabilities.
- `docs/command-effects.md:29,35` calls for narrow command contexts.
- `docs/ui-contributions.md:11` says feature components receive resolved props.

The implementation has drifted from those contracts. The refactor should close that gap incrementally, not introduce a new application framework.

## 2. What should change—and what should not

### The dependency rule

Only composition-root code and deliberately small adapters should know about `Model`.

```text
Message / command / completion
              │
              ▼
     composition-root adapter       <-- may know Model
              │
              ├── borrow feature state and explicit capabilities
              ▼
     feature method / workflow      <-- no Model dependency
              │
              ├── mutate private feature state
              └── return request or semantic outcome
              │
              ▼
     adapter maps outcome to effects and shell coordination
```

A useful review rule is:

> `Model` may select a feature and assemble its inputs; it must not be the feature's operating environment.

### Keep these patterns

- **Root state composition:** `data_object::Model` may continue to own typed feature fields. Adding a field at static composition time is not itself a SOLID violation.
- **Root dispatch:** `runtime.rs`, `MessageManager::update`, and registry entry points may receive `Model` in order to route work.
- **Pure free functions:** functions with explicit domain arguments are often clearer and easier to test than methods.
- **Owned background requests:** `FeatureEffect` implementations such as `ProjectGrepRequest` and `BufferGrepRequest` already follow “construct with owned inputs, do the job, consume the object.”
- **Domain encapsulation:** `EditorSession` and `ActiveEditor` own editor invariants and should remain the authority for document changes.
- **Narrow traits with real consumers:** `EditingTarget` and `VerticalNavigation` demonstrate useful policy boundaries.
- **Feature-state methods:** `FilePickerState`, `ProjectGrepState`, `BufferGrepState`, and `BufferJumpState` already contain useful local transitions. Their adapters should be narrowed before their internals are rewritten.
- **Backend free functions:** `crates/vcs` functions accept repository/domain values rather than the TUI `Model`. Moving them into an arbitrary `GitManager` class would not improve them.

### Do not introduce these substitutes

- No `Rc<RefCell<Model>>`, `Arc<Mutex<Model>>`, global singleton, or service locator.
- No universal `Feature`, `Manager`, `Repository`, or `Service` trait with unrelated methods.
- No trait for every function or every struct. Concrete capability structs and narrow argument structs are often sufficient.
- No background task holding `&mut Model` or capturing application state.
- No `Drop` implementation that performs modal cleanup, saves, Git work, or other hidden side effects.
- No big-bang conversion of free functions to methods, getters/setters, or inheritance.
- No cosmetic regrouping of `Model` fields presented as decoupling. A `FeaturesState` containing the same globally accessible fields changes nothing by itself.

### Where short-lived objects fit

The proposed “create, do the job, and discard” style is appropriate for:

1. **Owned effect requests:** a request owns paths, revisions, snapshots, and factories; execution consumes it.
2. **Synchronous use cases:** a small operation can own parsed inputs and return an outcome without retaining application references.
3. **Adapters:** a short-lived struct may borrow only the fields required by one call, as `EditingTarget` does conceptually.

It does **not** eliminate state that must survive between events. A commit waiting for saves, a merge waiting for Git, a picker waiting for filtering, and a two-key prefix all need explicit state between messages. That state should live in a private feature/workflow object with valid phases, then be cleared on completion or cancellation. It must not be scattered across unrelated global fields.

## 3. Prioritized findings

Priorities below describe refactor value and coupling risk, not known runtime bug severity.

### F1 — P1: `VcsState` is a second global model

**Evidence**

- `crates/tui/src/features/vcs/mod.rs:28-72`: 42 fields combine HEAD, repository identity, history list, pagination, expanded commits, commit detail, status changes, index operations, two kinds of diff, caches, buffer diff bases, notices, help, commit, merge, and a document factory.
- `mod.rs:356-815`: command handling, page navigation, selection, request construction, cache policy, refresh policy, and repository discovery are interleaved through free functions taking `Model`.
- `mod.rs:607-665,904-1047`: stage/unstage operations have no single-flight guard. Independent blocking mutations can both succeed in Git while an older completion is discarded by revision checks; optimistic status projection can then disagree with the real index.
- `mod.rs:843-1387`: each completion directly validates and mutates fields scattered across the aggregate.
- `page/vcs_history/mod.rs:21-37` treats every non-`Idle` detail state as open, while `features/vcs/mod.rs:310-321,408-423` routes commands as though only `Ready` detail is active. Loading/error panes can therefore navigate or toggle the list behind the visible detail.
- `features/vcs/mod.rs:485-506,1225-1251`: history-detail completion validates revision and repository but not requested commit ID. Opening a cached commit B while uncached commit A is loading can leave A's later completion overwriting B because cached transitions do not invalidate the request.
- `commit.rs` and `merge.rs` reach through `model.vcs` into the same aggregate and also mutate page, context, explorer, editor, and external-sync state.

This is more than a large file. Request ownership and invariants are implicit: for example, the correctness of a history completion depends on several independent fields remaining synchronized. Adding another VCS operation increases the number of possible interactions.

**Refactor**

Keep one `VcsFeature` composition object, but divide state by actual workflows:

```text
VcsFeature
├── RepositoryViewState       HEAD and current repository identity
├── ChangesController         status, selection, staging, expanded working-tree diff
├── HistoryController         pages, selection, commit detail, commit-file diff
├── BufferDiffController      per-buffer HEAD base and generation
├── CommitWorkflow            draft and Editing/Saving/Committing phases
└── MergeWorkflow             picker and Loading/Selecting/Running phases
```

Start by moving existing transitions and completion checks into methods on these types. Keep `crates/vcs` and Git semantics unchanged. Return requests/outcomes; let a thin VCS adapter perform page and cross-feature coordination.

The first VCS vertical slice should be `StatusController`, not a cosmetic file split. Give it `mutation: Option<InFlightMutation>` and reject or queue another index mutation for the same repository. Reconcile every successful mutation with authoritative Git status; a stale UI completion does not mean the earlier Git mutation never happened. If optimistic projection remains, it needs domain ownership plus eventual authoritative reconciliation. Add explicit `HistoryPane::{List, Detail}` so loading/error data states do not change command routing. Include commit identity in history-detail request state, validate it on completion, and invalidate outstanding detail requests on cached switches/close.

Do not create one fat `GitService` trait mirroring every backend function. Effect requests are already an external-work boundary. If a fake is needed, define a consumer-sized port such as `HistoryReader` or `CommitWriter`, not a universal Git interface.

### F2 — P1: save completion creates a shell → VCS dependency cycle

**Evidence**

- `shell/lifecycle.rs:27-47`: `finish_save` completes an editor save, clears lifecycle state, calls `vcs::commit::save_failed`, checks `model.vcs.commit.is_waiting_for_save`, and may continue a commit. Its early commit return bypasses deferred close/quit processing if those workflows overlap; current match ordering, rather than an explicit policy, decides what happens.
- `shell/buffer.rs:62-70`: low-level `start_save` clears `model.deferred_action` on error, even though save initiation is also used outside lifecycle close/quit.
- `features/vcs/commit.rs:144-217`: commit owns a save barrier but depends on the shell to call its continuation after each save.
- `lifecycle.rs:182-227`: `DeferredAction` and close/quit orchestration are global mutable fields manipulated by multiple helpers.

A new save-dependent feature would currently require editing shell lifecycle code. This violates the intended open/closed boundary and makes error policy depend on call order.

**Refactor**

Treat save completion as shared infrastructure with an explicit initiator and continuation tracking:

```text
Save request
  ├── initiator: Automatic
  ├── initiator: Lifecycle(close/quit workflow token)
  └── initiator: Commit(commit workflow token)
```

A concrete `SaveCoordinator` may own pending continuations and return typed `SaveEvent` values. The application root applies the editor completion, then routes events to lifecycle or commit. Neither module calls the other directly. Enums are sufficient; a publish/subscribe framework is not.

The request's initiator and its waiters are different concepts. If close/quit and commit can both wait on the same write, the coordinator must retain every active continuation and notify each whose barrier is satisfied. A single `owner` field must not accidentally discard another workflow's waiter. For mutually exclusive commit/close/quit goals, explicitly reject, queue, or cancel one workflow; preserving waiters must not silently launch conflicting goals.

Move `DeferredAction` behind a `LifecycleController` API. Put commit attempt identity, repository, and captured message inside `CommitPhase::{Saving, Committing}` rather than beside the phase as unrelated fields. Save-start errors should be returned to the requesting workflow, which decides whether to clear a close action, restore commit editing, or only display an error.

This slice must preserve current invariants: no automatic discard, no merge save, serial commit save barrier, follow-up saves for edits during I/O, and close/quit only after successful persistence.

### F3 — P1: core operations directly implement unrelated feature policy

**Evidence**

- `capabilities/document.rs:23-37,41-85`: pending opens are correlated only by path, so two same-path requests with different positions or contents are indistinguishable. Successful insertion also reveals the file in the explorer through `buffer::reveal_buffer`.
- `manager/message_manager.rs:121-129`: every successful open closes whichever file picker is currently visible, focuses the editor, and synchronizes VCS. An older explorer/search completion can therefore close or displace a newer unrelated modal.
- `shell/buffer.rs:15-41`: switching buffers saves, changes selection, reveals the file, and calls VCS directly.
- `features/external_sync.rs:17-25,102-115`: file-event handling directly calls VCS and explorer; reload completion directly refreshes a VCS buffer base. Ordinary worktree changes, `.git/index` changes, and save completion do not consistently invalidate status, while broad `repository_changed` plus watcher events can duplicate HEAD/history/status/diff-base/reload work.
- `features/vcs/merge.rs:298-306`: merge completion directly refreshes VCS, explorer, and all open buffers.
- `features/buffer_navigation.rs:57-65`: prefix input invokes the concrete buffer command handler rather than returning a semantic movement outcome.

These are hidden dependency edges. Reusing “open a document” or “reload a buffer” silently pulls in whichever policies happen to be encoded in its current caller or callee.

**Refactor**

Separate the operation from its follow-up policy:

- `DocumentOpening` owns `PendingOpen { request_id, location, origin }`, validates the latest completion by a generated `OpenRequestId` rather than path alone, inserts the document, applies an optional position, and returns `OpenApply::{Stale, Failed, Opened}`. It does not know picker, focus, explorer, or VCS.
- The root open coordinator separates shared reconciliation from origin continuation: `DocumentOpened` drives explorer reveal and VCS synchronization, while `OpenOrigin` permits picker close or focus restoration only if that originating workflow is still current. A newer unrelated modal must not be closed or displaced.
- Buffer switching returns a `BufferSwitched` outcome plus an optional save request. The root performs reveal/VCS follow-up.
- External event classification returns intents such as `RefreshStatus`, `RefreshHead`, `RefreshHistory`, `RefreshExplorer(path)`, and `ReloadBuffer(id)`. The root executes them through the owning features.
- Add an application-level `RepositorySyncWorkflow` above VCS/editor/explorer. Distinguish worktree content/structure, Git index, HEAD/refs, save, commit, branch switch, reload, active-buffer change, and buffer close. Coalesce each refresh stream as one in-flight request plus one trailing dirty request, and coalesce reloads per `BufferId`; commit success must not cause a wholesale worktree reload.
- Merge success returns `MergeCompleted`/`BranchSwitched`; root coordination refreshes repository state, explorer state, and eligible buffers through that workflow.

This is dependency inversion through semantic outcomes and narrow capabilities. It does not require a generic event bus.

### F4 — P1: `Completion` is an unrestricted global mutation capability

**Evidence**

- `manager/effect.rs:7-9`: `Completion::update(self, &mut Model)` gives every completion access to all mutable application state.
- `features/vcs/mod.rs` and `merge.rs`: completions contain substantial validation, state transition, notice, page, refresh, and reload logic.
- `capabilities/register.rs:68-76` and search completions are smaller, but still receive the same global authority.

The trait has one narrow method syntactically, but its argument is the broadest possible interface. This is an interface-segregation problem.

**Refactor**

Use two layers:

1. A tiny type-erased completion adapter may retain `&mut Model` because the runtime needs uniform routing.
2. Its implementation should only select the owning feature and call a typed method such as `apply(self, &mut FeatureState, &mut NarrowContext) -> FeatureOutcome`.

Domain completion tests should call that method without constructing `Model`. Worker `perform` methods should remain owned and free of application-state access.

A later cleanup may replace type erasure with typed feature messages where that is simpler. Do not replace this with a global `Any` payload map or move the same large matches into another central file.

### F5 — P1: workflow state is scattered and admits invalid combinations

**Evidence**

- `data_object::Model:134-156` stores independent feature visibility/active flags, `current_focus`, `pending_input`, `deferred_action`, and workflow states alongside global `context_stack`.
- `file_picker/commands.rs:73-95`, `theme_picker/mod.rs:38-69`, and `ui.rs:203-218`: modal visibility, input context, focus, and overlay order are separate sources of truth. Overlays sort only by kind, so the input-owning modal and rendered/cursor-owning modal can diverge under direct commands or asynchronous completion.
- `change.rs:116-136`, `buffer_navigation.rs:67-89`, `view_align.rs:63-77`, `line_jump.rs:120-175`, and `yank.rs` each update several root fields to begin or end one logical session.
- `features/vcs/merge.rs:15-26` represents phases with independent `active`, `loading`, and `merging` booleans plus repository/revision fields.
- `features/yank.rs:170-204,236-277`: asynchronous paste validates buffer and document revision, but not the cursor/workflow position. A completion can therefore paste at a cursor moved after the request; leaving and re-entering the same unchanged buffer is another possible ABA case.
- `features/buffer_jump/state.rs:14-21` and `mod.rs:74-109`: labels retain buffer identity but not the document revision or viewport from which targets were computed. External reload or resize can make accepted positions stale.
- Many VCS and picker fields are `pub` or `pub(crate)`, allowing views and unrelated modules to mutate state without maintaining request/selection invariants.

Examples of representable but undesirable states include a merge that is inactive but merging, a prefix with context but no pending text, and a completion token whose target belongs to a different operation. Current checks reduce the risk but do not make the states impossible.

**Refactor**

- Introduce per-feature session types with `start`, `handle_input`, `cancel`, and `finish` transitions.
- As a first pending-input fix, use an owner-tagged `PendingInput { owner: Context, text: String }`; `clear(owner)` must not clear another workflow's display. Next, derive the text from active feature state and remove the duplicated root string where practical.
- Decide and test whether page/focus transitions cancel or suspend a prefix. Do not leave this to accidental context-stack manipulation.
- Introduce a small `ShellUi` coordinator and `ModalSession { context, previous_focus, generation }`. Either allow one feature modal or render an explicit session stack; active input, topmost overlay, cursor, and focus restoration must come from the same session state. Acceptance commands must require a live session rather than retained results.
- Represent phases with enums, for example `MergePhase::{Closed, Loading, Selecting, Running}` and explicit commit/save states.
- Keep request token, repository, and operation identity together in one `RequestToken` where the same repeated invariant exists. Prefer a small concrete token over a generic state-machine library.
- Make fields private and expose read-only queries or view props.
- Capture the complete identity needed by asynchronous workflows. Initially, reject paste if its captured cursor changed and reject/recompute buffer-jump labels if document revision changed; define resize behavior explicitly.
- Have each transition return shell actions such as `PushContext`, `PopContext`, and `SetPendingInput`. Initially, pass a narrow concrete input-ui context; extract a shared abstraction only if several migrations demonstrate the same contract.

Do not use RAII/`Drop` to pop contexts. Cancellation, replacement, and completion should be explicit messages and deterministic transitions.

### F6 — P1: duplicated editing-target selection can select the hidden main editor

**Evidence**

- `features/editing/mod.rs:30-210` and the `*_with` functions in `change.rs`/`yank.rs` correctly operate on `impl EditingTarget`; this reusable core should be preserved.
- `editing/mod.rs:13-17,106-110,138-142`, `change.rs`, and `yank.rs` repeatedly use `VcsCommit => commit, _ => main`. The wildcard also covers `Vcs` and `VcsHistory`, so direct semantic commands or custom bindings can edit and auto-save the hidden main session.
- `yank.rs:170-204` constructs a main paste target on non-commit pages but later accepts it only on `AppPage::Editor`; direct fallback paste at `:156-160` has no equivalent page guard. Clipboard-provider and direct-register paths therefore enforce different policy.
- `manager/message_manager.rs:89-104` invokes semantic commands without checking their valid context; `keyboard_input_registry.rs:74-88` checks command existence, not whether a command is appropriate for the configured context.
- `line_jump.rs:138-159` duplicates editor/explorer/VCS-history selection in leaf helpers, with count policy and completion policy free to drift apart.

The central problem is duplicated target policy and a correctness-unsafe fallback, not merely that an adapter stores `&mut Model`.

**Refactor**

Create one root editing-target selector with exhaustive page handling: `Editor` → main, `VcsCommit` → commit when editing is enabled, and `Vcs | VcsHistory` → no target. Editing, change, yank, direct paste, and asynchronous paste request/completion must use the same target descriptor and availability policy. Add page-isolation tests before changing the selectors.

Keep `EditingTarget` and the `_with` operations unchanged initially. Root adapters may remain `Model`-aware during this correctness patch. Replacing their internal `model` field with disjoint borrows (`EditorSession`, context stack, register, narrow save policy) is a later optional narrowing, not the measure of success.

For line jump, parse into a `LineJumpAction` using an explicit `accepts_count` input, then have one root selector apply completion through `VerticalNavigation`. Do not let the prefix parser import concrete VCS history navigation.

Do not immediately split `EditingTarget` into many traits; split only if real implementors are forced to provide irrelevant policy.

### F7 — P2: local feature state exists, but command adapters still use global authority

**Evidence and disposition**

- `theme_picker/mod.rs:15-69`: small global wrapper around otherwise local state and theme policy. This is the best first pilot.
- `file_picker/commands.rs:10-95`: workspace, status, focus, context, document opening, and picker state are all accessed through `Model`, while `state.rs` already has local transitions. Its entry/preview caches also need explicit invalidation when external-sync notifications are redesigned; the sync coordinator should send semantic change events, not mutate picker internals directly.
- `project_grep/mod.rs:71-134`: similar adapter coupling; `state.rs` and `work.rs` already separate local search and owned work well.
- `buffer_grep/mod.rs:70-168` and `work.rs:49-60`: local search state is combined with global editor identity, preview, focus, status, and context. Extract a narrow active-buffer capability and return a navigation outcome. Validate source identity for every action—including Backspace, selection, acceptance, and external revision changes—not only printable insertion; make preview mutation private to the controller.
- `buffer_jump/mod.rs`: local jump-session state is good; command acceptance and focus should use a narrow editor target. Add revision validation before acceptance, and avoid clearing unrelated status on open.
- `explorer/commands.rs`: mostly needs explorer state, workspace root, and status. Extract those inputs without creating a filesystem trait unless tests require one. Separately, explorer tree reads currently perform synchronous filesystem I/O inside update; preserve the transactional tree/selection behavior, and move traversal to a generation-checked request only as a dedicated responsiveness slice.
- `page_scroll.rs`: needs only `PageScrollState` and `EditorSession`; this is a simple mechanical narrowing.
- `shell/focus.rs`: needs a shell focus/context state plus explicit editor/explorer availability, not the entire application.

These are not equally dangerous, but they are ideal early slices because existing behavior tests provide characterization coverage and state methods already exist.

### F8 — P2: presentational components query `Model` instead of receiving props

**Evidence**

- `page/vcs_history/history_list.rs:11-79` and `commit_detail.rs:11-92`: components hold `&Model` just to read history fields and theme.
- `page/vcs/change_section.rs:11-128`: change rendering reaches into VCS and workspace through the root.
- `page/vcs_commit.rs:87-139`: header formatting accepts `Model` for staged changes, notice, theme, and workspace.
- `component/file_picker.rs`, `features/buffer_grep/component.rs`, `project_grep/component.rs`, `buffer_jump/component.rs`, and `theme_picker/component.rs`: overlay providers and renderers use global read access.
- `component/diff.rs`: a shared presentation component imports the VCS feature's `DiffState`; it should accept presentation data rather than depend on an optional feature's state type.
- `ui.rs:19-23,191-218`: registry adapters also use `&Model`; these are legitimate composition points, unlike the leaf formatting helpers.

Read-only access cannot create mutation bugs, but it still obscures inputs, makes isolated rendering tests construct unrelated state, and encourages future mutation.

**Refactor**

Pages may read `Model` and compose views. Leaf components should receive concrete props such as `&HistoryState`, selection/expansion values, workspace root, and `UiTheme`, or a small page-specific view struct. Keep the existing UI registry initially; make each registered provider a thin root-to-props adapter. Do not create a large universal `AppViewModel` duplicating the entire model.

### F9 — P2: central routing still contains feature-specific policy

**Evidence**

- `manager/message_manager.rs:57-65` names file-picker scan/filter/preview messages explicitly.
- `message_manager.rs:70-83` resizes both the main and VCS commit editor.
- `message_manager.rs:107-129` contains explorer open and document-open follow-up policy.
- `runtime.rs:51-54` starts VCS work directly, and `runtime.rs:75-78` selects commit versus main editor mode by page.
- `manager/command_registry.rs:11` and `text_input_registry.rs:10` type every handler as a global-model function.
- `lib.rs:28-58` makes the root data module import every feature state, and all `Model` fields are public.

Some central composition is unavoidable and desirable. The problem is domain policy inside it, not the existence of a root.

**Refactor**

- Keep uniform registry functions as explicitly named **root adapters**. Their only job is mapping a `CommandId`/character to a typed feature command and assembling narrow inputs.
- Route feature-owned completions/messages to their owners without the manager understanding payload details.
- Let the active page expose resize/cursor requirements through a small shell adapter rather than making runtime know commit internals.
- Restrict field visibility after call sites move behind methods. Keep typed root fields; do not substitute a dynamic state bag.
- Treat root resize/page dispatch as low priority unless it is changing frequently.

## 4. Target design patterns

### 4.1 Private state object with methods

Use for pickers, searches, navigation, and controller state that lives for the feature lifetime.

```rust
struct ThemePickerState {
    phase: PickerPhase,
    selected: ThemeName,
}

impl ThemePickerState {
    fn selected_theme(&self) -> ThemeName {
        self.selected
    }

    fn close(&mut self) {
        self.phase = PickerPhase::Closed;
    }
}
```

The root adapter applies the selected `Theme` and updates status/context. Tests can instantiate the picker without `Model`.

### 4.2 Explicit workflow object

Use for a logical operation spanning messages. The object is retained only while needed and cleared by an explicit transition.

```rust
enum MergePhase {
    Closed,
    Loading { request: RequestToken },
    Selecting { repository: PathBuf },
    Running { request: RequestToken },
}

struct MergeWorkflow {
    phase: MergePhase,
    query: String,
    targets: Vec<String>,
    selected: Option<usize>,
    error: Option<String>,
}

enum MergeOutcome {
    None,
    Work(MergeWork),
    Completed(MergeCompleted),
}
```

`MergeWorkflow` may validate filtering, selection, tokens, and completion. It must not refresh the explorer or reload editor buffers. Those are consequences of `MergeOutcome::Completed`, handled by the root adapter.

### 4.3 Narrow concrete context first, trait when variation requires it

A context can be a plain struct of disjoint borrows:

```rust
struct BufferGrepContext<'a> {
    editor: &'a mut EditorSession,
    contexts: &'a mut ContextStack,
    status: &'a mut Option<StatusMessage>,
}
```

This already enforces least authority. Introduce a trait when there are multiple real policies or a fake is needed at an external boundary:

```rust
trait ActiveBuffer {
    fn source_snapshot(&self) -> Option<BufferTextSnapshot>;
    fn identity(&self) -> Option<(BufferId, i32)>;
    fn move_to(&mut self, position: DocumentPosition) -> Result<(), EditorError>;
}
```

The trait belongs to the consumer that needs it. The production adapter delegates to `EditorSession`. Static generics are sufficient unless registration genuinely requires dynamic dispatch.

### 4.4 Semantic outcomes instead of feature-to-feature calls

Prefer a small feature-specific outcome:

```rust
enum FilePickerOutcome {
    None,
    Work(FilePickerRequest),
    Open(OpenLocation),
}
```

The root maps `Open` to the document-open workflow. The picker does not import document, focus, explorer, or VCS modules. Avoid a universal outcome struct containing every possible field for every feature.

### 4.5 Typed view props

```text
VcsHistory page (may read Model)
└── HistoryList::new(HistoryListProps { ... })
    ├── history state and selection
    ├── expanded commit/files state
    └── resolved UiTheme
```

Props should be borrowed and read-only where practical. Presentation formatting remains in page/component code, not in `crates/vcs`.

## 5. Complete production-file disposition

This inventory covers every production file found by the textual `Model`-reference scan. “Keep adapter” means the signature is acceptable only while the body remains thin; it does not approve domain logic behind that signature.

| Files | Disposition |
| --- | --- |
| `runtime.rs` | Keep root `apply_message`; move VCS startup and page-specific cursor policy behind shell/page adapters when touched. |
| `manager/command_registry.rs`, `manager/text_input_registry.rs` | Keep uniform registration boundary, but rename/document handlers as root adapters and delegate immediately to narrow feature methods. |
| `manager/effect.rs` | Keep worker abstraction; reduce `Completion` implementations to routing adapters over typed feature `apply` methods. |
| `manager/message_manager.rs` | Keep Elm root dispatch; remove file-picker payload policy, open-follow-up policy, and feature-specific resize knowledge over staged slices. |
| `manager/event_manager.rs` | Read-only input translation is an acceptable adapter. A small input-context snapshot is optional, not an early priority. |
| `shell/lifecycle.rs` | Extract private lifecycle controller and owner-aware save continuation; remove direct VCS calls. |
| `shell/buffer.rs` | Split save/reveal primitives from buffer-switch workflow; return change outcomes instead of directly calling VCS. |
| `shell/focus.rs` | Operate on focus/context state and explicit availability inputs. |
| `capabilities/document.rs` | Make a real document-opening state/capability object; return opened results, with no explorer side effect. |
| `capabilities/register.rs` | Keep owned clipboard request; completion should target a narrow status sink through an adapter. |
| `features/editing/mod.rs` | Keep `EditingTarget` core and one target-selection adapter; remove repeated page checks in consumers. |
| `features/editing/target.rs` | Replace `&mut Model` fields with disjoint field borrows and narrow save policy. |
| `features/change.rs`, `features/yank.rs` | Preserve target-based text operations; encapsulate pending sessions and reuse one editing-target adapter. |
| `features/line_jump.rs` | Encapsulate count/prefix state and select `VerticalNavigation` at the root rather than importing VCS history policy into the workflow. |
| `features/buffer_navigation.rs`, `features/view_align.rs` | Encapsulate prefix lifecycle and return semantic movement/alignment outcomes. |
| `features/page_scroll.rs` | Narrow to page-scroll config/state plus `EditorSession`; no new trait needed. |
| `features/file_picker/commands.rs` | Assemble workspace/modal inputs and map `FilePickerOutcome`; keep existing state transition methods. |
| `component/file_picker.rs` | Convert overlay provider/renderers to thin adapters plus typed picker props. |
| `features/buffer_grep/mod.rs`, `features/buffer_grep/work.rs` | Extract active-buffer context and typed completion application; return cursor-navigation outcome. |
| `features/buffer_grep/component.rs` | Use buffer-grep props rather than `Model`. |
| `features/project_grep/mod.rs`, `features/project_grep/work.rs` | Keep local state/owned requests; narrow open/accept and completion adapters. |
| `features/project_grep/component.rs` | Use project-grep props rather than `Model`. |
| `features/buffer_jump/mod.rs` | Keep local session methods; use a narrow active-editor target and focus outcome. |
| `features/buffer_jump/component.rs` | Pass jump-session state, editor snapshot, and theme explicitly. |
| `features/theme_picker/mod.rs` | First pilot for a narrow root adapter and private state transitions. |
| `features/theme_picker/component.rs` | First pilot for typed overlay props. |
| `features/explorer/commands.rs` | Pass explorer state, workspace root, and status explicitly; keep filesystem logic behind existing helpers. |
| `features/external_sync.rs` | Separate event classification/reload capability from VCS/explorer orchestration. |
| `features/vcs/mod.rs` | Split repository, changes, history, and buffer-diff controllers; move all completion transitions behind methods. |
| `features/vcs/commit.rs` | Extract explicit commit workflow; coordinate with save infrastructure through typed events, not shell calls into VCS. |
| `features/vcs/merge.rs` | Extract explicit merge phase/workflow; return success for root refresh/reload orchestration. |
| `page/editor.rs` | Page-level read-only composition is acceptable; pass narrower props to children when touched. |
| `page/vcs/mod.rs`, `change_section.rs` | Page composition may read root; merge popup and change rows should receive VCS/view props. |
| `page/vcs_history/mod.rs`, `page/vcs_history/history_list.rs`, `page/vcs_history/commit_detail.rs` | Page composition may read root; list/detail/line helpers should receive history props. |
| `page/vcs_commit.rs` | Compose from commit/changes/workspace props; header helpers should not accept `Model`. |
| `ui.rs` | Keep registry adapters and layout policy initially; registered contributions should immediately forward typed props to leaf components. |

## 6. Incremental migration plan

Apart from explicitly reviewed correctness fixes identified in Step 0, each step is behavior-preserving. Steps show dependency order; each named feature within a step should still be its own small patch. Stop after each patch for review. Do not combine architectural moves with new features, Git changes, or save-policy changes.

### Step 0 — Characterize behavior, guard correctness, and set dependency rules

- Keep the current workspace/integration tests as the behavior baseline.
- Write down the allowed `Model` boundaries from Section 2 in review guidelines.
- For new code, prohibit `Model` imports in feature `state.rs`, `work.rs`, and presentational component files unless explicitly reviewed as an adapter.
- Add focused tests before moving save, commit, external reload, and merge logic. Existing tests are useful but should not be the only coverage for those high-risk slices.
- Add reproduction tests for non-editor-page editing and direct/clipboard paste parity, cross-prefix cancellation, paste after cursor movement, and buffer-jump acceptance after external reload. If a defect is confirmed, fix it in a separately reviewed correctness patch before the mechanical refactor; do not hide that behavior change inside a move.
- Add VCS scheduling tests for two index mutations before completion, earlier backend success arriving after a newer completion, external worktree/index/save invalidation, history detail Loading/Failed command routing, cached-B-during-uncached-A detail completion, overlapping commit and close/quit barriers, and duplicate merge/watcher refreshes. Prioritize confirmed index/UI divergence and hidden-editor fixes over optional cosmetic pilots.
- Before optional pilots, land separate small correctness slices for confirmed critical races. In particular, `StatusController` single-flight plus authoritative status reconciliation and history-detail request identity should precede broader VCS movement; these new boundaries can be reused by Step 6.

**Exit:** baseline tests pass, adjacent correctness guards are explicit, and reviewers can distinguish root composition from leaf coupling.

### Step 1 — Prove the pattern with theme picker and page scroll

- Move theme open/close/apply transitions behind `ThemePickerState` methods with explicit outcomes.
- Make the registered theme overlay a root-to-props adapter.
- Narrow page-scroll handling to its config/state and `EditorSession`.
- Keep command IDs, keymaps, rendering, and effect APIs unchanged.

**Exit:** feature unit tests need no `Model`; existing adapter integration tests still pass; no global registry redesign.

### Step 2 — Apply the pattern to merge, then picker/search workflows

- Convert merge booleans to explicit phases and move filter/selection/completion logic into `MergeWorkflow`.
- Return `MergeCompleted` to a thin adapter; initially leave the existing refresh/reload implementation in that adapter.
- Narrow file picker, project grep, buffer grep, and buffer jump one at a time, reusing their state methods.
- Convert file-picker scan/filter/preview to a typed `FilePickerCompletion`, removing its three global `Message` variants and manager arms without changing revision checks.
- Convert their leaf renderers to typed props.
- Keep background request execution and stale-result semantics unchanged.

**Exit:** workflow state and stale completions are tested without `Model`; only small adapters access the root; no cross-feature behavior changes yet.

### Step 3 — Consolidate editing and prefix sessions

- After target selection is correct, optionally narrow main/commit editing adapters from a `Model` field to disjoint field borrows; keeping thin root-aware adapters is also acceptable.
- Centralize editing-target selection for editing/change/yank with exhaustive page handling: Editor → main, VcsCommit → commit, Vcs/VcsHistory → no target.
- Encapsulate change/yank/line-jump/buffer-navigation/view-align start-input-cancel transitions.
- Make pending prompt and context changes atomic from each workflow's perspective, with owner-tagged cancellation and an explicit page/focus transition policy.
- Extract a shared modal helper only after several concrete implementations prove the same need.

**Exit:** main and commit editing behavior is unchanged; register/auto-save policies remain distinct; invalid prefix cleanup and cancellation tests pass without global fixtures.

### Step 4 — Decouple document, buffer, and external-sync orchestration

- Introduce `DocumentOpening` with generated request IDs, pending origin/location, and typed stale/failure/success results.
- Move picker close, focus, explorer reveal, and VCS synchronization to the root open coordinator; test same-path out-of-order completion and completion while an unrelated modal is active.
- First extract a typed `buffer::switch(movement)` shared by direct and prefix commands so string round-tripping disappears without losing save/reveal/VCS behavior. Then return buffer-switch outcomes instead of calling VCS from the buffer leaf.
- Return external-sync intents and route them through an application `RepositorySyncWorkflow` with event-specific invalidation and one-in-flight-plus-trailing-dirty coalescing. Include semantic file-picker cache invalidation without reaching into picker state from the watcher.
- Move merge's refresh/reload consequences from its adapter into the same explicit coordination path; avoid duplicate watcher-triggered reads and reloads.

**Exit:** document opening and buffer switching have no direct feature-to-feature calls; stale opens, dirty reload conflicts, inactive reloads, explorer reveal, and VCS diff-base behavior retain integration coverage.

### Step 5 — Separate save/lifecycle/commit ownership

- Add owner-aware save continuations and a private lifecycle controller, including an explicit reject/queue/cancel policy for overlapping commit and close/quit goals.
- Route save results to automatic-save, close/quit, or commit owners without shell → VCS calls.
- Make commit's `Editing/Saving/Committing` transitions methods on `CommitWorkflow`.
- Preserve no-autosave for the commit scratch session and no automatic save before merge.

**Exit:** save failure and stale completion tests cover every owner; close/quit safety, follow-up saves, commit retry/cancellation, and commit success behavior are unchanged. This is the highest-risk step and should not be bundled with Step 4 or 6.

### Step 6 — Decompose the remaining VCS aggregate

- Complete any changes/staging/diff extraction not already landed in the early `StatusController` correctness slice.
- Complete history/pagination/detail extraction around the request-identity and explicit-pane fixes from Step 0.
- Extract HEAD/repository and buffer-diff-base state, including buffer-close/reload cleanup and duplicate-load prevention.
- Move completion token validation next to the state it protects, including history-detail commit identity.
- Make VCS fields private and expose read-only view queries/props.
- Keep the production Git backend and all fast-forward/staging/commit semantics unchanged.

**Exit:** VCS operations can be unit-tested with local state and owned completions; page and runtime adapters contain coordination only; no completion reaches into unrelated VCS substate.

### Step 7 — Clean up routing and enforce the new boundaries

- Remove obsolete central file-picker message/payload handling where typed completion routing has replaced it.
- Move page-specific resize/cursor policy behind page/shell adapters.
- Rename broad registry handler types to make their adapter role explicit.
- Reduce `Model` field visibility and remove stale re-exports/imports.
- Re-run the textual scan and review remaining `Model` occurrences individually. The goal is not zero; it is that every remaining occurrence is an approved composition point.

**Exit:** architecture documents match implementation, new features have a clear local path, and global-model usage is intentional rather than habitual.

## 7. SOLID review guide for each patch

### Single responsibility

A module should have one reason to change. `MergeWorkflow` changes for merge behavior; `DocumentOpening` changes for safe insertion; root coordination changes for cross-feature policy. If a save implementation changes and VCS commit code must also change, ownership is still wrong.

### Open/closed

A new save consumer or buffer-change follower should be added through a typed owner/outcome and root registration, not by editing an unrelated feature's completion function. Static Rust composition still requires explicit wiring; zero wiring is not the goal.

### Liskov substitution

There is no broad inheritance problem to fix. For narrow traits such as `EditingTarget` or a future `ActiveBuffer`, every implementation must honor revision, availability, context, and effect contracts. Do not create a substitute implementation that silently skips required invariants.

### Interface segregation

`&mut Model` is the largest interface. Prefer the smallest concrete context that supports the operation. Split traits only when consumers need different method sets; a one-method trait taking `Model` is still a fat interface in practice.

### Dependency inversion

High-level workflows should depend on semantic inputs/outcomes and narrow capabilities. Concrete root adapters should depend on `Model`, `EditorSession`, filesystem/Git implementations, and Ratatui. Feature modules should not call each other's concrete command handlers or mutate each other's fields.

## 8. Tests and architectural guardrails

For every slice:

1. **Local unit tests:** construct only feature/workflow state and narrow fakes or concrete capabilities. Test valid transitions, invalid phases, cancellation, errors, and stale tokens.
2. **Adapter integration tests:** retain current end-to-end command/message tests to prove keymaps and real root wiring.
3. **Cross-feature regression tests:** explicitly cover open → reveal/focus/VCS, buffer switch → save/reveal/VCS, external reload → conflict/VCS base, save → close/quit, save → commit, and merge → refresh/reload when that slice changes.
4. **Rendering tests:** construct typed props and use `TestBackend`; do not require unrelated VCS/editor/lifecycle state.
5. **Validation:** run `cargo fmt --all`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `git diff --check` after every slice.

Useful structural checks during review:

- `state.rs` and domain workflow files do not import `data_object::Model`.
- `FeatureEffect::perform` uses only owned request data and backend capabilities.
- A `Completion` adapter delegates rather than implementing unrelated policy.
- No feature imports another feature's `handle_command` or writes its state directly.
- Page-local formatting functions accept props rather than `Model`.
- Remaining `&mut Model` functions are listed intentionally as root adapters.

Avoid brittle source-text linting as the first enforcement mechanism. Module visibility, small interfaces, and code review should carry the design; automated dependency checks can be added later if violations recur.

## 9. Definition of done for the overall refactor

The refactor is complete when:

- Feature domain logic and workflow transitions can be tested without constructing `Model`.
- Persistent state is private to its owning feature/controller, with explicit phase types for multi-event workflows.
- Background jobs own their inputs and never access application state off the update thread.
- Completion and command registries retain only thin, identifiable root adapters.
- Shell lifecycle has no VCS-specific branches, and document/buffer/external-sync primitives have no hidden feature side effects.
- Presentational leaf components receive resolved read-only props.
- Existing editor safety, auto-save, close/quit, external reload, search, and VCS behavior passes all regression and smoke tests.
- Remaining `Model` references are deliberate root composition, routing, or adapters—not merely unchanged legacy coupling.
