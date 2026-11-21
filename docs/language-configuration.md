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

The abstraction, injection path, strict TOML parser, and user-file loading are implemented. `app::language_config::parse_language_servers` produces owned registry data without exposing TOML types to workflows. Production startup loads `~/.eggplant/languages.toml` before terminal initialization. If the file is absent, community defaults provide these external commands:

| Languages | Extensions | Command |
| --- | --- | --- |
| Rust | `rs` | `rust-analyzer` |
| TypeScript / TypeScript React | `ts`, `mts`, `cts`, `tsx` | `typescript-language-server --stdio` |
| JavaScript / JavaScript React | `js`, `mjs`, `cjs`, `jsx` | `typescript-language-server --stdio` |
| Python | `py`, `pyi` | `pyright-langserver --stdio` |

Only command recipes are included; the executables must be installed separately.

A present language file replaces the community defaults; an empty file disables language services explicitly. There is currently one active language service, one server per registered language, and one workspace root. The parser therefore rejects multiple providers per language. Service reuse requires an exact `LanguageServerSpec` match; changing language, command, arguments, or extensions drops the old service before starting another. This sequential replacement prevents cross-language process reuse. Retaining simultaneous providers and merging their diagnostics require additional routing slices.

## User configuration

Save the supported Helix-style subset as `~/.eggplant/languages.toml`:

```toml
[language-server.typescript-language-server]
command = "typescript-language-server"
args = ["--stdio"]

[language-server.basedpyright]
command = "basedpyright-langserver"
args = ["--stdio"]

[language-server.ruff]
command = "ruff"
args = ["server"]

[[language]]
name = "typescript"
file-types = ["ts", "mts", "cts"]
language-servers = ["typescript-language-server"]

[[language]]
name = "typescriptreact"
file-types = ["tsx"]
language-servers = ["typescript-language-server"]

[[language]]
name = "python"
file-types = ["py", "pyi"]
language-servers = ["basedpyright"]
```

The language `name` is also the LSP `languageId`, so React file types use `typescriptreact` or `javascriptreact`. For Ruff's lint-focused Python diagnostics instead of Basedpyright, set Python's `language-servers = ["ruff"]`. Running both providers together is not supported yet.

See the complete [`languages.example.toml`](../languages.example.toml), which also retains Rust and configures JS/JSX. A present configuration replaces the community defaults, so include every language you want to keep.

Install the configured executables separately and ensure they are available on `PATH`. Configuration is loaded before terminal initialization; read and validation errors report the language-file path. Unsupported files and unavailable executables do not prevent ordinary editing.

Configuration parsing produces owned registry values rather than leaking TOML types into application features. Unknown server references, duplicate languages, empty commands, missing/empty file types, unsupported fields, and multiple servers for one language fail with actionable errors. Missing executables remain runtime notifications and must not prevent ordinary editing.

## Later slices

Detailed acceptance criteria live in [PLAN Milestone 11](../PLAN.md#milestone-11-multi-provider-language-workflows).

1. Introduce stable configured server IDs, retain compatible services keyed by workspace/server, and make diagnostics generations and per-path contributions provider-specific (M11.1).
2. Accept ordered multi-provider language lists, synchronize all attached providers, use first-capable navigation, and merge diagnostics (M11.2).
3. Add server initialization/configuration values only after basic multi-provider lifecycle and stale-result tests are proven (M11.3).
4. Consider trusted project overrides and safe process reuse across language IDs; never execute project configuration without an explicit trust policy (M11.3).
5. Keep completion, hover, rename, code actions, automatic installation, and other full-IDE features out of scope.
