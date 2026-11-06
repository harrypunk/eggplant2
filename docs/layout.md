# UI layout trees

Use ASCII trees in Markdown for layout documentation. They are readable without a browser, easy to review in diffs, and sufficient for the current component hierarchy. Mermaid is unnecessary unless we later need complex flow or dependency diagrams.

## Current editor page

```text
Root: vertical Flex
├── main: horizontal Flex
│   ├── file explorer
│   │   └── fixed-root filesystem tree
│   ├── editor: vertical Flex, borderless
│   │   ├── buffer tabs
│   │   │   └── filename · active highlight · dirty marker
│   │   └── editing area
│   └── tools: optional, currently hidden
│       └── future agent / search / file-info panel
└── status bar
    ├── mode: NORMAL / INSERT
    └── status or error message
```

## Ownership and layout rules

- The page owns the root/main regions and component composition.
- The editor component owns its vertical tabs/content split.
- `Editor::content_area` is the shared text-area calculation for Helix viewport sizing and terminal cursor placement.
- Tools have an optional layout region but are not rendered while hidden.
- Add future panels as explicit layout regions rather than embedding them inside unrelated components.
- Keep layout calculations in shared Flex layouts so style changes do not require hard-coded offsets.
- Update this file and the layout/cursor tests whenever the hierarchy changes.
