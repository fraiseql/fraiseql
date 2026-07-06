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

### Cron Scheduler

The `CronScheduler` manages periodic function execution:

- Cron expressions parsed at startup
- State persisted in PostgreSQL (`cron_state` migration)
- Leader election prevents duplicate execution in multi-instance deployments

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
- `query` — execute a GraphQL query back into the engine.
- `storage_get` / `storage_put` — object storage.
- `env_var` — read allowlisted secrets/config.
- `auth_context` — the caller's authenticated context (RLS-aware execution).
- `log` — structured logging captured into the function result.

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
