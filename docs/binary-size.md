# Binary size plan

## Status

Deferred. No release-profile changes are active.

Observed on 2026-11-26: the Linux release binary is approximately **33 MiB**. Size reduction is exploratory and has no numeric target or benchmark limit. It must not weaken reliability, performance, diagnostics, or feature ownership.

## Principles

1. Measure the deployed release artifact, not a debug or incidental incremental build.
2. Preserve symbols or debug information for development and production diagnosis.
3. Prefer smaller distribution artifacts over stripping the only development copy.
4. Prevent duplicate dependency stacks before attempting aggressive compiler settings.
5. Evaluate runtime performance, clean-build time, startup behavior, and binary size together.
6. Do not split the agent into another process merely to reduce file size.

## Baseline inventory

Before changing settings, record:

- OS, CPU architecture, toolchain, enabled features, and exact source revision.
- Clean release artifact size and section totals from `size`.
- Largest crates/functions from `cargo bloat` or an equivalent symbol report.
- Stripped and unstripped sizes.
- Clean-build duration, startup smoke behavior, and the normal workspace test/Clippy baseline.
- Whether native libraries such as SQLite are vendored or supplied by the system.

Keep the measurements comparable by using the same clean-build procedure for every experiment.

## Ordered experiments

### 1. Separate deployment from diagnostics

Produce a smaller distribution copy while retaining an unstripped binary or separate debug information for diagnosis. Do not remove symbols from the only artifact used for debugging; release symbols recently made production input-routing diagnosis possible.

**Exit:** the smaller artifact passes smoke tests, and its addresses can still be diagnosed using the retained symbols/debug data.

### 2. Thin LTO

Evaluate thin link-time optimization against the baseline.

**Record:** artifact size, clean-build duration, startup/runtime smoke results, and test status.

### 3. Fat LTO and one codegen unit

Evaluate fat LTO and one codegen unit only if thin LTO leaves a worthwhile opportunity. Accept the slower build only for a demonstrated benefit.

### 4. Size-oriented optimization

Measure `opt-level = "z"` or `"s"` separately from LTO changes. Reject settings that materially harm interactive responsiveness or provider/LSP work merely to reduce file size.

### 5. Panic behavior

`panic = "abort"` is the last resort, not an initial optimization. It changes failure behavior and requires a dedicated review of terminal restoration and panic diagnostics.

## Dependency-growth rules

As agent functionality expands:

- Reuse the existing Tokio runtime, HTTP/TLS stack, serialization, text handling, and storage adapters.
- Prefer OpenAI-compatible/provider-neutral protocols over separate native provider SDKs.
- Keep fetched model catalogs in SQLite; do not embed large provider/model datasets.
- Do not embed language servers, provider executables, or a speculative plugin runtime.
- Review monomorphization only from size evidence; do not replace useful generics with dynamic dispatch by default.
- Do not rewrite SeaORM/SQLx solely for file size.
- Consider system SQLite only as an explicit portability-versus-size tradeoff after checking the current linkage.
- Avoid executable packers such as UPX by default. Distribution archives can provide compression without changing runtime behavior.

## Tracking and adoption

Add a repeatable size report before considering CI enforcement. Prefer reporting unexpected relative growth over a fragile numeric failure threshold.

No setting is adopted without:

1. A comparable before/after measurement.
2. Full workspace tests and Clippy.
3. Real-terminal smoke coverage.
4. A documented build-time, runtime, deployment, and diagnostics tradeoff.

The goal is a smaller, understood distribution artifact—not a particular number at the expense of the editor's reliability.
