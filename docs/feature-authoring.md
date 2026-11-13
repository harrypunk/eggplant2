# Feature authoring checklist

Use this guide when adding an internal Eggplant feature through the M6 contribution contracts. It describes the current APIs; it is not an external-plugin ABI.

## 1. Start with the feature boundary

Before writing code, identify:

- Feature-owned state and invariants.
- Semantic commands and stable string IDs.
- Whether updates are pure or require effects.
- Whether the feature contributes a panel, overlay, or status item.
- Which existing capabilities it needs, such as document opening or dirty-save handling.
- Tests that prove behavior and registration conflicts.

Keep feature state, messages, update logic, effects, components, and tests together when practical. Do not add new arms to `MessageManager` or the runtime effect executor as a first step.

## 2. Add explicit typed state

Add a feature state type and compose it in `Model`. Static internal features should use explicit fields, not a global `Any` map or service locator.

Use semantic enums for meaningful states and `Option` when absence matters. Do not create independent copies of editor text, dirty state, cursor offsets, or viewport state.

The test-only `ExampleFeatureState` in `test_utils/example_feature.rs` demonstrates the composition pattern without adding a production placeholder feature.

## 3. Register semantic commands

Give each action a stable `CommandId`, description, and handler:

```rust
commands.register(Command::feature(
    CommandId("example.increment"),
    "increment example",
    handle_command,
))?;
```

The handler receives the model and command ID and returns an optional effect. Keep physical keys out of the handler: configured direct chords and built-in leader sequences resolve to the same command ID.

- Preserve existing IDs when renaming behavior.
- Use feature-specific prefixes for new IDs.
- Return an error status for an unexpected ID instead of silently ignoring it.
- Let duplicate registrations and duplicate chords fail at startup.

Compatibility `Message` handlers remain available for shell concerns, but new features should prefer feature handlers. For unbound printable input, register a `TextInputRegistry` handler for the feature context and consume the shared `Message::TextInput`; do not add a feature-specific character message or dispatcher arm.

## 4. Keep updates pure

An update should only change state and optionally return an effect. It must not:

- Read or write the filesystem directly.
- Spawn Tokio tasks.
- Query the terminal.
- Render widgets as a side effect.
- Mutate another feature's private state directly.

Cross-feature behavior goes through semantic commands or existing capabilities such as `document::open_file`, `buffer::start_save`, or `buffer::reveal_buffer`.

## 5. Use `FeatureEffect` for background work

Implement `FeatureEffect::perform` to return a completion `Message`. The default `execute` runs on the blocking pool and sends the completion through the existing channel.

```rust
impl FeatureEffect for ExampleEffect {
    fn perform(self: Box<Self>) -> Message {
        Message::InvokeCommand(CommandId("example.complete"))
    }
}
```

Return the request from update with `Effect::feature(request)`.

Override `execute` only when the work needs different scheduling, such as async save or main-thread construction of non-`Send` Helix values. Preserve request IDs, buffer IDs, and revisions so stale completions cannot replace newer state. The runtime must not gain a feature-specific effect arm.

## 6. Contribute UI through `UiRegistry`

Choose the smallest suitable contribution:

- `register_panel(PanelSlot::..., render)` for a shell panel slot.
- `register_status(StatusSlot::..., render)` for status content.
- `register_overlay(id, provider)` for conditional popup content.
- `register_editor_overlay(id, provider)` for decorations over the active editor's visible text. The renderer receives the existing snapshot and resolved text rectangle; it must not rescan the document or mutate state.

An overlay provider returns `None` while hidden and an `OverlayContribution` while visible. The contribution includes a semantic kind, placement, and render function. The shell computes all rectangles, orders hints before modals, and suppresses the editor cursor whenever an overlay is present.

Components receive resolved state/style props and do not:

- Compute global page layout.
- Control input contexts or modal precedence.
- Perform I/O or spawn effects during rendering.
- Query terminal capabilities directly.

Use concrete components and small render adapters; do not force every widget into a trait object.

## 7. Register at the composition root

Built-in registrations live in one composition path. A feature may add its own `register` function to group related commands and UI contributions, as the test-only example does.

The runtime and unrelated feature modules should not need edits. Adding a registration is allowed; adding feature-specific branches to central coordinators is not the goal.

Keep panel/status slots and overlay IDs unique. Treat duplicates as startup errors with actionable messages rather than silently replacing an existing contribution.

## 8. Preserve editor invariants

Every feature must retain:

- Always-on automatic save for modified file-backed buffers.
- Stable `BufferId` and revision-aware completion handling.
- UTF-8, regular-file-only document opening.
- Main-thread construction of non-`Send` Helix documents.
- One terminal owner and one application event/update loop.
- Shell-owned focus, modal precedence, layout, and cursor policy.

Future unattended edits must name an explicit buffer and expected revision; active-buffer UI commands are not an agent-safe mutation API.

## 9. Required tests

Include the closest practical set:

- State transitions and boundary conditions.
- Command registration and handler behavior.
- Effect execution and completion routing.
- Duplicate commands, slots, overlay IDs, and key chords.
- Stale request/revision completion rejection.
- Popup visibility, placement, and cursor suppression.
- Narrow terminals, Unicode, and empty states when applicable.
- Existing end-to-end behavior when touching shared capabilities.

The M6 proof is `feature_works_through_command_effect_and_ui_registration_only`: it registers typed state, a command, a popup, and an effect, then verifies rendering and completion without changing runtime dispatch.

## 10. Avoid premature abstraction

Free functions are sufficient for many features. Introduce a narrow context struct only when a `Model` parameter obscures real dependencies. Introduce a trait when substitution, testing isolation, or an external boundary needs it—not merely because a feature exists.

Do not add a fat `Feature` interface, a native dynamic-library ABI, or an external plugin host to solve an internal feature-registration problem.
