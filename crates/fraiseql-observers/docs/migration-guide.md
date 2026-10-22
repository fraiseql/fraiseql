# Migration Guide: Running Observers in `fraiseql-server`

This guide moves a deployment that assembles observers from the `fraiseql-observers`
library to the observer runtime inside `fraiseql-server`. The server reads `[observers]`
from `fraiseql.toml`. [Operating Observers](../../../docs/operations/observers.md)
documents its configuration, admin API, scaling and metrics.

Upgraders: the library's deduplication, job queue, search sink, NATS bridge, failover
coordinator and runtime config document are gone. Use the server runtime below.

## What the Server Runs

- The checkpointed change-log driver loop (at-least-once delivery)
- One poller per listener id, chosen by a PostgreSQL advisory lock
- `cache` actions, through `[observers.runtime.redis]`
- The dead letter queue and its admin endpoints
- `fraiseql_observer_*` metrics on `/metrics`, with the `observers-metrics` feature

The server runs no action-result cache.

---

## Step 1: Apply the Checkpoint Table

The checkpoint table ships as
`crates/fraiseql-observers/migrations/02_create_observer_checkpoints.sql`. If you already ran
the library's checkpointed loop, the table exists and the server resumes from it.

## Step 2: Move Configuration to `fraiseql.toml`

Declare observers and the runtime under `[observers]`. Transport settings go in
`[observers.runtime.transport]` (`transport` and `nats` only). Redis for `cache` actions
goes in `[observers.runtime.redis]`.

The server refuses keys and variables it does not run, rather than ignoring them. Remove
`run_bridge`, `run_executors` and any `[bridge]` table, and unset
`FRAISEQL_NATS_RUN_EXECUTORS`, `FRAISEQL_NATS_ENABLE_BRIDGE`, `FRAISEQL_BRIDGE_*`,
`FRAISEQL_REDIS_POOL_SIZE`, `FRAISEQL_REDIS_DEDUP_WINDOW_SECS` and
`FRAISEQL_REDIS_CACHE_TTL_SECS`.

## Step 3: Test

```bash
# 1. Verify checkpoint saved
psql $DATABASE_URL -c "SELECT * FROM observer_checkpoints LIMIT 5;"

# 2. Test recovery: restart fraiseql-server, verify no re-processing.
#    On start its log shows "Restored observer change-log checkpoint; resuming (no replay)"

# 3. Verify the runtime's cursor
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" \
  http://localhost:8000/api/observers/runtime/health | jq .last_checkpoint
```

## Step 4: Monitor

```bash
# Metrics
curl -s -H "Authorization: Bearer $METRICS_TOKEN" \
  http://localhost:8000/metrics | grep fraiseql_observer_

# Dead letter queue
curl -s -H "Authorization: Bearer $ADMIN_TOKEN" http://localhost:8000/api/observers/dlq/stats

# Saved checkpoints
psql $DATABASE_URL -c "SELECT listener_id, last_processed_id, updated_at FROM observer_checkpoints;"
```

---

## Running Old and New Side by Side

Do not run the library driver and the server against the same change log at the same
time. Each listener id has its own checkpoint and dispatch ledger, so two drivers with
different ids both run every action. Stop the library driver, then start the server.

The server's listener id is `change_log` and is not configurable in `fraiseql.toml`. A
library driver that used `change_log` shares the server's checkpoint and ledger. One that
used another id does not, and the server keeps its own cursor.

## Rollback

```bash
# 1. Stop fraiseql-server's observer runtime (remove [observers] or stop the server)
# 2. Restart the previous library driver.
```

This resumes without loss only when both use the same listener id.

---

## Validation Checklist

- [ ] All tests passing
- [ ] No error rate increase
- [ ] Database connections stable
- [ ] DLQ empty (no action failures)
- [ ] Restart resumes from the checkpoint
- [ ] Alerting configured
- [ ] Rollback plan verified

## Support

- Troubleshooting: `troubleshooting.md`
- Operating observers: `../../../docs/operations/observers.md`
- Architecture: `../../../../docs/architecture/overview.md`
