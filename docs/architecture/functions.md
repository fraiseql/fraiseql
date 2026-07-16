# Functions Architecture

The `fraiseql-functions` crate provides a serverless functions runtime for
FraiseQL, enabling event-driven triggers, scheduled tasks, and custom logic
execution alongside the GraphQL engine.

## Overview

```
Mutation → Observer Event → Trigger Registry → Function Execution
                                    ↓
                              Cron Scheduler → Periodic Function Execution
```

Functions are **not** part of the hot query path. They execute asynchronously in
response to mutation events or on a cron schedule.

## Components

### Trigger Registry

The `TriggerRegistry` maps mutation events to function handlers. When a mutation
completes, the observer pipeline checks the registry for matching triggers and
dispatches execution.

- Supports `before_mutation` and `after_mutation` hooks
- `BeforeMutationChain` allows ordered execution of pre-mutation logic
- Triggers are registered at server startup from the compiled schema
- `after:capture` triggers (#366) fire on **externally-captured** writes (a
  third-party daemon / `psql` INSERT) via the change-log reader — the ingress dual
  of `after:mutation`; see [external-write-capture.md](./external-write-capture.md).

### Durable After-Mutation Dispatch

`after:mutation` function dispatch is **durable by default** (ADR 0015): a
transient failure is retried with backoff and, once retries are exhausted, the
invocation is dead-lettered so money- and send-path work is never silently lost.

- **Retry.** Transient failures (5xx, timeouts, execution errors) are retried up
  to `max_attempts` with exponential backoff + jitter. A `4xx` client error is
  treated as permanent and dead-lettered without retry. Backoff and the
  transient/permanent split reuse the observer subsystem via a shared
  `DispatchPolicy` (`fraiseql-observers`), so retries age identically in both.
- **Dead-letter queue.** An exhausted or permanently-failed dispatch is pushed to
  the shared dead-letter queue (`DeadLetterQueue::push_function`), tagged with a
  `DispatchSource` discriminator, under the same size-cap / drop-newest policy as
  observer-action failures. Inspect the count via `function_dlq_count` on the
  observer delivery-health endpoint.
- **Durable DLQ (#598).** The store is selectable via `[functions] dlq_store`:
  `"memory"` (the default — fast, but dead-letters vanish on restart) or
  `"postgres"` (persisted to `_fraiseql_function_dlq`, so a dead-lettered dispatch
  survives a restart and stays listable/replayable). `FRAISEQL_FUNCTIONS_DLQ_STORE`
  overrides the compiled value for production. The Postgres table is a server-owned
  operational table (no RLS; a dead-letter has no single tenant) whose `payload` can
  contain row data — treat it as sensitive: it is never logged at default level and
  is retention-bounded by `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`.
- **Fire-and-forget opt-out.** Set `re_runnable = true` on a function definition
  for re-runnable/idempotent work (e.g. LLM scoring): such dispatch stays
  fire-and-forget with no retry or dead-letter overhead.
- **Configuration.** The retry policy round-trips per-function from the compiled
  schema (`FunctionDefinition.retry`); server-level defaults are overridable via
  `FRAISEQL_FUNCTIONS_RETRY_MAX_ATTEMPTS`,
  `FRAISEQL_FUNCTIONS_RETRY_INITIAL_DELAY_MS`,
  `FRAISEQL_FUNCTIONS_RETRY_MAX_DELAY_MS`, and `FRAISEQL_FUNCTIONS_DLQ_MAX_SIZE`.

Durable dispatch requires the `functions-runtime` feature, which enables the
`observers` subsystem (whose dead-letter-queue store is reused).

### Cron Scheduler (#595)

A `cron:` function fires from a running server: at startup the lifecycle builds one
leased `CronPoller` per cron function (server-side, `crate::cron`) and spawns it on
the server's task set. A cron function is **"a scheduled source without a cursor"** —
the poller reuses the sources' machinery:

- **Cron expressions parsed at startup**; each poller ticks once a minute.
- **Single-firing across replicas** via the sources' PostgreSQL advisory lease
  (`LeaseGuardedRunner`, keyed `cron:<function>`): N replicas → exactly one firing per
  scheduled window. This is the "leader election" the docs previously claimed but did
  not implement.
- **State persisted** to `_fraiseql_cron_state` (`last_fired_at`, `fire_count`) — a
  durable fire record + a cross-restart "already fired this window" guard.
- **Authority:** the function runs on the phase-02 I/O host, so `fraiseql_query` works
  under the function's `run_as` ceiling (fail-closed when absent) — a purge/report job
  can query and mutate.
- **Missed-tick policy: skip.** A server down over a scheduled instant does not replay
  on next boot; the next matching window fires normally (cron has no cursor/backlog to
  resume, unlike a source).

> **Design note (A vs B).** We *implemented* `cron:` (variant A) rather than retiring it
> in favor of cron-scheduled sources (variant B): the `_fraiseql_cron_state` migration
> and the sources' lease were already in place, so the delta was small and the "a
> scheduled job is just a `cron:` function" authoring story is cleaner. The requires
> a DB pool (the lease + state table). The legacy in-process `CronScheduler` firing
> path (`NoopHostContext`, no lease) is superseded by `CronPoller` for production.

### Function runtimes

A function module runs on one of two interchangeable backends, selected per
module by its `runtime` field:

| Runtime | Feature | Authoring | Notes |
|---------|---------|-----------|-------|
| **WASM** (`RuntimeType::Wasm`) | `functions-runtime` | any language → `wasm32-wasip2` component | wasmtime, component model |
| **Deno** (`RuntimeType::Deno`) | `functions-runtime-deno` | JavaScript & TypeScript | V8 isolate. `TypeScript` types are stripped to JS before execution (`deno_ast`/swc), gated by `DenoConfig.enable_typescript` (on by default). |

Both reach the same **I/O-capable host surface** through
`FunctionObserver::invoke_with_context`, which dispatches by the module's runtime.
The host surface (`HostContext`) exposes:

- `http_request` — outbound HTTP, **deny-by-default SSRF allowlist**
  (`FRAISEQL_FUNCTIONS_ALLOWED_DOMAINS`), redirects disabled, DNS-rebind checks.
- `query` (`fraiseql_query`) — execute a GraphQL query or mutation back into the
  engine, under the function's **`run_as`** ceiling (see below). Wired for
  `after:mutation` and scheduled sources; `after:ingest` is a tracked follow-up.
  A function with no `run_as` can *read* only what an anonymous principal can and
  can *write* nothing (fail-closed).
- `storage_get` / `storage_put` — object storage.
- `env_var` — read allowlisted secrets/config.
- `auth_context` — the caller's authenticated context (RLS-aware execution).
- `log` — structured logging captured into the function result.

### Declarative `when` predicates (#597)

An `after:mutation` (or `after:capture`, #366) function can declare *when* it fires,
evaluated by the dispatcher on the row images **before** any runtime spins — a false
predicate produces no dispatch record at all (not a skipped/failed dispatch):

```jsonc
{
  "name": "notify_approved",
  "trigger": "after:mutation:Order:update",
  "when": [                                             // conjunction; omitted = always
    { "field": "status", "changed_to": "approved" },    // transition test (UPDATE-only)
    { "field": "kind",   "eq": "standard" }             // state test (INSERT + UPDATE)
  ]
}
```

- **`eq`** — the field currently equals the value, evaluated on the after-image
  (INSERT/UPDATE) or the pre-image (DELETE). A missing field never equals a value.
- **`changed_to`** — `old.field != v && new.field == v`. UPDATE-only (`changed_to` on
  a non-`update` trigger is a **load error**). A DELETE never matches.
- The list is a **conjunction** (all must hold); an empty/absent `when` always fires
  (back-compat). Exactly one operator per predicate; unknown keys are a load error.
  This is a dispatch filter, not a rules engine — anything richer stays guest code.

> **Pre-image caveat.** The after:mutation **route** path carries only the after-image
> (the mutation response), so `changed_to` there gates on `new.field == v` and cannot
> distinguish a real transition from a re-save. Full transition detection needs the
> pre-image — the `after:capture` path (backed by the change log) with `pre_image=True`.

### Function authority — `run_as` (#594)

A function's `fraiseql_query` writes run under an explicit least-privilege
**ceiling**, exactly the model scheduled sources use (see
[sources.md](./sources.md)). It is declared on the function definition in the
compiled schema:

```jsonc
{
  "name": "recordApproval",
  "trigger": "after:mutation:Order:update",
  "runtime": "Deno",
  "run_as": { "roles": ["order_writer"], "scopes": ["write:order"], "tenant": "acme" }
}
```

- **Fail-closed.** A function with **no `run_as`** runs its bridge under an
  anonymous `system_job` identity — no roles, no scopes, no tenant — so RLS and
  field-authorization deny every write until an operator grants a ceiling. Granting
  authority is a deliberate act, never a default (same words as the sources docs).
- **Audited.** A function-authored write is stamped `system_job:<function-name>`
  under `ActorType::SystemJob` in the change log — the same audit envelope a source
  write carries — so a bridge write is attributable to the function that issued it.
- **Bridge-write asymmetry (deliberate).** A write a function issues through
  `fraiseql_query` does **not** itself fire `after:mutation` functions: after-mutation
  dispatch is invoked only from the GraphQL/REST route handlers, and the bridge wraps
  the core executor, bypassing them. So a bridge-written `Order` update does **not**
  fire `notify_approved`. This is an invariant, not a race — there is no
  bridge→after:mutation loop to guard against.

From a TypeScript guest these are `Deno.core.ops.fraiseql_*` (typed via the
`FRAISEQL_HOST_TYPES` declarations); from a WASM guest they are the
`fraiseql:host/io` imports. Both share **one** `DynHostContext` bridge, so the
SSRF/validation policy is defined once, not per runtime. A host op invoked on a
path that has no live host context (the sync `invoke` path) **fails loud** rather
than silently returning empty data.

## Observability (#598)

Function dispatch is observable on `/metrics` (Prometheus facade; exported when the
server is built with the `metrics` feature, like the source and wire metrics) and in
the structured logs. Emitted per background dispatch:

| Metric | Type | Labels | Meaning |
|--------|------|--------|---------|
| `fraiseql_function_dispatches_total` | counter | `function`, `trigger_kind`, `result` | One background dispatch that ran. `trigger_kind` ∈ {`after:mutation`, `after:ingest`, `after:capture`, `cron`}; `result` ∈ {`ok`, `error`, `dead_lettered`}. A fire-and-forget (`re_runnable`) single-attempt failure is `error`; a durable dispatch that exhausted its retries is `dead_lettered`. |
| `fraiseql_function_run_duration_seconds` | histogram | `function` | Wall-clock of a dispatch that ran (all retry attempts included). |
| `fraiseql_function_predicate_skips_total` | counter | `function` | A `when` predicate (#597) evaluated false, so **no isolate spun** — the zero-cost-skip made visible. |
| `fraiseql_function_dlq_size` | gauge | — | Current function-dispatch DLQ depth (this replica's store view). |
| `fraiseql_function_dlq_evictions_total` | counter | — | Function dead-letters dropped because the DLQ was at capacity (drop-newest). |

**Trigger kinds not metered here.** `before:mutation` runs synchronously in the
request (its outcome is the mutation's own success/failure, already on the GraphQL/HTTP
metrics); `http` edge functions return to their caller and are metered by the HTTP
layer; `after:storage` has no runtime dispatch path yet. None is a background dispatch,
so none is a `fraiseql_function_dispatches_total` row.

**Structured logs** — a dead-letter logs at `error` with the `function`, `attempts`,
and the per-dispatch `idempotency_token`, so an alert traces to the exact dispatch
(and the operator can dedupe a manual replay with the same token). The dead-lettered
`payload` is never logged at default level.

The full platform metric set (sources + functions) is listed in one table in
[sources.md](sources.md#observability) and here; a dashboard consuming both keys off
`fraiseql_source_*` and `fraiseql_function_*`.

## Configuration

Functions are enabled via feature flags on `fraiseql-server`:

- `functions` — edge-function HTTP endpoint + the pure `after:mutation` planner.
  The stock binary compiles only this; no runtime, no live host context.
- `functions-runtime` — actually *run* `after:mutation` functions after commit,
  on a live host context (WASM runtime + `host-live`).
- `functions-runtime-deno` — additionally run **TypeScript/JavaScript** functions
  (Deno/V8). Additive to `functions-runtime`; a separate opt-in because V8 adds
  ~30 MB and compile time.

The embedder assembles the `FunctionsSubsystem` and registers the runtime(s) it
built with on the observer, e.g.:

```rust
observer.register_runtime(RuntimeType::Deno, DenoRuntime::new(&DenoConfig::default())?);
```

Trigger definitions are part of the compiled schema and configured through
Python/TypeScript decorators:

```python
@fraiseql.mutation(
    sql_source="create_user",
    operation="create",
    invalidates=["users"]
)
class CreateUser:
    name: str
    email: str
```

## Crate Dependencies

```
fraiseql-functions
├── fraiseql-error
├── fraiseql-core (optional)
├── fraiseql-db (optional)
├── fraiseql-observers
└── fraiseql-storage (optional)
```

## Testing

```bash
cargo nextest run -p fraiseql-functions --features runtime-wasm,runtime-deno,host-live
cargo test -p fraiseql-server --test platform_e2e_test  # E2E tests
```

> **Use `cargo nextest`, not `cargo test`, for the Deno runtime.** Each test that
> spins up a V8 isolate does so on a fresh thread; the shared-process `cargo test`
> harness can `SIGSEGV` when several isolates are created in one process. Nextest
> runs each test in its own process (fresh V8 platform), which is how CI runs them.

## Authoring: the local invoke harness (`fraiseql functions invoke`)

A function author does not need a running server, a database, or the network to
test a function — `fraiseql functions invoke` runs a compiled function in a **real
V8 isolate** against a fixture payload, with **mocked host ops**, and prints the
result plus every host-op call the guest made. It is the author's inner loop:
fixture → run → observe. Built into the CLI behind the opt-in `functions-invoke`
feature (V8 is ~30 MB, so the stock CLI stays lean).

```bash
# A matching payload runs; --explain shows why the `when` predicate did/didn't fire.
fraiseql functions invoke notifyApproved --payload event.json --explain

# Mock the host ops the function calls (a request matching no mock fails loud).
fraiseql functions invoke syncDeal --payload deal.json \
    --mock-http http.json --mock-query query.json --idempotency-token abc123
```

The module is loaded exactly as the server loads it (from the compiled schema's
`module_dir`). Host ops are answered by a recording mock: `fraiseql_query` /
`fraiseql_http_request` from `--mock-query` / `--mock-http` (a matched entry → its
canned response; a miss against a configured mock **fails loud**, surfacing as a
guest error); other ops return benign defaults so a first run reveals which ops a
function calls before its mocks are written. `--idempotency-token` injects the token
the guest reads via the host op.

**Payload fixtures** are validated against the trigger kind — an `after:mutation` /
`after:capture` fixture is `{ "event_kind": "update", "old": {…}, "new": {…} }` (a
bare object is treated as an insert's `new` image). The `when` predicates (#597) are
evaluated *before* any isolate spins, so a non-matching payload costs nothing.

**Exit codes** are scriptable in CI: `0` = ran; `3` = the `when` predicate did not
match (nothing would fire); `4` = the guest errored; `1` = a config/harness error.

### Typed guest payloads (`functions.d.ts`)

`fraiseql generate-client typescript` emits a `functions.d.ts` alongside the client
whenever the compiled schema declares functions. It gives a function author editor
type-checking for both halves of a function:

- **The host surface** — an ambient `Deno.core.ops.fraiseql_*` declaration
  (`FraiseqlHostOps`), so `fraiseql_query` / `fraiseql_http_request` / … are typed.
- **The event payload** — one interface per function, derived from its trigger. An
  `after:mutation` / `after:capture` function on entity `E` gets
  `{ event_kind, old: E | null, new: E | null }` (with `E` imported from the generated
  `./types`); `cron` gets its schedule context; `after:ingest` gets the inbound-message
  shape. An entity the schema does not define falls back to `unknown` rather than a
  dangling reference.

```typescript
import type { NotifyUserEvent } from "./functions";
// `import type` is erased by the runtime's type-stripper; it is authoring-only.
export default async (event: NotifyUserEvent) => {
  if (event.new?.status !== "approved") return;
  await Deno.core.ops.fraiseql_query(/* … typed host op … */);
};
```

> Tracked follow-ups: `cron` / `after:ingest` payload *synthesis* in `invoke`, and a
> `--record` mode that captures real host-op traffic into the mock files for golden replay.

## See Also

- [Storage Architecture](storage.md) -- Object storage backends
- [Architecture Overview](overview.md) -- System-wide architecture
