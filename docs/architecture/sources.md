# Sources (Scheduled Ingress) Architecture

A **`Source`** is the dual of an observer. An observer is *egress* — "the database
changed, tell the world." A source is *ingress* — "on a schedule, fetch the world and
drive the results into the database via mutations," resuming from a durable cursor with
at-least-once delivery.

It is opt-in behind the `sources` Cargo feature and, like observers, is
STABLE-tracked-may-evolve.

```text
                    ┌──────────── scheduling envelope (generic) ────────────┐
 cron schedule ───► │ lease.acquire()  →  load cursor                       │
 (per source)       │   ├─ Model A: PullSource::poll() → spine → advance    │ (txn)
                    │   └─ Model B: run connector(ctx: cursor/fetch/mutate) │
                    │ release lease → metrics / logs                        │
                    └───────────────────────────────────────────────────────┘
```

The scheduling / single-firing / cursor / observability **envelope** is generic. What
runs inside it is one of two models (below). This is what makes the primitive
expandable: a native source and a user's TypeScript connector are the same primitive
with a different body.

---

## Two execution models

Authoring is Python/TypeScript; **runtime is pure Rust, no FFI** — so a source's
connector body runs in the Deno functions runtime (`functions-runtime-deno`, the same
mechanism behind `after:mutation` / `after:ingest` / `cron:` functions), not in
Python. The `@source` decorator is authoring *metadata* only: it compiles, it does not
execute.

- **Model A — framework-driven (native Rust `PullSource`).** `poll(ctx) -> PullBatch`
  returns normalized `InboundMessage`s plus the advanced cursor; the framework
  spine-dedups each message, dispatches the existing `after:ingest` path, and advances
  the cursor **transactionally**. The poll-IMAP email adapter is the reference Model A
  source — see [inbound-email.md](inbound-email.md). Best isolation and testability; the
  framework owns idempotency.

- **Model B — handler-driven (Deno connector).** The connector drives the loop itself:
  `ctx.cursor` → `fraiseql_http_request` (fetch) → `fraiseql_query` (mutate) →
  `ctx.advance`. The envelope still supplies single-firing, the cursor store, the
  `run_as` identity, and observability. **This is the connectors-are-user-code path** —
  the boundary is deliberate: a connector is ordinary user TypeScript, sandboxed in the
  Deno runtime under an SSRF allowlist and a bounded `run_as` authority ceiling, not
  trusted first-party infrastructure.

Both share the **same** scheduling / lease / cursor / observability envelope.

---

## Contracts

- **At-least-once, cursor-gated.** Run → writes commit → cursor advances. A crash
  before the advance is safe: the next run re-fetches from the old watermark.
- **Transactional advance is the default for Model A.** The native `PullSource` cursor
  advance participates in the **same transaction** as the ingest writes, so each batch
  is atomic (writes + watermark, or neither) — no reprocess window. Model B (Deno)
  advances after-commit, because its `fraiseql_query` calls are independent
  transactions.
- **Exactly-once business writes are the connector author's job.** The framework
  guarantees at-least-once delivery plus a stable per-firing `idempotencyToken`; it does
  **not** dedupe your domain rows. Write idempotently — natural keys, upserts,
  `ON CONFLICT`. This is the one contract most connectors get wrong, so it bears
  repeating: a source *will* occasionally re-deliver a batch (retry, failover,
  `UIDVALIDITY` reset), and only idempotent writes make that harmless.

---

## Single-firing across replicas

A source scheduled on N replicas must fire on exactly one. Each firing runs under a
PostgreSQL advisory lease keyed on a collision-resistant stable hash of the source name
(`LeaseGuardedRunner`): the replica that acquires the lease runs the connector while
holding it, then releases explicitly; the others skip the tick. Within a replica a
source never overlaps itself (the tick is skipped if the previous run is still going).

The lease is **held for the run and released between ticks** — it is *not* a
steady-state leader election. So "which replica is the leader" is not an observable
property (a PostgreSQL advisory lock exposes no holder); the health signal is the
`fraiseql_source_skips_not_leader_total` metric, which counts, per replica, the ticks a
replica yielded to another.

---

## Identity — `run_as` (fail-closed)

A scheduled source has no request and no JWT, yet its Model B connector's
`fraiseql_query` mutations must run under *some* `SecurityContext` — which governs
operation/field authorization, RLS, and the change-log tenant stamp. A source therefore
runs under an explicit, per-source, least-privilege **authority ceiling**:

```
run_as = { roles: [...], scopes: [...], tenant?: "..." }
```

- **Fail-closed.** A source with **no** `run_as` runs with no authority — the anonymous
  context — so RLS and field authorization deny its writes until an operator grants a
  ceiling. Granting authority is a deliberate act, never a default.
- **The ceiling is static; the tenant is per-message.** `run_as.tenant` pins a
  single-tenant or global source. A **multi-tenant** source (one `stripe_sync` → many
  tenants) leaves `tenant` unset and scopes each write to the message's tenant at
  runtime — only the connector knows which tenant a fetched record belongs to. A source
  already pinned to a tenant cannot forge writes for another.
- Built on `SecurityContext::system_job(...)` (`ActorType::SystemJob`) — an audit-only,
  never-token-derived background principal. The `SystemJob` actor is not itself an
  authorization input; authority comes solely from the configured roles/scopes.

---

## Configuration & feature surface

- **Cargo feature `sources`** (opt-in, mirrors `inbound`) on `fraiseql-server`; the
  default binary stays lean. A compiled schema that declares sources while the feature
  is off boots with a loud warning and no scheduler.
- **`[sources]` TOML** — operator-facing runtime tuning, overridable by environment
  (env > TOML > default):

  ```toml
  [sources]
  enabled = true            # FRAISEQL_SOURCES_ENABLED — global on/off
  allowed_domains = [       # FRAISEQL_SOURCES_ALLOWED_DOMAINS (comma-separated)
    "api.example.com",      # SSRF allowlist for connectors' outbound fetches;
  ]                         # deny-by-default — empty permits no outbound host
  log_payloads = false      # log each firing's trigger payload at debug (opt-in)
  ```

- **Per-source definitions come from the compiled schema** (authoring), mirroring the
  observers split: `name`, `schedule` (cron), `function`, `enabled`, `cursor`, and
  `run_as` live in the `sources` array of `schema.compiled.json`, not in operator TOML.

---

## Authoring surface

The decorator is metadata only — it emits a `sources` entry, it never runs.

```typescript
// TypeScript
@Source({ schedule: "*/5 * * * *", cursor: "orders", runAs: { roles: ["order:write"] } })
```

```python
# Python
@fraiseql.source(schedule="*/5 * * * *", cursor="orders", run_as={"roles": ["order:write"]})
```

The connector body (Model B) is a separate `.ts` handler bound by name; see the runnable
`sdks/official/fraiseql-typescript/examples/sources/poll_orders.connector.ts` for the
`ctx.cursor` → fetch → `ctx.query` upsert → `ctx.advance` loop and the
`ctx.query(mutation, vars, { tenant })` per-message tenant sugar.

---

## Observability

A source is operable — you can see it fire, measure lag, read its cursor, and find
failures — through three surfaces:

**Metrics** (Prometheus facade; exported when the server is built with the `metrics`
feature, like the wire metrics). Emitted per firing from the poller:

| Metric | Type | Labels | Meaning |
|--------|------|--------|---------|
| `fraiseql_source_fires_total` | counter | `source`, `result` | A firing that ran, by outcome (`ok` / `error`). A failed firing re-runs from the last cursor next tick — `result="error"` is the source-failure signal; there is no separate source DLQ. |
| `fraiseql_source_skips_not_leader_total` | counter | `source` | Ticks this replica skipped because another replica held the lease (cross-replica single-firing health). |
| `fraiseql_source_run_duration_seconds` | histogram | `source` | Wall-clock of a firing that ran. |

**Structured logs** — every firing logs `fire` / `skip` / `error` with the `source` and
the per-firing `idempotency_token` (the connector reads the same token via the
`Deno.core.ops.fraiseql_idempotency_token()` host op, to key idempotent writes); a
successful `ctx.advance` logs a value-free "cursor advanced" line (the opaque cursor
value is never logged). The trigger payload is logged only when `[sources] log_payloads`
is enabled.

**Read-only status surface** — `fraiseql sources` lists every compiled source with its
schedule, `run_as` ceiling, and — against a database — its durable cursor: value,
version, and staleness (seconds since the last advance, the lag signal).

```console
$ fraiseql sources --db-url postgres://…
Sources (2)

● orders    schedule */5 * * * *    enabled
    function   pollOrders
    run_as     roles=["order:write"] scopes=[] tenant=(per-message / global)
    cursor     v4 · advanced 37s ago · 2026-07-15T12:07:01+00:00 · value {"page":4}

○ invoices    schedule 0 * * * *    DISABLED
    function   pollInvoices
    run_as     (none — fail-closed: mutations are denied until a run_as ceiling is granted)
    cursor     never advanced (no watermark yet)
```

Add `--json` for a machine-readable snapshot. The cursor is the durable, cross-replica
source of truth for progress; the metrics and logs are the per-firing, per-replica
signal — together they answer "is this source healthy."

---

## See Also

- [inbound-email.md](inbound-email.md) — the poll-IMAP email adapter, the reference
  Model A native `PullSource`.
- [functions.md](functions.md) — the Deno functions runtime, the `after:ingest` host
  surface, and durable dispatch (retry + dead-letter) that Model A sources reuse.
- [webhooks.md](webhooks.md) — the push side of inbound ingestion and the shared
  `InboundMessage` spine; a future fast-follow folds the webhook path under the same
  `Source` umbrella.
