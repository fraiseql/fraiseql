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
  observer-action failures. Inspect it via `function_dlq_count` on the observer
  delivery-health endpoint.
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

## See Also

- [Storage Architecture](storage.md) -- Object storage backends
- [Architecture Overview](overview.md) -- System-wide architecture
