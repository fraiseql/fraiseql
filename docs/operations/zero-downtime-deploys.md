# Zero-Downtime Deploys

Every interesting FraiseQL change produces a new `schema.compiled.json`. This document
describes how to roll a new compiled schema across a production fleet without dropping
traffic — and, just as importantly, which problems are **not** FraiseQL's to solve.

> **The principle.** The deploy / load-balancer layer guarantees version-coherent routing
> and drains the old fleet. The application handles graceful shutdown and the versioning of
> shared async state. FraiseQL deliberately does **not** try to serve two compiled schemas
> from one process.

This is the same division of labour every mature stateless service uses. It means the
hard part of a zero-downtime deploy lives in your orchestrator (Kubernetes, Nomad,
[fraisier](#fraisier-the-batteries-included-path), or a hand-rolled blue-green), not inside
`fraiseql-server`.

---

## How a process moves to a new schema

`fraiseql-server` serves exactly one compiled schema **at any instant**. At boot it loads
`schema.compiled.json` and builds an executor held in an `ArcSwap`
(see [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md#server-startup-behaviour)).
There are two ways to move a process — or a fleet of them — to the next schema:

- **In-place atomic reload** (single instance, no restart). Send `SIGUSR1` to the process
  (`kill -USR1 <pid>`), or call `POST /api/v1/admin/reload-schema`. The server re-reads the
  schema file, validates it, and atomically swaps the executor. In-flight requests holding the
  old executor finish on it; new requests get the new schema. No dropped connections, no
  restart. A reload that fails validation **keeps the previous schema** and logs an error
  (`schema_reload_errors_total`) — fail-safe. See
  [runbooks/13-schema-hot-reload-failure.md](../runbooks/13-schema-hot-reload-failure.md) for
  the trigger commands and failure diagnosis.
- **Fleet roll** (multi-instance, the production default). Start new processes on the new
  schema and drain the old ones behind a load balancer ([Patterns 1–3](#pattern-1--rolling-deploy-single-fleet-in-place) below).

The key property either way: a process **never serves two schemas at once**. The swap is
atomic and the old schema is *replaced*, not held alongside the new one for per-request
routing. That is what keeps a request deterministic, and it is why you do not need
dual-schema load or `Accept-Version` routing (see [What you do not need](#what-you-do-not-need)).
Version coherence across a multi-instance fleet is enforced **above** the process by the load
balancer, which already has to exist for a fleet — not by cramming two schemas into one binary.

---

## What FraiseQL already gives you

| Capability | Where | What it does |
|------------|-------|--------------|
| **In-place atomic schema reload** | `SIGUSR1` / `POST /api/v1/admin/reload-schema` | Re-reads and validates the schema file, then atomically swaps the executor (`ArcSwap`). In-flight requests finish on the old schema; new requests get the new one. Failed validation keeps the previous schema. Auto-installed at boot when a schema path is configured. |
| **Graceful shutdown drain** | `server.shutdown_timeout_secs` (default **30s**) | On `SIGTERM`/Ctrl-C the HTTP server stops accepting new connections, lets in-flight requests finish (`axum::serve(...).with_graceful_shutdown`), then the observer runtime is stopped cleanly and remaining lifecycle tasks are drained. Past the timeout, the process logs a warning and exits. |
| **Compiled-schema format guard** | startup | The binary checks the compiled schema's integer `schema_format_version` against the version it expects. A **mismatch is fatal** (refuses to boot); a schema with **no** version field boots with a `WARN`. See [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md#schema-versioning). |
| **Schema-decoupled DLQ retry** | observer DLQ | A dead-lettered action stores the already-**resolved** action payload, so a retry replays that frozen work and never re-resolves against the retrying binary's schema. A v1-produced DLQ entry retried by a v2 binary cannot silently corrupt. See [Observer state across versions](#observer-state-across-versions). |
| **Health / readiness endpoints** | `/health`, `/readiness` (both configurable) | Give the load balancer something to gate on before sending traffic to a new instance. |

These are the entire in-process contribution to a zero-downtime deploy. Everything else
is orchestration.

---

## The hazard is the database, not the binary

Two schema versions running concurrently — whether two fleets behind one load balancer, or one
process whose old in-flight requests still run the previous schema after an in-place reload —
is trivially safe **as long as both schemas work against the database that is live during the
overlap window**. The schema swap is instant and reversible; the database migration is neither.

Use **expand / contract** (a.k.a. parallel-change) migrations so old and new schemas overlap
safely:

1. **Expand** — apply only *additive*, backward-compatible DDL first (new nullable columns,
   new views, new tables). The **old** fleet keeps working against the expanded database.
2. **Deploy** — roll the new fleet. Both versions now run against the expanded database.
3. **Contract** — only after the old fleet is fully retired, apply destructive DDL (drop
   columns, tighten constraints) in a *later* deploy.

Never couple a destructive migration to the same deploy that introduces the binary that
needs it — that removes your ability to roll back. The schema-migration runbook
([runbooks/11-schema-migration.md](../runbooks/11-schema-migration.md)) covers the
compatibility analysis and rollback procedure in detail.

---

## Pattern 1 — Rolling deploy (single fleet, in place)

The default for Kubernetes `Deployment` / Nomad / most PaaS:

```
expand DB  →  start new pods (new schema)  →  readiness gate  →  LB shifts traffic
           →  old pods receive SIGTERM  →  drain (≤ shutdown_timeout_secs)  →  exit
```

Requirements:

- Migration is **expand-only** (Pattern works only with backward-compatible DDL).
- `shutdown_timeout_secs` ≥ your longest reasonable in-flight request; the orchestrator's
  termination grace period must be ≥ `shutdown_timeout_secs` so the OS does not `SIGKILL`
  mid-drain.
- Readiness probe points at the readiness path so the LB never routes to a pod whose schema
  has not finished loading.

---

## Pattern 2 — Blue-green (two fleets, shared Postgres + Redis)

Two complete fleets — **blue** (v1) and **green** (v2) — point at the same Postgres and
Redis. The gateway flips traffic blue → green atomically once green is healthy.

```
expand DB  →  bring up green fleet (v2)  →  health-gate green
           →  gateway flips blue → green (atomic)  →  drain blue  →  tear down blue
```

Why it is safe with shared state:

- **Reads/writes** — both fleets run the same expanded DB; the contract step is deferred
  until blue is gone.
- **Observer DLQ** — entries are schema-decoupled (see below), so a blue-produced failure
  retried by green replays resolved work.
- **Rollback** — if green misbehaves, flip back to blue. Because the DB was only expanded,
  blue still works.

---

## Pattern 3 — Canary

Route a fraction of traffic to v2, watch error rates and latency, then widen.

- **Traffic-percentage canary** (LB weights, e.g. 5% → green) works today with no FraiseQL
  changes — it is just a partial Pattern 2.
- **Per-tenant canary routed *inside a shared fleet*** (a tenant subset served v2 by the same
  process pool) is a **non-goal**. It would require in-process per-tenant schema routing,
  which contradicts "one schema per process." If you need per-tenant version pinning, route
  those tenants to a dedicated fleet at the gateway instead.

---

## fraisier: the batteries-included path

[fraisier](https://github.com/fraiseql/fraisier) is the deploy-orchestration engine for this
workflow. Every deploy is a **saga** — an ordered list of compensable steps that commit as a
whole or roll back in reverse, across one host or many:

```
preflight → fetch → migrate → activate → restart → health → verify
```

Each step registers its inverse as it completes. If the post-activation health check fails,
fraisier re-activates the previous release from a durable ledger **and migrates the database
back down**, in the right order, on every host. Its five adapter axes map directly onto the
patterns above:

| Axis | Role in a zero-downtime deploy |
|------|--------------------------------|
| `artifact` | fetch + verify + atomically activate the new `fraiseql-server` + `schema.compiled.json` |
| `migration` | apply the expand step (and compensate / roll back on failure) |
| `service` | restart the fleet member (systemd / docker-compose / rc) |
| `health` | probe the configured health endpoint before declaring the instance live |
| `lb` | drain + reattach + swap traffic (nginx) — the blue-green flip |

fraisier is the recommended path, but nothing here is fraisier-specific: any orchestrator
that can health-gate, drain, and roll back (Kubernetes, Nomad, Argo Rollouts, …) implements
the same patterns.

> fraisier is pre-release (`1.0.0-beta.3` at time of writing). The in-process guarantees above
> (in-place reload, graceful drain, schema-format guard, schema-decoupled DLQ) are
> deploy-tool-agnostic, so operators on Kubernetes or any other LB get the same zero-downtime
> story.

---

## Subscriptions during a roll

WebSocket subscriptions are **per-instance**: a subscription is bound to the binary that
accepted it. During a roll:

1. The LB stops routing **new** connections to the draining instance (Pattern 1 readiness
   flip / Pattern 2 gateway flip).
2. The draining instance stops accepting new connections. Active subscription sockets close as
   the instance winds down — either when its event source stops feeding the subscription loop,
   or when the orchestrator's termination grace period elapses and the process exits.
3. A subscription client whose socket closes **reconnects** and lands on the new fleet, which
   re-establishes the subscription against the new schema.

Two consequences for operators:

- **Size the termination grace period deliberately.** The HTTP server's graceful shutdown
  waits for in-flight work; long-lived subscription sockets are not force-closed on a fixed
  timer, so the orchestrator's grace period is the real upper bound on a draining instance's
  lifetime. Set it to a value you are comfortable having a subscriber hang onto a retiring
  pod for.
- **Clients must reconnect-and-resubscribe on disconnect** (standard for any long-lived
  WebSocket). FraiseQL does not migrate a live subscription from a v1 process to a v2 process
  — that per-instance affinity is what keeps subscriptions deterministic.

> **Known gap.** On shutdown the server does not send a protocol-level `Complete`/close frame
> with a "server draining, please reconnect" reason to each active subscription; the socket
> simply closes. A graceful per-subscription close on drain is a possible future enhancement.
> It does not affect correctness — a reconnecting client re-subscribes cleanly — only the
> tidiness of the client-side signal.

---

## Observer state across versions

Observer dead-letter retries are **schema-decoupled by construction**, which is why
blue-green and rolling deploys are safe for observer state:

- A failed action is dead-lettered with its already-**resolved** action payload, not a
  reference that must be re-resolved against a compiled schema.
- A retry replays that frozen payload. A DLQ entry produced under schema v1 and retried under
  a v2 binary therefore replays v1's resolved action — schema drift across the fleet cannot
  silently corrupt the retry.

**Audit follow-up (optional).** The DLQ does not yet record *which* schema version produced a
failed action. Recording it would be an observability nicety, not a correctness fix; the
clean way to add it is to surface the change-spine `schema_version` envelope column onto the
observer `EntityEvent` (reader-side of the schema-versioning work), after which it rides into
the DLQ entry automatically. This is tracked separately and intentionally **not** bolted onto
the legacy `observer_dlq_items` SQL table, which the live Redis-backed DLQ does not use.

---

## What you do **not** need

These were once imagined as in-server features; the deploy-layer approach makes them
unnecessary:

- ❌ **Loading N compiled schemas concurrently in one binary** (`ArcSwap<HashMap<SchemaVersion, …>>`).
  Moving to the next schema is handled by in-place atomic reload (single instance) or a fleet
  roll (multi-instance) — never by holding two schemas at once for per-request routing.
- ❌ **`Accept-Version`-header schema routing.** The LB already routes version-coherently.
- ❌ **Per-connection schema affinity machinery.** Subscriptions are per-instance by design;
  reconnect handles the roll.

---

## Operator checklist

- [ ] Migration is **expand-only** for this deploy; destructive DDL is deferred to a later
      contract deploy.
- [ ] Orchestrator termination grace period ≥ `server.shutdown_timeout_secs`.
- [ ] Readiness probe gates traffic on schema load completion.
- [ ] Compiled-schema `schema_format_version` matches the binary's expected version (a
      mismatch is a fatal boot refusal); see
      [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md#schema-versioning).
- [ ] Rollback path verified: because the DB was only expanded, the previous fleet still runs.
- [ ] Subscription clients reconnect-and-resubscribe on disconnect.

---

## See also

- [compiled-schema-lifecycle.md](compiled-schema-lifecycle.md) — how `schema.compiled.json`
  moves from CI to production, and the startup / reload contract.
- [runbooks/01-deployment.md](../runbooks/01-deployment.md) — step-by-step deploy, health
  checks, and rollback commands.
- [runbooks/11-schema-migration.md](../runbooks/11-schema-migration.md) — compatibility
  analysis and rollback for a compiled-schema update.
- [runbooks/13-schema-hot-reload-failure.md](../runbooks/13-schema-hot-reload-failure.md) —
  triggering an in-place `SIGUSR1` / admin-endpoint reload and diagnosing a failed reload.
- [observer-idempotency.md](observer-idempotency.md) — `EffectivelyOnce` checkpoints, which
  keep observer delivery correct across a fleet roll.
