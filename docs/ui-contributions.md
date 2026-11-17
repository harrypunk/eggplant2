# UI contribution model

The shell page owns layout and composition:

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

For example, `HistoryListProps` supplies only history state, selection, expanded commit identity, file details, and theme. `CommitDetailProps` similarly supplies detail state, file selection/expansion, diff state, and theme. `ChangeSectionProps` provides changes, selection, expanded diff identity, workspace root, and theme for staged and unstaged sections. Route pages assemble those props from `Model`; the components and their tests operate without application state. `CommitHeaderProps` likewise isolates staged-summary formatting to changes, workspace root, notice, and theme. The page-local `MergePopup` component separates its named query/targets/status layout from pure target-line and status formatting helpers.

The shell resolves overlay ordering by semantic priority, not accidental registration order. Feature components never query terminal size, perform filesystem work, or install global key handlers.

## Rendering and cursor policy

Rendering reads state and never performs updates or repository/filesystem work. The page computes all contribution rectangles, renders hints before modals, and suppresses the editor cursor whenever an overlay is present. The topmost overlay may publish its own cursor through `with_cursor`; the file picker uses this with the shared Unicode-aware `TextInput` component.

Ratatui `Widget` lifetimes and object safety should not drive a premature trait-object framework. Start with concrete components and small render-contribution adapters, and erase types only where the page boundary demonstrably requires it.

## Registration tests

- Duplicate panel/status slots and overlay IDs fail during registration.
- Modal precedence, cursor hiding, layout clamping, and registration ordering are tested at the shell boundary.
- A test-only feature must prove popup rendering and visibility cleanup through composition-root registration alone, without runtime changes.
