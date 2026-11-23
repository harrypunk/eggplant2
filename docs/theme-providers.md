# Theme providers and adapter design

Status: Helix and Ghostty providers, configuration, generation-checked file loading, local theme discovery, case-insensitive picker filtering, registered provider sections, and atomic selection persistence are implemented. Additional provider formats are pending.

## Purpose

Eggplant should be able to accept themes from different ecosystems without making its components understand Ghostty configuration, Neovim highlight groups, VS Code JSON, or Helix theme files.

The main goal is **dependency inversion**, not a universal theme format or plugin framework.

- Components consume a small set of semantic UI roles and exact syntax scopes.
- Provider adapters translate external formats into provider-neutral theme drafts.
- One resolver validates and completes a theme before installation.
- Concrete provider selection lives only at the application composition root.

## Non-goals

- Do not build a general plugin runtime, dynamic Rust ABI, or global service locator.
- Do not make Ghostty, Neovim, or VS Code filenames part of component APIs.
- Do not infer complete syntax highlighting from a UI-only color palette.
- Do not execute Ghostty, Neovim, VS Code, Lua, JavaScript, or extension code to obtain colors.
- Do not silently mix arbitrary palettes when a source is incomplete or invalid.
- Do not add runtime color keys such as `magenta`, `purple`, or `blue` to component code.

## Dependency rule

```text
                 Provider I/O
                      │
                      ▼
              Provider document
                      │
                      ▼
             Pure format adapter
                      │
                      ▼
               ThemeDraft + diagnostics
                      │
                      ▼
        Resolver: fallback + validation
                      │
                      ▼
             Complete immutable Theme
                      │
                      ▼
       UiTheme + exact syntax highlighting
                      │
                      ▼
            Pages and components
```

Dependencies point inward:

- The theme core knows semantic roles, validation, and resolution policy.
- Adapters know their input format and the canonical draft, not application components.
- The composition root knows which concrete adapter to select.
- Components know only resolved semantic styles, never provider identity.

## Canonical model

Use two models with different completeness guarantees.

### `ThemeDraft`

A sparse, provider-neutral intermediate value containing:

- Source metadata: provider ID, display name, and optional source location.
- Optional semantic UI roles and their resolved Ratatui-compatible styles.
- Optional exact syntax-scope styles.
- Capability information describing UI and syntax coverage.
- Non-fatal diagnostics for ignored or unsupported source features.

A draft is an adapter implementation detail. It may use maps while an adapter is being constructed; it must not become a loosely typed runtime styling API for components.

### `Theme`

A fully resolved, validated value safe to install in `Model.theme`:

- `UiTheme`, with required semantic fields such as base, selection, error, `file_path`, and `vcs_branch`.
- Exact syntax highlighting for Helix's recognized scopes, or an explicit statement that syntax is unavailable.
- Provenance and optional warnings retained for user feedback.

Required semantic fields remain ordinary struct fields. This preserves the current compiler-checked completeness guarantee rather than replacing it with unchecked map lookups.

## Narrow interfaces

The exact Rust shape can be decided during implementation. Conceptually, three small boundaries are sufficient.

### 1. Provider loading

A provider source locates and reads a document through an owned effect:

```text
ThemeSourceConfig
    → owned load request
    → provider document or neutral load error
```

Loading may eventually include reading a file or importing a machine-generated highlight export. It must not run during rendering or command update.

Associated document types are preferable to `Any`, untyped maps, or a universal JSON value shared by every adapter.

### 2. Pure format adaptation

An adapter converts its own document type into a draft:

```rust
trait ThemeAdapter {
    type Document;

    fn adapt(document: &Self::Document)
        -> Result<ThemeDraft, ThemeAdapterError>;
}

```

This boundary performs only:

- Provider-specific parsing support and schema interpretation.
- Color and modifier conversion.
- Mapping provider concepts to semantic roles.
- Capability and diagnostic reporting.

It does not perform filesystem I/O, choose application state, manage timers, or render widgets.

### 3. Theme resolution

The resolver accepts a draft and an explicitly selected fallback theme:

```text
ThemeDraft + fallback Theme + validation diagnostics
    → Result<Theme, ThemeResolutionError>
```

It owns only:

- Fallback application.
- Required-role validation.
- Capability checks.
- Final immutable construction.

Provider adapters do not decide fallback behavior, and components do not repair incomplete themes.

## Composition, not a provider framework

Configuration should describe a source, not a component interface:

```text
provider = "ghostty"
location = "~/.config/ghostty/themes/my-theme"
fallback = "default"
```

A small composition-root enum or registry may map provider IDs to adapters. Adding an entry there is expected. Adding a new enum variant to page components, `UiTheme`, or status rendering is not.

Prefer static generic adapters for fixed formats. Introduce a registry only if runtime provider selection becomes an actual requirement; it is not a third-party plugin service.

## Semantic mapping examples

Mappings belong in adapter tests and documentation, not in shared rendering code.

| Eggplant role | Ghostty | Neovim / LazyVim | VS Code |
| --- | --- | --- | --- |
| Base text/background | `foreground`, `background` | `Normal` | `editor.foreground`, `editor.background` |
| Selection | `selection-foreground`, `selection-background` | `Visual` | `editor.selectionBackground` |
| Cursor-line background | Derived from the selection background or fallback | `CursorLine` | `editor.lineHighlightBackground` |
| Muted text | A deliberately chosen palette entry | `LineNr`, `Comment` | `editorLineNumber.foreground`, `descriptionForeground` |
| File path | A deliberately subdued palette entry | `Directory` | A subdued workbench or TextMate role |
| VCS branch | A deliberately distinct palette entry | A deliberate highlight-group mapping | A workbench/Git-decoration or accent role |
| Error | A red palette entry | `ErrorMsg`, `Error` | `editorError.foreground` |
| Diff add/delete/change | Green/red/change palette entries | `DiffAdd`, `DiffDelete`, `DiffChange` | `editorGutter.addedBackground`, `deletedBackground`, `modifiedBackground` |
| Popup | Background/foreground derived by adapter policy | `Pmenu` | `editorWidget.background`, `editorWidget.foreground` |
| Status line | Derived by adapter policy | `StatusLine` | `statusBar.background`, `statusBar.foreground` |

“Deliberate mapping” means a documented provider-specific choice, not a heuristic that searches for color names in components.

### Ghostty

The current `theme::ghostty` adapter supports a static, UI-only subset of Ghostty's documented config format:

- `background` and `foreground` (required).
- Optional `selection-background` and `selection-foreground`.
- `palette = N=COLOR` for indices 0–255, using decimal, binary (`0b`), octal (`0o`), or hexadecimal (`0x`) indices. Only the first 16 ANSI colors affect Eggplant roles.
- Hex colors with 1–4 digits per channel, with or without a leading `#`, plus the eight basic named colors.
- Other Ghostty configuration options are ignored rather than evaluated.

The adapter maps these values to UI roles and uses the shared resolver for missing roles and exact syntax fallback. It does not invoke Ghostty, read configuration files during update, or infer syntax highlighting.

Future source loading should preserve this pure adapter boundary.

### Neovim and LazyVim

LazyVim is a distribution, not a colorscheme format. The useful provider data is Neovim highlight-group definitions for the active colorscheme.

Prefer a small neutral export containing group names and resolved attributes. Do not parse LazyVim Lua configuration or evaluate arbitrary theme code.

A Neovim adapter may later support:

- Static JSON/highlight exports generated outside Eggplant.
- A separate effect that requests a highlight dump from an explicitly configured Neovim process, if that workflow is ever justified.

Those are different loading sources that can reuse the same pure highlight-group adapter.

### VS Code

VS Code themes commonly contain two distinct inputs:

- Workbench colors for application UI.
- TextMate `tokenColors` for syntax highlighting.

A VS Code adapter should handle both, with explicit capability reporting when one is missing. TextMate scope mappings must be tested against Helix's exact recognized scopes rather than treating all strings as interchangeable.

No VS Code installation, extension host, or JavaScript evaluation is needed for static JSON themes.

## Fallback and capability policy

Fallback is selected once at composition, never discovered implicitly by a component.

Recommended rules:

1. A complete source may replace the fallback entirely.
2. A UI-only source may fill missing UI roles from the named fallback and must use an explicit syntax fallback.
3. Missing required values fail resolution with source-aware diagnostics.
4. Unknown provider fields may be ignored with warnings; malformed required values are errors.
5. A provider's unsupported feature must not silently alter unrelated syntax or UI roles.
6. Installation is atomic. The previous theme remains active until a complete replacement resolves successfully.

Capabilities should distinguish at least:

- Complete UI and syntax support.
- UI-only support requiring syntax fallback.
- Insufficient data for safe installation.

Do not reduce these states to a misleading `is_complete` boolean.

## Runtime and Elm integration

Provider loading belongs in background effects with owned request identity:

1. Capture source, location, fallback, and selection generation.
2. Load the document outside input/rendering.
3. Run the pure adapter and resolver.
4. Return a complete `Theme` or neutral error.
5. Accept only the latest generation.
6. Install atomically or preserve the previous theme.

A missing file, invalid document, unsupported capability, or stale completion must not leave a partially applied theme.

Components continue to receive `&Theme` or `&UiTheme`; they do not know whether the theme came from an embedded Helix file, Ghostty, Neovim, or VS Code.

## Testing boundaries

### Adapter contract tests

Each provider gets small fixtures covering:

- Minimal valid input.
- Complete valid input.
- Missing optional and required roles.
- Unknown fields.
- Invalid colors/modifiers.
- UI-only versus complete capabilities.
- Exact provider-specific scope mappings and palette values.

Exact RGB assertions belong here, not in page components.

### Resolver tests

Use synthetic drafts and a fake fallback to verify:

- Fallback application.
- Required-role validation.
- Capability rejection.
- Diagnostic preservation.
- Atomic construction.

### Application integration tests

Use a fake source/loader to verify:

- Previous-theme preservation on failure.
- Latest-generation-wins completion behavior.
- No I/O during update or rendering.
- Existing built-in themes retain their current behavior.

### Component tests

Construct `UiTheme` directly from semantic roles. Components must not load provider fixtures or know provider IDs.

## Incremental migration path

### Current checkpoint

- `UiThemePatch` represents sparse provider-neutral UI roles.
- `ThemeDraft` carries optional exact syntax and provider display metadata.
- `HelixThemeAdapter` converts each embedded Helix theme through the shared path.
- `GhosttyThemeAdapter` parses static Ghostty config into the same provider-neutral UI draft.
- `ThemeResolver` fills only missing UI/syntax values from an explicit fallback.
- `ThemeConfig` accepts existing built-in strings or a Ghostty provider table with path and fallback.
- `start_loading` performs a bounded background read and resolves the result through the same adapter/resolver path.
- `ThemeLoadState` rejects stale completions and preserves the previous theme on errors.
- `ThemePickerEntry` supports built-in and local Ghostty entries with stable selection identity.
- Opening the picker discovers user themes before system themes, suppresses duplicate names, and renders a `ghostty` section.
- Applying a discovered entry uses the same bounded loader and active-theme fallback as startup.
- `Query` and `QueryInput` provide case-insensitive filtering and a cursor-aware input without provider dependencies.
- `ThemeProviderRegistry` assembles ordered `ThemeSection` values from `ThemeProviderId` metadata and filtered entries. Rendering has no provider-specific branches.
- `ThemePickerEntry` carries provider identity and source capabilities; new file-backed providers can register and render without component changes.
- `Config::save_theme` atomically updates only the theme setting while preserving unrelated TOML values.
- `ThemePersistenceState` allows one in-flight save plus a trailing latest selection, so older writes cannot win. Ghostty sources persist only after a valid load.
- Built-in selection keeps its existing identity; external themes may have independent display names.
- Component-facing `Theme` and `UiTheme` APIs remain unchanged.

### Next steps

1. **Add richer formats as needed.** VS Code workbench/TextMate and Neovim highlight exports can be independent adapters and loading sources.
2. **Keep source policy explicit.** Additional loading sources must retain bounded reads, pure adaptation, explicit fallback, and previous-theme preservation.

Every provider slice should be independently reviewable and should not require edits to page components.

## Acceptance criteria for a new adapter

A provider is ready only when:

- Its document type and loading source are explicit.
- Its pure adapter has no application-state or rendering dependency.
- Semantic mappings and fallbacks are documented and tested.
- Capability behavior is explicit, especially for syntax support.
- Resolution is validated and atomic.
- Existing providers and built-in themes are unchanged.
- Components require no provider-specific edits.
