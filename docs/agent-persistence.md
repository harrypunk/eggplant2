# Agent persistence

Status: implemented. Provider-neutral records, asynchronous `SessionStore`, finalized-run effects, and a SeaORM SQLite adapter with migrations, transactions, keyset reads, and search are complete. Finalized transcript rehydration, `/new`, the `/sessions` workspace picker, latest-workspace startup restoration, keyset recent-session pagination, `/restore <session-id>`, and restart smoke coverage are complete; M12.7 is complete with strictly linear sessions. Provider credentials also have a backend-neutral `ProviderCredentialStore`, SeaORM SQLite migration/CRUD implementation, and masked multi-provider API-key save workflow. Provider model catalogs use the separate `ModelCatalogStore` and `agent_model_catalogs` table for cached, validator-aware refreshes. OpenAI-compatible catalog selection, activation, and restart restoration are implemented; interactive account login and non-OpenAI provider/model selection remain deferred. SQLite is always enabled and defaults to `~/.eggplant/ai/data.sqlite`; `session_db` can override that path. Cloud backends and backend-specific full-text search remain deferred.

## Recommendation

Use the repository/storage port with **replaceable relational adapters**. SQLite is the only bundled backend; add MySQL/Postgres only for a concrete deployment requirement. The application depends on `SessionStore`, so changing the adapter does not change the TUI, editor, or provider dependencies.

SeaORM is a good fit when Eggplant benefits from typed entities, migrations, and one mostly portable relational model. It is not a search engine or a scaling guarantee. Query shape, indexes, pagination, transaction boundaries, output limits, and database-specific full-text behavior remain Eggplant's responsibility.

If the actual requirements remain a few recent sessions and simple filters, `sqlx` is also a valid lower-level alternative. Choose SeaORM for maintainability across several relational backends, not because an ORM is required for correctness.

## Dependency direction

Keep the independently publishable `eggplant-agent` crate free of SeaORM, SQLx, Tokio, filesystem, and database drivers:

```text
AgentSession / provider-neutral records
              │
              ▼
        SessionStore port
              │
     SeaOrmSessionStore
              │
        SQLite now
        MySQL/Postgres later
```

The TUI/composition root selects the implementation. Provider code, popup/panel rendering, and editor mutation must not depend on SeaORM entities or SQL.

Within the SeaORM adapter, `store/mod.rs` is only the `SessionStore` implementation and connection owner. `store/write.rs` owns finalized-run validation and transactions, `store/read.rs` owns recent/transcript queries and related-row assembly, `store/search.rs` owns message/tool predicates and keyset search, and `store/records.rs` owns row/domain mappings, persistence encodings, pagination values, and SQLite bounds. Entities and migrations remain separate from these workflows. This keeps SQL operations explicit without giving one module all storage responsibilities.

The port lives in `agent-storage`. Relational drivers belong in separate adapters; neither `eggplant-agent` nor editor code depends on them. The TUI depends only on `SessionStore`, never SeaORM entities, SQL, connection URLs, or backend query syntax.

The default application creates and migrates `~/.eggplant/ai/data.sqlite`; an absolute `session_db` overrides that location. There is no SQLite feature flag or in-memory production fallback. MySQL/PostgreSQL drivers and cloud URLs are not included.

## Storage port

The port should expose operations needed by real consumers, not generic CRUD:

- Create a session and append a run with its explicit editor/provider context.
- Append finalized user/assistant/tool messages in sequence order.
- Update run status/completion/error without rewriting history.
- Load recent sessions with deterministic keyset pagination.
- Load one session's ordered transcript, batching related rows.
- Search messages/tool calls with a bounded filter/page.
- Validate storage identity before applying completion events.

Use `SessionId` as the database identity. `RunId` remains scoped to a session and must not become a globally unique database key by itself.

Storage writes belong in background effects with request/run identity. Do not hold a service or document lock during database I/O, and do not make popup rendering wait on a database query. `Effect::combine` is not a transaction barrier.

The TUI captures a `FinalizedRun` only after complete, failed, or cancelled session state is applied. Live deltas and pending approvals are not persisted as finalized messages. The write effect owns all records, and its completion is accepted only for the matching `SessionId` and `RunId`. Backend failure leaves the provider result intact and produces a bounded storage warning in both Agent views; stale completions cannot clear a newer run's warning.

Headless restoration uses `load_session` over the `SessionStore` interface to collect every transcript page, deduplicate related run/tool rows, and produce a provider-neutral `SessionSnapshot`. `AgentRuntime::restore_session` rejects replacement during active work and applies only finalized snapshots. Restoration preserves `SessionId`, original creation time, stored session title, finalized run status, ordered messages/tools, and the highest restored `RunId`; the next submission continues after that ID rather than overwriting run 1. Session titles are initialized from the first prompt and retained as runtime metadata rather than recomputed from the bounded message buffer, so later eviction cannot rename a session. Restored assistant text becomes answer-only display parts, with no synthesized thinking, token usage, pending approval, or active continuation. Message/tool memory bounds still apply after computing the next run identity. This is a reusable runtime/storage capability used by the interactive `/sessions` picker: recent rows are workspace-filtered, completion generations reject stale loads, and successful restoration closes the picker after dispatching the shared session-changed UI action. Tab reuses `RecentQuery.before` with the stored keyset cursor to append older pages; selection remains anchored to `SessionId`, rendering stays bounded, and initial/next-page completions are distinct typed outcomes. `/restore <session-id>` intentionally bypasses the workspace-filtered picker for explicit cross-workspace restoration. Direct requests advance a runtime session revision, so late completions cannot replace newer runs, `/new`, or restore requests. Startup also restores the most recently updated finalized session for the current workspace before provider selection; `/new` starts fresh, no provider request occurs during transcript loading, and resume-read failures leave an empty session with a bounded storage warning.

## Proposed relational schema

Exact types/migrations are an implementation decision, but normalize fields used for filtering and pagination:

### `agent_sessions`

- `id` (UUID or another stable string ID)
- `created_at`, `updated_at`
- Optional workspace identity/root metadata
- Optional user-facing title generated separately from raw prompt text

Index `(updated_at, id)` for recent-session keyset pagination.

### `agent_runs`

- `id`
- `session_id`
- `run_number`
- `status`
- `created_at`, `completed_at`, optional `error`
- Provider/model metadata needed for reconstruction
- Editor context as bounded JSON only when appropriate

Unique `(session_id, run_number)` and index `session_id`.

### `agent_messages`

- `id`
- `session_id`
- `run_id`
- `sequence`
- `role`
- `text`
- `created_at`

Unique `(session_id, sequence)` and index `(run_id, sequence)` for ordered, batched transcript loading.

### `agent_tool_calls`

- `id`
- `run_id`
- `call_id`
- `name`
- `arguments_json`
- `status`
- `result_json`
- `created_at`, `completed_at`

Index `(run_id, name)` for common tool filtering. Store bounded results by default; very large artifacts need a separate object/reference model later, not unbounded database blobs.

Append message/run/session updates in one transaction where the backend requires consistent transcript state. Load related rows in batches rather than one query per message.

### `agent_provider_credentials`

- `provider_id` (primary key and stable provider catalog ID)
- `auth_type` (`oauth` or `api_key`, enforced by a database check and SeaORM active enum)
- Optional `base_url` for OpenAI-compatible endpoints or provider-specific configuration
- Optional `selected_model`, set only after the user chooses from the provider's available catalog
- Optional `api_key`
- Optional OAuth `access_token`, `refresh_token`, `expires_at`, and `account_id`
- `created_at`, `updated_at` as Unix milliseconds

One provider has at most one stored credential. Upserting another authentication method replaces the old method and clears its inapplicable fields. `ProviderCredentialStore` is separate from `SessionStore`: credentials are per-user authentication state, not workspace transcript records. Metadata listing selects only provider ID and method, while reading a credential returns the secrets needed by provider authentication. API-key records require a nonempty key and no OAuth fields; OAuth records require a nonempty access token and no API key.

This prototype stores local database secrets as plaintext, similar to a private credential file; encryption or OS keychain integration is not implemented. Keep the database in the per-user data directory with appropriate filesystem permissions, and never include credential values in debug output, notifications, or logs. API-key saves use the same per-user `session_db` connection policy and run asynchronously. OpenAI-compatible login then fetches `/models`; activation persists `selected_model` before installing the provider, and startup restores it unless explicit configuration takes precedence. Other providers remain storage-only. Account/OAuth login, token refresh orchestration, and non-OpenAI provider/model selection are separate later slices.

### `agent_model_catalogs`

- `provider_id` (primary key)
- `models_json` (normalized, bounded array of model IDs)
- `checked_at` (Unix milliseconds of the last completed remote check)
- Optional `last_modified` and `etag` for future conditional refreshes

Catalogs are independent of credential records and contain no authentication material. Successful OpenAI-compatible `/models` responses are upserted through `ModelCatalogStore`, then used by the model selector. A cache-write failure produces a warning without blocking selection. `selected_model` remains a separate user choice: refreshing a catalog does not change the active model.

## Search and scale

SeaORM does not provide portable, high-quality full-text search automatically. Treat these as separate concerns:

1. **Portable filters:** role, provider, model, run status, tool name, timestamp, workspace, and call ID. These belong in normalized columns and indexes.
2. **Pagination:** use `(updated_at, id)` or `(sequence, id)` keysets rather than large offset scans.
3. **Text search:** define repository operations such as `search_messages`, with backend-specific implementations behind the same port.
4. **Output bounds:** limit result count and returned text even when the database can return more.

Backend options may include SQLite FTS5, PostgreSQL `tsvector`, and MySQL `FULLTEXT`. Their syntax, ranking, and configuration differ. Keep those expressions in the adapter and return provider-neutral results. Do not pretend one SeaORM query is fully portable across all three.

## SQLite first

A local SQLite database is appropriate for:

- Finalized sessions, runs, messages, and tool calls.
- Recent-session navigation and bounded keyword/filter queries.
- Testing migrations, rollback paths, and corruption/error reporting.

Use a per-user data directory, not the workspace or repository root. Session content may contain secrets or proprietary code, so apply the same redaction and access rules as the rest of the agent.

Tests should use temporary SQLite files or isolated in-memory databases. No test should require MySQL, PostgreSQL, cloud credentials, or user data.

## Cloud MySQL/PostgreSQL later

Cloud support is a deployment extension, not a reason to redesign the agent session:

- Supply `EGGPLANT_AGENT_DATABASE_URL` or an equivalent secret reference outside source-controlled configuration.
- Use a small connection pool and background effects with timeouts/cancellation.
- Validate TLS/driver features when pinning the SeaORM version; do not silently send cloud credentials without the intended transport policy.
- Run migrations explicitly with operational locking/backup policy, not implicitly during terminal rendering.
- Use provider-specific indexes and query plans before claiming scale.

The public session/tool contracts must not expose a SQL connection, credentials, entity types, or backend-specific query strings to the TUI.

## SeaORM packaging

The adapter pins SeaORM 2.0.4 with default features disabled and only `macros`, `sqlx-sqlite`, and `runtime-tokio-rustls`. Versioned migrations use SeaORM's own schema builders, `ConnectionTrait`, transactions, and a small migration-history entity; no migration-manager or vendored upstream crate is required. This avoids unwanted defaults while preserving SeaORM's backend abstraction. SQLite-only connection setup is isolated in the adapter, and text search uses portable `LOWER`/`LIKE` expressions with escaped literal wildcards and case-insensitive matching in both stores.

Optional `sqlx-mysql` / `sqlx-postgres` features belong only to future deployment adapters. Dependency, compile time, binary size, and migration complexity remain outside `eggplant-agent` and the default TUI build.

## Implementation sequence

1. **Implemented:** provider-neutral finalized session/run/message records and storage ports.
2. **Implemented:** injected-provider finalization flows through the existing update/effect path, with run-scoped storage warnings and no rendering barrier.
3. **Implemented:** separate SeaORM SQLite adapter with migrations, transactions, and keyset reads.
4. **Implemented:** filter/search and truncation tests plus corrupt, locked, overflow, migration-failure, and cancellation coverage.
5. **Implemented:** multi-turn provider history, declarative transcript rehydration, stable session metadata, `/new`, and the generation-safe `/sessions` picker.
6. **Complete (M12.7):** latest-workspace startup continuation, keyset recent-session pagination, direct restore-by-ID, and restart/resume smoke coverage are implemented. Initial sessions remain strictly linear; branching/forking/session trees are not initial-version goals.
7. **In progress (M12.8):** provider-neutral `ConversationContextPolicy`, full/deterministic bounded projections, omission metadata, and runtime injection are implemented. Capacity-aware request budgeting and any compaction evaluation remain.
8. Deferred: add backend-specific full-text search only with measured requirements.
9. Deferred: add cloud configuration, migrations policy, TLS/pool limits, and manual deployment smoke tests behind the same `SessionStore` boundary.

Persistence must not delay M12.2's real provider. Production startup always injects SQLite; tests may intentionally leave the store unconfigured to exercise other workflows. Hiding the popup/panel preserves the current in-progress session.
