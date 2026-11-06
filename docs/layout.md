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
└── which-key panel: displayed/hidden; bottom of main, above status bar
```

## Region constraints

| Region | Layout constraints |
| --- | --- |
| Root | `main = Fill(1)`, `status = Length(1)` |
| Main | `file explorer = Length(28)` when displayed, `editor = Fill(1)`, `tools = Length(32)` when displayed |
| Editor | `buffer tabs = Length(1)`, `editing area = Fill(1)` |
| Which-key overlay | Bottom of `main`, height `Length(3)` |

All layout groups use `Flex::Start`. Hidden nodes remain in the layout tree and use zero width, so they do not reduce the editor's fill area.

## Ownership and layout rules

- The page owns root/main regions and component composition.
- The editor component owns its vertical tabs/content split.
- `Editor::content_area` is the shared text-area calculation for viewport sizing and cursor coordinates.
- Add future panels as explicit layout regions rather than embedding them inside unrelated components.
- Keep layout calculations in shared Flex layouts so changes do not require hard-coded offsets.
- Update this file and layout tests whenever the hierarchy or constraints change.
