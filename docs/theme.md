# Theme architecture

Status: living design. The first slice implements config-selected `default`, `tokyonight`, and `catppuccin_mocha` themes, exact layered syntax highlights, and semantic UI styles. Runtime switching and user-provided theme files remain future increments.

## Purpose

Themes should change the application's appearance without changing editing, filesystem, input-routing, or component-state logic. The Elm architecture already provides the required boundary:

```text
configuration or command
          │
          ▼
 resolve a complete Theme
          │
          ▼
       Model.theme
          │
          ▼
 page view(&Model) passes resolved style props
          │
          ▼
 components choose semantic roles for their current state
          │
          ▼
       Ratatui Style
```

A theme is data plus mappings from semantic roles to Ratatui styles. It is not a second state-management system and must not perform work during rendering.

## Responsibilities

| Module | Owns | Must not own |
| --- | --- | --- |
| `config` | Deserialize the configured theme name and other raw configuration. | Ratatui colors, component styling, or theme application. |
| `theme` | Built-in theme definitions, semantic UI/syntax roles, validation, and conversion to resolved `Style` values. | Focus, selection, editing state, layout, or filesystem access during rendering. |
| `Model` | The active, fully resolved `Theme`. | Raw theme TOML or per-component copies of the palette. |
| `MessageManager` | Apply a valid theme-selection message by replacing `Model.theme`; report selection errors without disturbing unrelated state. | Color definitions or component-specific appearance decisions. |
| Pages | Compose layout and pass the active theme or its UI/syntax slices to components. | Theme-name lookup or file loading. |
| Components | Map local presentation state such as selected, focused, or error to semantic style roles. | Raw palettes, theme names, configuration loading, or global `Model` access. |
| Runtime/effect executor | Load and validate an external theme only if file-backed themes are added. | Rendering decisions. |

The editor crate continues to own syntax-highlight data. It must remain independent of Ratatui and Eggplant UI theme definitions.

## Reusing existing open-source themes

Do not manually recreate popular palettes component by component. Use an existing editor theme format as the source of truth and isolate compatibility in the theme module.

**Helix-compatible TOML is the recommended first source format.** Eggplant already uses Helix grammars and scopes, and Helix themes define both syntax and UI styles. The pinned Helix source already contains Tokyo Night variants and Catppuccin Latte, Frappé, Macchiato, and Mocha. For example, these files provide:

- Syntax scopes such as `keyword`, `function`, `string`, and their more specific children.
- UI roles such as `ui.background`, `ui.text`, `ui.linenr`, `ui.selection`, `ui.menu.selected`, `ui.popup`, `ui.statusline`, and `ui.window`.
- Named palettes, parent-theme inheritance, modifiers, and RGB/indexed/terminal colors.

This is a better match than terminal emulator themes, which usually supply only a general palette, or theme-specific Rust crates, which support only one theme family.

### Compatibility boundary

A theme adapter may reuse `helix_view::theme::Loader` for filesystem-backed themes and `helix_view::Theme` for palette, inheritance, and scope lookup. Convert Helix `Style`/`Color`/`Modifier` values to Ratatui values inside the TUI theme adapter. This uses Helix data libraries only; it does not adopt Helix's terminal, keymap, commands, or application loop.

The adapter maps existing UI scopes to Eggplant's semantic roles, for example:

| Eggplant role | Preferred Helix scope | Fallback policy |
| --- | --- | --- |
| Base text/background | `ui.background` and `ui.text` | Explicit default-theme base. |
| Gutter/muted text | `ui.linenr` | `ui.text.inactive`, then base. |
| List selection | `ui.menu.selected` | `ui.selection`, then a default selection patch. |
| Popup | `ui.popup` | `ui.menu`, then base. |
| Status/mode line | `ui.statusline.normal`, `ui.statusline.insert` | `ui.statusline`, then base. |
| Panel border | `ui.window` | Base foreground. |

The adapter owns this mapping and its fallbacks. Components still see only resolved semantic roles. Unsupported Helix features can initially produce warnings or documented fallbacks; they must not cause per-component color branches.

### Preserve exact syntax scopes for faithful themes

The current `TextViewSnapshot` reduces Helix scopes to broad `HighlightKind` values such as `Keyword` and `String`. A quick adapter could query only those broad scopes, but that loses distinctions such as `keyword.control`, `keyword.function`, `variable.builtin`, and language-specific variants. The result would reuse the palette without faithfully reproducing the theme.

For faithful upstream rendering, retain the exact Helix highlight/scope identifiers in each visible `HighlightSpan` instead of collapsing them to `HighlightKind`. Preserve all active layers, not only the last scope, so broad and specific styles can be patched in order. The editor crate still owns syntax traversal; the TUI theme adapter owns appearance.

Helix highlight indices are relative to a recognized-scope table, not universally interchangeable between theme objects. Eggplant's `DocumentFactory` configures syntax queries with Helix's default recognized scopes. At theme resolution, the adapter iterates those scope names, performs selected-theme parent-scope fallback, converts the results to Ratatui, and stores them in the same order. Rendering can then compose exact highlight IDs without mutating the global loader or risking theme-local index mismatches. Unknown/out-of-range IDs produce no additional style.

Prefer this exact-scope path when upstream theme fidelity is the goal. Broad-kind mapping is acceptable for a temporary spike, but should not become a second long-term theme contract.

### Shipping and discovering resources

Adding Helix as a dependency does **not** make its runtime theme directory automatically available to Eggplant. Except for Helix's embedded default themes, its loader searches explicit filesystem directories. Eggplant therefore needs an explicit asset policy:

1. Ship a curated set of unmodified upstream TOML files as Eggplant resources, so normal use does not require a Helix installation or network access.
2. Optionally search user theme directories for additional Helix-compatible themes.
3. Do not reference Cargo's git checkout or assume a system Helix runtime path.

Packaging can embed curated TOML resources or install them as application data. Embedded resources can be parsed at startup without filesystem effects; user-provided files use the load-effect path described below. Supporting inheritance for embedded themes requires resolving all referenced parent resources as one bundle.

When vendoring or generated packages include upstream themes, retain attribution and review the applicable upstream and per-theme license notices. Do not assume every contributed theme has identical licensing. Adding a supported theme should ideally mean syncing a resource and its notice, not rewriting its colors in Rust.

Cobalt is not included in the pinned Helix theme directory. It can come from an external Helix-compatible theme if one is available, or from a one-time conversion into that shared format. Other format importers, such as VS Code JSON or Base16, should be separate adapters only when needed; Base16 generation is useful for broad coverage but will not exactly reproduce every editor-specific theme.

## Resolved theme in the model

Keep the existing ownership direction:

```rust
struct Model {
    theme: Theme,
    // Existing editor, explorer, picker, and UI state...
}
```

`Model.theme` is the single active appearance snapshot. It is resolved at startup or replaced by an update, then borrowed immutably by the view. Components do not read `Config`, call `Theme::from_name`, or choose a theme themselves.

A small initial shape is sufficient:

```rust
struct Theme {
    ui: UiTheme,
    // Ratatui styles resolved in recognized Helix scope order.
    syntax: Vec<Style>,
}

struct UiTheme {
    base: Style,
    muted: Style,
    selection: Style,
    border: Style,
    focused_border: Style,
    error: Style,
    key_hint: Style,
}

impl Theme {
    fn highlight(&self, highlights: &[helix_core::syntax::Highlight]) -> Style;
}
```

These names illustrate semantic roles, not a required final field list. Add roles when components need them rather than building a large design-token framework up front. `Style` values may be patches: not every role needs to specify every foreground, background, and modifier attribute.

Theme names and raw configuration strings stay at the configuration/command boundary. A valid runtime selection can use a small validated name type; components never consume that identifier.

## Semantic styles, not component color logic

A component should express **what a state means**, while the theme defines **how that meaning looks**:

```rust
let style = match entry_is_selected {
    true => styles.selection,
    false => styles.base,
};
```

The component must not contain this instead:

```rust
Style::new().fg(Color::Magenta)
```

Likewise, a theme should expose `selection` or `focused_border`, not `magenta` or `blue`, as component-facing API. Palette names may be useful privately inside a theme definition, but they are not application semantics.

Prefer shared roles for shared meaning. Explorer rows, picker results, and buffer tabs can use the same selection role unless a real visual requirement justifies a narrower role. Do not immediately create methods such as `explorer_selected`, `picker_selected`, and `tab_selected` that all duplicate one concept.

## Passing styles through pages

Pages remain responsible for composition. Pass the narrowest existing style slice that a component needs:

```rust
FileExplorer::new(workspace, explorer, focus, &model.theme.ui)
StatusBar::new(mode, status, &model.theme.ui)
TextView::new(snapshot, &model.theme) // Text/gutter UI plus exact syntax styles.
```

A composite component such as the file picker may receive `&Theme` when it renders both themed UI and a syntax-highlighted preview. It should pass only the relevant slice to its children. Introduce a component-specific style-props struct only when shared UI/syntax slices no longer express the component's needs cleanly.

No component should mutate the theme or retain configuration state. A theme change therefore requires no component invalidation protocol: the next ordinary `view(&Model)` call receives the new resolved styles.

## Style composition and precedence

Define composition once so a new theme does not expose accidental differences between components.

For normal document text, the recommended order is:

```text
base text style → syntax highlight patch → selection patch, when selected
```

Ratatui's `Style::patch` can combine these layers. Applying selection last lets a theme deliberately preserve syntax foregrounds while setting a selection background, or override both when that is its documented design.

Apply the same principle elsewhere:

- Base foreground/background come from `UiTheme::base`.
- Muted text, errors, and key hints add or replace semantic attributes.
- Focus determines whether `border` or `focused_border` is used; it does not change layout.
- Border shape, padding, widths, and popup placement remain layout/component concerns unless a deliberate future design makes them themeable.
- Avoid `Style::reset()` in leaf components unless clearing all inherited terminal styling is explicitly required.

Theme definitions must be complete by construction. Required semantic fields should be ordinary struct fields so adding a new role produces compiler errors in every theme, rather than missing-color fallbacks scattered through components.

## Elm message flow

### Startup

Startup resolves the configured built-in theme before entering the application loop:

```text
Config.theme: String
    → validate and resolve Theme
    → install in Model
    → run the normal event/update/view loop
```

An unknown or invalid configured theme should produce one clear startup error listing supported names. Do not silently mix part of the requested theme with the default theme.

Theme resolution is initialization work, not something repeated by `main.rs` or pages on each frame. As theme loading grows, it can move behind a startup application builder; the theme module still owns validation and construction.

### Switching between built-in themes

Built-in themes are static data, so switching is a pure update and needs no effect:

```text
Theme command
    → Message::SetTheme(validated_name)
    → MessageManager replaces Model.theme
    → normal redraw uses the new theme
```

The update changes no buffer text, cursor, explorer selection, picker contents, or save state. If a requested built-in name is invalid, preserve the active theme and set an error status.

A runtime theme selector is optional. Supporting several built-in themes through `config.toml` is a valid first increment and does not require adding a picker or command-menu workflow.

### File-backed themes, if added later

Reading custom theme files must cross the existing effect boundary rather than occurring in `update` or `view`:

```text
Message::SetTheme(name/path)
    → Effect::LoadTheme(request)
    → worker reads, deserializes, and validates a complete theme
    → Message::ThemeLoaded { request_id, result }
    → update replaces Model.theme only for the latest successful request
```

Keep the previous theme active while loading or after failure. Use a request ID or generation so a slow earlier completion cannot replace a newer selection. Completion errors become ordinary status messages. Never render a partially loaded theme.

This asynchronous path is unnecessary for compiled-in themes and should not be introduced before external themes are actually supported.

## Built-in theme organization

`theme.rs` can remain one file while the theme contract and default palette are small. When additional themes make that unclear, split by ownership rather than by component:

```text
theme/
├── mod.rs          Theme, UiTheme, resolved syntax table, name resolution
└── builtin/
    ├── default.rs
    └── another.rs
```

Each built-in module supplies the same semantic contract. Name resolution selects a complete theme; it does not branch inside components.

For the first implementation, prefer a small set of curated Helix-compatible TOML resources resolved at startup. Imported themes do not need per-theme Rust modules. User-selected external files, automatic terminal-capability detection, hot reload, and additional source-format importers are separate features. If terminal capability adaptation is added, resolve it at initialization/loading time rather than querying the terminal during rendering.

## Migration from the current styling

Keep each step reviewable and preserve the default appearance until a step intentionally changes it.

1. **Define the semantic contract.** Expand `Theme` into UI and syntax style slices while retaining the existing `default` highlight colors.
2. **Route existing hardcoded styles through it.** Migrate status errors, selected rows/tabs, key hints, gutters, borders, and base text/background without changing state or layout logic.
3. **Pass resolved styles from pages.** Remove component access to raw colors and avoid passing `Config` into the view tree.
4. **Import an upstream theme.** Add one curated Helix-compatible resource and its license notice, resolve it through the adapter, and extend validated name resolution and `config.example.toml`.
5. **Optionally add runtime selection.** Add `SetTheme` and a command-menu action only if interactive switching is desired.
6. **Consider external themes separately.** Add load effects, validation, stale-completion handling, and documentation only when that feature is in scope.

Do not combine the second theme with a large component redesign. The purpose of migration is to make appearance interchangeable while behavior remains unchanged.

## Testing

Theme tests should verify contracts and behavior rather than snapshot every color unnecessarily.

- Every supported built-in name resolves; unknown names produce useful errors.
- Every theme satisfies the same required semantic fields by construction.
- A component maps selected/focused/error states to the same semantic roles under different themes.
- Rendering with `TestBackend` checks representative cell styles as well as unchanged text/layout.
- `SetTheme` changes only `Model.theme` and relevant status feedback; buffers, editing history, focus, and explorer state remain intact.
- Editor and picker previews use the same `Theme` mapping for exact Helix highlights, including parent-scope fallback and layered composition.
- If external loading is added, failed and stale completions preserve the active theme, and view code performs no file I/O.

## Design rules

1. **One active theme:** `Model.theme` is the only source of appearance during a frame.
2. **Semantic boundary:** components request meaning; themes decide colors and modifiers.
3. **Pure view:** no theme lookup, parsing, filesystem access, or terminal queries during rendering.
4. **Pure built-in updates:** switching static themes needs only a message and update.
5. **Effects for external work:** file-backed themes use the same runtime/completion pattern as other I/O.
6. **Atomic changes:** replace a complete theme only after successful validation; never partially restyle the model.
7. **No editor-core coupling:** Helix/`editor` provides highlight semantics, while TUI themes own Ratatui styles.
8. **Small increments:** start with semantic styles and two built-ins; add registries, custom files, or reload only when required.
