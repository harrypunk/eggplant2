# UI layout trees

Use ASCII trees in Markdown for layout documentation. They are readable without a browser, easy to review in diffs, and sufficient for the current component hierarchy. Mermaid is unnecessary unless we later need complex flow or dependency diagrams.

## Current editor page

```text
Root: vertical Flex
├── main: horizontal Flex
│   ├── file explorer: displayed/hidden
│   ├── editor: vertical Flex
│   │   ├── buffer tabs
│   │   └── editing area
│   └── tools: displayed/hidden
└── status bar

Overlay layer:
└── popup container: cleared, bordered, titled
    └── key hints: displayed/hidden; bottom-right of main, above status bar
```

## Region constraints

| Region | Layout constraints |
| --- | --- |
| Root | `main = Fill(1)`, `status = Length(1)` |
| Main | `file explorer = Length(28)` when displayed, `editor = Fill(1)`, `tools = Length(32)` when displayed |
| Editor | `buffer tabs = Length(1)`, `editing area = Fill(1)` |
| Key-hints overlay | Bottom-right of `main`; content-sized vertical list with borders, above the status bar |

All layout groups use `Flex::Start`. Hidden nodes remain in the layout tree and use zero width, so they do not reduce the editor's fill area.

## Ownership and layout rules

- The page owns root/main regions and component composition.
- The editor component owns its vertical tabs/content split.
- `ToolsPanelState` owns only shell visibility and active-tool selection. Future search, agent, and info tools keep their state and behavior in separate modules; hiding the shell does not stop their work or discard their state.
- The tools panel component receives resolved shell props and delegates tool-specific rendering rather than implementing every tool itself.
- `Popup` is a reusable visual container: it clears the background, draws the border/title, and renders any `Widget` in the inner area. Pages own placement; input contexts own modality.
- `KeyHints` owns list presentation and intrinsic size; the page owns bottom-right popup placement.
- `Editor::content_area` is the shared text-area calculation for viewport sizing and cursor coordinates.
- Add future panels as explicit layout regions rather than embedding them inside unrelated components.
- Keep layout calculations in shared Flex layouts so changes do not require hard-coded offsets.
- Update this file and layout tests whenever the hierarchy or constraints change.
