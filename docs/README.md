# Documentation guide

Start with the [project overview](../README.md), then use the focused document for each concern:

## User reference

- [Key input, sequences, and hints](key-input.md) — complete keybindings, input contexts, and sequence behavior.
- [Smoke test](smoke-test.md) — automated checks and the real-terminal Linux procedure.

## Core editor

- [Editor design with Helix and Elm](editor-design.md) — buffer/view ownership, empty-folder state, message flow, viewport, and persistence.
- [Filesystem work and the file picker](filesystem.md) — opening, previews, background effects, and mutation rules.
- [UI layout trees](layout.md) — component hierarchy, region ownership, and layout rules.
- [Themes](theme.md) — built-in themes, semantic roles, and resolved styles.

## Feature architecture

- [Feature architecture](feature-architecture.md) — high-level ownership and dependency rules.
- [Feature composition](feature-composition.md) — React-style interfaces, adapters, reusable components, and effect policies.
- [Command and effect routing](command-effects.md) — semantic commands and worker completion boundaries.
- [UI contributions](ui-contributions.md) — panels, overlays, status slots, and cursor policy.
- [Feature authoring](feature-authoring.md) — registration and testing checklist.
- [Milestone history](milestone-history.md) — completed architecture and editing slices.

## Optional addons

- [VCS addon](vcs.md) — optional Git status, staging, and inline diff workflows.

## Planning

- [Roadmap](../PLAN.md) — current state and upcoming reviewable milestones.
- [Binary size plan](binary-size.md) — deferred measurement, diagnostics, compiler, and dependency-growth policy.
