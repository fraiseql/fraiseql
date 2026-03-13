# Observer Idempotency Table Operations Guide

When using `CheckpointStrategy::EffectivelyOnce`, the observer stores a unique key for each
processed event before acknowledging it. If the same event is delivered again (e.g., after
a crash or broker redelivery), the key is found and the event is skipped.

This prevents duplicate processing of non-idempotent operations: billing charges, audit log
writes, email sends, and external API calls where duplicate execution would be visible to
end users.

---

## Table Schema

The `create_table_if_not_exists()` method on `CheckpointStrategy` creates the table
automatically on first use. You can also create it manually in a migration:

```sql
CREATE TABLE IF NOT EXISTS observer_idempotency_keys (
    idempotency_key  TEXT        NOT NULL,
    listener_id      TEXT        NOT NULL,
    processed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (idempotency_key, listener_id)
);

-- Index for cleanup queries (see Cleanup Strategy below)
CREATE INDEX IF NOT EXISTS idx_observer_idempotency_processed_at
    ON observer_idempotency_keys (processed_at);
```

Each observer should use a **distinct table name** to avoid cross-observer key collisions:

```python
@fraiseql.subscription(
    entity_type="Order",
    topic="order.created",
    operation="create",
    checkpoint=fraiseql.EffectivelyOnce(
        idempotency_table="billing_observer_keys"   # unique per observer
    )
)
def handle_order_created(order: Order) -> None:
    charge_customer(order)
```

---

## Auto-Creation at Startup

`CheckpointStrategy::EffectivelyOnce` exposes a `create_table_if_not_exists()` method:

```rust
// Called automatically during observer initialization
strategy.create_table_if_not_exists(&pool).await?;
```

The observer **fails closed** if table creation fails (e.g., insufficient permissions).
It will not start without the idempotency table in place.

---

## Table Growth Rate

The table grows by one row per processed event. Plan accordingly:

| Events/day | Rows/month | Rows/year | Approx. table size/year |
|-----------|-----------|----------|------------------------|
| 1,000 | 30,000 | 365,000 | ~50 MB |
| 10,000 | 300,000 | 3.65 M | ~500 MB |
| 100,000 | 3 M | 36.5 M | ~5 GB |

Sizes are approximate; actual size depends on key length and PostgreSQL page fill factor.

---

## Cleanup Strategy

Idempotency keys older than your message broker's maximum redelivery window are safe to
delete. If your broker retains messages for 7 days, keys older than 7 days cannot be
redelivered and can be purged.

Recommended: run this daily via `pg_cron` or an external scheduler:

```sql
DELETE FROM observer_idempotency_keys
WHERE processed_at < NOW() - INTERVAL '7 days';
```

Adjust the interval to match your broker's retention policy. Typical values:

| Broker | Default retention | Recommended cleanup interval |
|--------|------------------|------------------------------|
| NATS JetStream | Configurable (default unlimited) | Match stream `max_age` |
| Redis Streams | Configurable | Match stream `MAXLEN` retention |
| pg_notify | Until consumed | 24 hours (pg_notify is ephemeral) |

Add a monitoring alert if the table exceeds 10 million rows; this indicates either
the cleanup job is not running or the broker is experiencing abnormal redelivery rates.

---

## Failure Modes

| Scenario | Behaviour |
|----------|-----------|
| Table missing at observer startup | Observer logs ERROR and refuses to start (fail closed) |
| Table missing mid-operation | Event processing fails; message is requeued by broker |
| Database write fails before acknowledgment | Event is NOT acknowledged; will be redelivered |
| Duplicate key found (idempotency hit) | Event is skipped; acknowledgment sent to broker |
| Database unreachable | Observer pauses and retries with backoff |

The observer always fails closed: if deduplication cannot be guaranteed, events are not
processed.

---

## Monitoring

The observer exposes deduplication metrics on `/metrics` (Prometheus format):

```
# Total events processed successfully
fraiseql_observer_events_processed_total

# Total duplicate events detected and skipped
fraiseql_observer_dedup_detected_total

# Total processing cycles saved by deduplication
fraiseql_observer_dedup_processing_skipped_total
```

A rising `fraiseql_observer_dedup_detected_total` indicates your message broker is
redelivering messages. Investigate:

1. Is the observer crashing before it can acknowledge messages?
2. Is the broker's acknowledgment timeout set too short?
3. Is the observer processing time close to the broker's ack deadline?

---

## Operational Runbook

### "Observer failed to start: idempotency table missing"

```sql
-- Create the table manually (requires CREATE TABLE permissions)
CREATE TABLE IF NOT EXISTS observer_idempotency_keys (
    idempotency_key  TEXT        NOT NULL,
    listener_id      TEXT        NOT NULL,
    processed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (idempotency_key, listener_id)
);
```

Then restart the observer. It will call `create_table_if_not_exists()` and proceed.

### "Table is too large / cleanup is failing"

```sql
-- Check current size
SELECT
    relname,
    pg_size_pretty(pg_total_relation_size(relid)) AS total_size,
    pg_size_pretty(pg_relation_size(relid)) AS table_size,
    n_live_tup AS live_rows
FROM pg_stat_user_tables
WHERE relname = 'observer_idempotency_keys';

-- Delete old keys in batches to avoid lock pressure
DO $$
DECLARE
  rows_deleted INT;
BEGIN
  LOOP
    DELETE FROM observer_idempotency_keys
    WHERE ctid IN (
      SELECT ctid FROM observer_idempotency_keys
      WHERE processed_at < NOW() - INTERVAL '7 days'
      LIMIT 10000
    );
    GET DIAGNOSTICS rows_deleted = ROW_COUNT;
    EXIT WHEN rows_deleted = 0;
    PERFORM pg_sleep(0.1);  -- yield to other queries
  END LOOP;
END;
$$;

-- Reclaim space after bulk delete
VACUUM ANALYZE observer_idempotency_keys;
```

### "I need to reset an observer to reprocess from the beginning"

Clearing idempotency keys allows events to be reprocessed. Do this with care:

```sql
-- Clear keys for a specific observer only
DELETE FROM observer_idempotency_keys WHERE listener_id = 'my-observer-name';
```

After clearing, also delete the checkpoint so the observer starts from the beginning:

```sql
DELETE FROM observer_checkpoints WHERE listener_id = 'my-observer-name';
```
