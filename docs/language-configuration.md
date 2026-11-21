# External language-server configuration

Eggplant should bundle LSP protocol support, not language-server executables or a growing catalog of language-specific adapters. External tools are installed and managed by the user or project.

## Design

Follow the useful part of Helix's model:

1. Detect a language from configured file types and project roots.
2. Define named external server commands independently.
3. Attach one or more server names to each language.
4. Resolve executables through `PATH`; never install them automatically.

Language-server configuration is data owned by the application layer. Terminal UI features do not contain Rust, TypeScript, Python, ESLint, or Ruff special cases.

## Abstract interfaces

`app::language` owns the provider-neutral boundary:

- `LanguageServerSpec` is owned, configuration-ready data: language, command, arguments, and file extensions.
- `LanguageServerRegistry` is the lookup interface used by document/navigation workflows.
- `LanguageServers` is the current in-memory registry implementation.
- `LanguageService` abstracts document synchronization and source navigation independently of LSP.
- Owned `LanguageEvent` values deliver diagnostics, messages, progress, and terminal failures.

The application composition root injects `Arc<dyn LanguageServerRegistry>` into the model. `initial_model_with_languages` accepts any implementation, and `features/code_navigation` no longer calls a built-in registry constructor. This permits user configuration, project configuration, and test fakes without changing workflow code.

## Current implementation status

The abstraction and injection path are implemented. Compatibility behavior still constructs `LanguageServers::rust_defaults()` at production startup, which registers only:

```text
.rs → rust-analyzer
```

User `languages.toml` loading is **not implemented yet**. There is currently one active language service, one server per registered language, and one workspace root. Keyed services and multiple providers per document are required before mixed-language or TypeScript-plus-ESLint setups are safe.

## Planned file format

The next slice can load a small Helix-style subset such as:

```toml
[language-server.typescript-language-server]
command = "typescript-language-server"
args = ["--stdio"]

[language-server.ruff]
command = "ruff"
args = ["server"]

[[language]]
name = "typescript"
file-types = ["ts", "tsx"]
language-servers = ["typescript-language-server"]

[[language]]
name = "python"
file-types = ["py", "pyi"]
language-servers = ["ruff"]
```

This example illustrates the intended format; it is not currently accepted by Eggplant.

Configuration parsing should produce the same owned registry interface rather than leaking TOML types into application features. Invalid references, duplicate definitions, and empty commands should fail with actionable configuration errors. Missing executables remain runtime notifications and must not prevent ordinary editing.

## Later slices

- Load user language configuration at startup, then optionally merge trusted project overrides.
- Key running services by workspace root and server identity.
- Allow multiple servers per document: first capable provider for navigation, merged diagnostics from all providers.
- Add server initialization/configuration values only when the basic loader and lifecycle are proven.
- Keep completion, hover, rename, code actions, automatic installation, and other full-IDE features out of scope.
