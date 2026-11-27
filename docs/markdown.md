# Markdown rendering

Assistant responses are Markdown snapshots rendered for display only. Provider history, persisted messages, tool input, and user drafts retain their original text.

## Dependency identity: do not confuse these crates

Eggplant uses:

```toml
[dependencies]
markdown-ratatui = "0.2"
```

- Repository: **https://github.com/karanabe/mira**
- Crate: https://crates.io/crates/markdown-ratatui
- License: **MIT OR Apache-2.0**
- Parsing model: `markdown-model`, backed by `pulldown-cmark`

This is **not** the similarly named package below:

```toml
[dependencies]
ratatui-markdown = "0.3" # Not the Eggplant dependency
```

- Different repository: **https://github.com/celestia-island/ratatui-markdown**
- Its latest 0.3.7 crates.io metadata reports **`LicenseRef-sysl-1.0`**, a custom license. Earlier releases have different metadata; no version of that package has been adopted or license-audited for Eggplant.
- Do not silently substitute this package when upgrading or copying the dependency example.

## Reference implementations

Pi separates display-only transforms, a Markdown token tree, semantic styles, width-aware nested layout, and terminal output. It reparses immutable streaming snapshots and injects code highlighting into the generic renderer. Eggplant follows those boundaries without Pi's JavaScript runtime or ANSI-string intermediates.

Helix also uses `pulldown-cmark` for LSP documentation and its syntax loader for fenced-code highlighting. Its renderer belongs to `helix-term`, Helix's own TUI and editor component runtime, and has narrower table/list/link behavior. Eggplant can reuse the highlighting concept through an adapter, but does not adopt the Helix UI renderer.

## Eggplant boundaries

```text
AgentSession / immutable message text
  ↓
Agent transcript role policy
  ├─ user messages: plain text
  ├─ tool events and approvals: plain/diff rows
  └─ assistant messages: Markdown
       ↓
component/markdown::PreparedMarkdown
  ├─ markdown-model::Document
  ├─ markdown-ratatui::Layout (width, Unicode wrapping, styles, limits)
  └─ caller-owned top clipping
       ↓
ratatui::Buffer
```

`component/markdown/mod.rs` is the package-facing adapter and maps `UiTheme` roles to Markdown text, heading, link, code, diagram, muted, and selection styles. `component/agent/transcript.rs` composes Markdown and plain blocks, supplies role prefixes, and bottom-anchors both Agent presentations using actual wrapped block heights. Neither adapter performs filesystem, network, provider, or terminal-probing work.

The package provides CommonMark plus tables, task lists, and strikethrough, nested block layout, code wrapping, narrow-table fallback, and bounded parsing/layout. Parse or layout-limit failures fall back to readable raw message text. Trailing package block-spacing rows are excluded when the Agent transcript supplies its own inter-message spacing.

Streaming uses complete immutable snapshots, matching Pi's correctness model. There is no incremental parser state to corrupt on partial delimiters. The current view prepares snapshots on demand; persistent `Arc<Document>`/`MarkdownView` caches remain a measurement-driven optimization rather than hidden global state.

Code fences currently use semantic code styling without language-specific highlighting. A future highlighter should be injected behind the Markdown adapter, with plain-code fallback for unknown languages or highlight failures. Mermaid, math, terminal images, OSC 8 hyperlinks, and additional transforms are separate policies, not reasons to couple the Agent session to a renderer.

## Regression coverage

Tests cover common blocks and inline cell styles, GFM tables/tasks/quotes, unclosed streaming fences, Unicode width limits, top clipping, raw user prompts versus rendered assistant Markdown, both Agent surfaces, and bottom anchoring after wrapping. Component tests construct local sessions and buffers; no provider or network access is required.
