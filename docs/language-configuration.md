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

The abstraction, injection path, and strict TOML parser are implemented. `app::language_config::parse_language_servers` produces owned registry data without exposing TOML types to workflows. Compatibility production behavior still constructs `LanguageServers::rust_defaults()` at startup, which registers only:

```text
.rs → rust-analyzer
```

Startup file discovery/loading is **not wired yet**. There is currently one active language service, one server per registered language, and one workspace root. The parser therefore rejects multiple providers per language. Service reuse requires an exact `LanguageServerSpec` match; changing language, command, arguments, or extensions drops the old service before starting another. This sequential replacement prevents cross-language process reuse. Retaining simultaneous providers and merging their diagnostics require additional routing slices.

## Supported parser format

The app-layer parser accepts this Helix-style subset:

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

The parser accepts this format in application/library calls, but production startup does not yet read a language configuration file.

Configuration parsing produces owned registry values rather than leaking TOML types into application features. Unknown server references, duplicate languages, empty commands, missing/empty file types, unsupported fields, and multiple servers for one language fail with actionable errors. Missing executables remain runtime notifications and must not prevent ordinary editing.

## Later slices

- Load user language configuration at startup, then optionally merge trusted project overrides.
- Retain multiple idle/running services keyed by workspace root and server identity instead of sequentially replacing the active service.
- Allow multiple servers per document: first capable provider for navigation, merged diagnostics from all providers.
- Add server initialization/configuration values only when the basic loader and lifecycle are proven.
- Keep completion, hover, rename, code actions, automatic installation, and other full-IDE features out of scope.
