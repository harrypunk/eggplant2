# Agent persistence

Status: provider-neutral finalized records, the asynchronous `SessionStore` port, and in-memory keyset/search behavior are implemented. The TUI still uses in-memory sessions by default; the SeaORM SQLite adapter is next.

## Recommendation

Use a repository/storage port with an **in-memory implementation now** and a **SeaORM implementation for relational persistence later**. Start with SQLite, then add MySQL/Postgres only for a concrete deployment requirement.

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
       ┌──────┴────────┐
       │               │
 InMemoryStore   SeaOrmSessionStore
                       │
                 SQLite now
                 MySQL/Postgres later
```

The TUI/composition root selects the implementation. Provider code, popup/panel rendering, and editor mutation must not depend on SeaORM entities or SQL.

The port and in-memory implementation live in `agent-storage`. Relational drivers belong in separate adapters; neither `eggplant-agent` nor editor code depends on them.

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

## Suggested SeaORM packaging

At implementation time, pin and review exact versions/features rather than enabling every backend by default. Expected dependency shape:

- `sea-orm` with `sqlx-sqlite` and `runtime-tokio-rustls` for the first adapter.
- `sea-orm-migration` for versioned schema migrations.
- Optional `sqlx-mysql` / `sqlx-postgres` features only when those deployments are implemented.

Dependency, compile time, binary size, and migration complexity are real costs. They belong in the adapter, not in `eggplant-agent` or ordinary editing.

## Implementation sequence

1. **Implemented:** provider-neutral finalized session/run/message records and an in-memory `SessionStore`.
2. Prove fake-provider persistence through the existing update/effect path.
3. Add the SeaORM SQLite adapter with migrations and keyset reads.
4. Add filter/search and truncation tests, failure handling, and corrupt/locked database behavior.
5. Add backend-specific full-text search only with measured requirements.
6. Add cloud configuration, migrations policy, TLS/pool limits, and manual deployment smoke tests.

Persistence must not delay M12.2's real provider. If no store is configured, the current in-memory session remains the default and hiding the popup/panel still preserves it.
