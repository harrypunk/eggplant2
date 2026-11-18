# Eggplant Editor

Eggplant is a small **AI-first, keyboard-driven terminal editor** written in Rust. It aims to make exploration, editing, and future AI-assisted workflows use the same safe application commands and effects rather than separate, tightly coupled integrations.

The current application is intentionally lightweight: it is not a full IDE, a Vim/Helix clone, or a plugin platform.

## What it provides

- A Linux terminal UI built with Ratatui.
- File exploration and multiple UTF-8 text buffers.
- A small Vim-inspired Normal/Insert/Visual editing model with exactly one cursor/selection.
- Automatic persistence after edits and before close/quit, with revision and external-change protection.
- Fuzzy file opening, current/project search, and built-in themes.
- A separate **VCS addon** for branch display, switching/creation, changed files, staging, commits, and inline diffs.
- Narrow boundaries for future AI commands instead of direct widget or Helix access.

## Run

```sh
cargo run -p tui -- [FILE_OR_FOLDER]
```

Without a path, Eggplant opens the empty editor. A file opens a buffer; a folder opens a workspace explorer without implicitly opening a document.

The application reads `~/.eggplant/config.toml` when present. See `config.example.toml` for options and `docs/key-input.md` for the complete keyboard reference.

## Architecture

```text
crates/
├── editor/    # Documents, buffers, editing, viewport, and persistence
├── tui/       # Terminal runtime, application state, features, and rendering
└── vcs/       # Optional Git capability with private Helix/Gitoxide types
```

Eggplant uses selected Helix primitives without adopting Helix's terminal application, keymaps, or editor controller. One Elm-style event/update/effect loop owns state transitions, while background work returns completion messages.

The VCS integration is an **addon**, not a requirement of the core editor. Editing and persistence do not depend on Git or on the VCS page.

## Documentation

- [Key input](docs/key-input.md) — complete keybindings, input contexts, and sequence behavior.
- [Editor design](docs/editor-design.md) — core document, buffer, viewport, and persistence invariants.
- [Feature architecture](docs/feature-architecture.md) — extension ownership and dependency rules.
- [VCS addon](docs/vcs.md) — optional Git status, staging, and diff workflows.
- [Filesystem](docs/filesystem.md) — opening, previews, background work, and mutation rules.
- [Themes](docs/theme.md) — built-in themes and resolved styles.
- [Theme providers](docs/theme-providers.md) — proposed adapter architecture for external theme formats.
- [Milestone history](docs/milestone-history.md) — implemented architecture and editing slices.
- [Command and effect routing](docs/command-effects.md) — semantic commands and worker completion boundaries.
- [Transient notifications](docs/notifications.md) — source-independent one-shot feedback and expiry composition.
- [UI contributions](docs/ui-contributions.md) — panels, overlays, status slots, and cursor policy.
- [Smoke test](docs/smoke-test.md) — automated and real-terminal validation procedures.
- [Roadmap](PLAN.md) — current state and upcoming reviewable slices.

## Development

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Feature-specific documentation lives beside the architecture guides rather than in this introduction.
