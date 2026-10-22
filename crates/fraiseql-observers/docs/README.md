# FraiseQL Observer System Documentation

Observers run side effects (webhooks, email, Slack, cache invalidation, database functions)
after database mutations. In a deployment they run inside `fraiseql-server`, configured
under `[observers]` in `fraiseql.toml`. This directory documents the `fraiseql-observers`
library the server builds on.

## Quick Start

### For Operations

1. **Configure and run** → [Operating Observers](../../../docs/operations/observers.md)
   - Environment overrides, admin HTTP API, scaling, metrics
   - Check runtime status: `GET /api/observers/runtime/health`
   - Inspect execution logs: `GET /api/observers/logs`
   - Manage failures: `GET /api/observers/dlq`

2. **Something is wrong** → [Troubleshooting Guide](troubleshooting.md)

3. **Performance issues** → [Performance Tuning Guide](performance-tuning.md)

4. **Moving from a library-built runtime to the server** → [Migration Guide](migration-guide.md)

### For Developers

1. **Embedding the library** → [Integration Guide](integration-guide.md)
   - Checkpointed driver loop, Redis cache actions, metrics

2. **Architecture** → [Architecture Overview](../../../../docs/architecture/overview.md)

---

## Documentation Index

| Document | Purpose | Audience |
|----------|---------|----------|
| **[Operating Observers](../../../docs/operations/observers.md)** | Configuration, admin HTTP API, environment overrides, scaling, metrics | Operators |
| **[Troubleshooting](troubleshooting.md)** | Problem diagnosis and solutions | Operators, Support |
| **[Performance Tuning](performance-tuning.md)** | Optimization strategies and benchmarking | DevOps, Developers |
| **[Migration Guide](migration-guide.md)** | Moving to the server's observer runtime | Operators, Tech Leads |
| **[Integration Guide](integration-guide.md)** | Embedding the library | Developers |
| **[Architecture Overview](../../../../docs/architecture/overview.md)** | System design | Everyone |

---

## Features

```
Database mutation
        ↓
core.tb_entity_change_log (durable outbox)
        ↓
ChangeLogListener (one poller per listener id, by PostgreSQL advisory lock)
        ↓
ObserverExecutor
  ├─ Event matching
  ├─ Condition evaluation
  ├─ Actions, in order (webhook, email, Slack, cache, database function, log)
  ├─ Retry with backoff
  └─ Dead letter queue for failures
        ↓
Batch recorded in the dispatch ledger, checkpoint saved (at-least-once)
```

- **Persistent checkpoints**: a restart resumes from the stored cursor.
- **Several replicas**: one replica polls; the others stand by and take over.
- **Transports**: PostgreSQL change log, NATS JetStream (`nats` feature), in-memory.
- **Redis**: `cache` actions invalidate keys; the library also offers an action result cache.
- **Prometheus metrics**: `fraiseql_observer_*` series (`metrics` feature).
- **Circuit breaker**: `resilience` module, not installed by the executor or the server.

---

## Common Scenarios

### "I'm losing events on restart"

Checkpoints and the dispatch ledger make delivery at-least-once. See
[Operating Observers - Checkpoints](../../../docs/operations/observers.md#checkpoints) and
[Integration Guide - Persistent Checkpoints](integration-guide.md#persistent-checkpoints).

### "Actions take too long"

The actions of one event run one after another, and no setting runs them in parallel.
Find the slow action with `fraiseql_observer_action_duration_seconds` and lower its timeout
or speed up its target. See
[Operating Observers - Metrics](../../../docs/operations/observers.md#metrics) and
[Performance Tuning](performance-tuning.md).

### "The same action ran twice for one event"

Delivery is at-least-once. Make actions idempotent where a repeat matters. See
[Observer Idempotency](../../../docs/operations/observer-idempotency.md) and
[Troubleshooting - Issue 4](troubleshooting.md#issue-4-duplicate-events-being-processed).

### "I don't know what's happening"

1. Check status: `GET /api/observers/runtime/health`
2. View DLQ: `GET /api/observers/dlq/stats`
3. Inspect an event's executions: `GET /api/observers/logs?event_id=<uuid>`
4. View metrics: scrape the server's `/metrics` (`fraiseql_observer_*`)
5. Read the [Troubleshooting Guide](troubleshooting.md)

---

## Key Concepts

### Listener

The listener polls `core.tb_entity_change_log` for new rows, skipping rows already recorded
in the dispatch ledger. It hands each batch to the executor.

### Checkpoint

A checkpoint stores the last processed change-log id for a listener id. On restart, the
listener resumes from it.

### Dispatch Ledger

After a batch's actions run, the batch is recorded in the ledger. A batch that ran but was
not recorded runs again, so delivery is at-least-once.
