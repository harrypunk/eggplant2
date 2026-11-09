# UI layout trees

Use ASCII trees in Markdown for layout documentation. They are readable without a browser, easy to review in diffs, and sufficient for the current component hierarchy. Mermaid is unnecessary unless we later need complex flow or dependency diagrams.

## Current editor page

```text
Root: vertical Flex
├── main: horizontal Flex
│   ├── file explorer: displayed/hidden
│   ├── editor: vertical Flex
│   │   ├── buffer tabs
│   │   └── editing area: horizontal Flex
│   │       ├── line-number gutter
│   │       └── document text
│   └── tools: displayed/hidden
└── status bar

Overlay layer:
└── popup container: cleared, bordered, titled
    ├── key hints: displayed/hidden; bottom-right of main, above status bar
    └── file picker: centered in main; query/results left, text preview right
```

## Region constraints

| Region | Layout constraints |
| --- | --- |
| Root | `main = Fill(1)`, `status = Length(1)` |
| Main | `file explorer = Length(28)` when displayed, `editor = Fill(1)`, `tools = Length(32)` when displayed |
| Editor | `buffer tabs = Length(1)`, `editing area = Fill(1)`; line-number gutter = document digits + one space, text = `Fill(1)` |
| Key-hints overlay | Bottom-right of `main`; content-sized vertical list with borders, above the status bar |
| File-picker overlay | Centered in `main`, maximum 120×24; left query/results and right preview in equal columns |

All layout groups use `Flex::Start`. Hidden nodes remain in the layout tree and use zero width, so they do not reduce the editor's fill area.

## Ownership and layout rules

- The page owns root/main regions and component composition.
- The editor component owns its vertical tabs/content split.
- `ToolsPanelState` owns only shell visibility and active-tool selection. Future search, agent, and info tools keep their state and behavior in separate modules; hiding the shell does not stop their work or discard their state.
- The tools panel component receives resolved shell props and delegates tool-specific rendering rather than implementing every tool itself.
- `Popup` is a reusable visual container: it clears the background, draws the border/title, and renders any `Widget` in the inner area. Pages own placement; input contexts own modality.
- `KeyHints` owns list presentation and intrinsic size; the page owns bottom-right popup placement.
- `FilePicker` composes the query, ranked result list, and preview inside `Popup`; it never performs filesystem I/O during rendering.
- `Editor::content_area` excludes tabs, and `TextView::text_area` excludes the line-number gutter. Together they are the shared text calculation for viewport sizing and cursor coordinates. `EditorBuffer` and picker previews both delegate rendering to `TextView`.
- Add future panels as explicit layout regions rather than embedding them inside unrelated components.
- Keep layout calculations in shared Flex layouts so changes do not require hard-coded offsets.
- Update this file and layout tests whenever the hierarchy or constraints change.
