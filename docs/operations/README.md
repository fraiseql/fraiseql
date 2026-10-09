# Operations Documentation

Guides for running FraiseQL in production.

| Document | Purpose |
|----------|---------|
| [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md) | How `schema.compiled.json` moves from CI to production, sensitivity classification, deployment options |
| [zero-downtime-deploys.md](zero-downtime-deploys.md) | Rolling / blue-green / canary deploys behind a load balancer; expand-contract migrations; graceful drain; why one schema per process |
| [observers.md](observers.md) | Operating observers under `fraiseql-server`: feature flags, honoured environment overrides, the `/api/observers` admin API (DLQ, runtime, checkpoints), scaling with NATS vs PostgreSQL, metrics |
| [observer-idempotency.md](observer-idempotency.md) | At-least-once delivery: when observer actions repeat, the event id (`X-FraiseQL-Event-Id`) as the dedup key, the dispatch ledger, building an idempotent consumer |
| [read-replicas.md](read-replicas.md) | Serving compiled queries from PostgreSQL replicas: static read/write routing, the read-your-writes pin window, failure behaviour, tenant isolation on every pool |
| [graphql-sse-streaming.md](graphql-sse-streaming.md) | Incremental GraphQL delivery: `@stream` and `@defer` over SSE or `multipart/mixed`, negotiation, resumption via `Last-Event-ID`, per-batch auth re-checks, consistency caveats |
| [rest-stream-resumption.md](rest-stream-resumption.md) | `GET /rest/v1/{resource}/stream` resumption: why the resume point is not "every event with a higher seq", what the dispatch ledger records, the four refusals, the replay bound, and where duplicates come from |
| [emit-ddl.md](emit-ddl.md) | The `compile --emit-ddl` directory as a contract with Confiture: file format and version line, determinism, version policy, what it does not carry |
| [vector-search.md](vector-search.md) | pgvector similarity search: declaring vector fields, the `nearest` top-K argument, threshold WHERE filters, the native-column storage contract, emitted DDL |
| [actor-policies.md](actor-policies.md) | Restricting an operation to actor classes (`requires_actor`) and per-class cost budgets: why an allow-list, why delegation is not consulted, the one executor gate that covers every transport, and what makes the classification unforgeable |
| [admin-sql-console.md](admin-sql-console.md) | `POST /api/v1/admin/sql`: the gated arbitrary-SQL endpoint — the three switches that mount it, read-only vs read-write tokens, rollback-by-default and the commit opt-in, timeout/row/one-statement bounds, RLS preview, the audit trail, and what none of it bounds |
| [dev-mode.md](dev-mode.md) | The development edit loop: `fraiseql watch` (compile + zero-downtime reload of a separately running server) vs `run --watch`, failure semantics, drift linting on save |
