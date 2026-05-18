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
