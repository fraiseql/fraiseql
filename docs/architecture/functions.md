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

### WASM Runtime (Experimental)

Functions can be authored as WASM modules:

- Host bridge infrastructure for calling back into FraiseQL
- `SecurityContext` injection for RLS-aware function execution
- Deno-compatible host operations framework

## Configuration

Functions are enabled via the `functions` feature flag on `fraiseql-server`.
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
cargo test -p fraiseql-functions --lib     # Unit tests (118 pass)
cargo test -p fraiseql-server --test platform_e2e_test  # E2E tests
```

## See Also

- [Storage Architecture](storage.md) -- Object storage backends
- [Architecture Overview](overview.md) -- System-wide architecture
