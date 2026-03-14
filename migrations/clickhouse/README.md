# ClickHouse Migrations

This directory contains database migrations for the ClickHouse analytics database used by FraiseQL's Arrow Flight integration.

## Overview

The migrations set up:

1. **fraiseql_events** - Main table storing all observer events (90-day TTL)
2. **fraiseql_events_hourly** - Hourly aggregations by entity and event type
3. **fraiseql_org_daily** - Daily organization statistics
4. **fraiseql_event_type_stats** - Event type distribution metrics
5. Helper functions for common analytics queries

## Schema

### Main Events Table

```sql
CREATE TABLE fraiseql_events (
    event_id String,
    event_type String,
    entity_type String,
    entity_id String,
    timestamp DateTime,
    data String,  -- JSON
    user_id Nullable(String),
    org_id Nullable(String)
)
```

**Indexes:**

- Bloom filter on `event_type` for fast filtering
- Bloom filter on `entity_type` for entity queries
- Bloom filter on `org_id` for organization filtering

**Storage:**

- MergeTree engine with monthly partitioning
- Ordered by `(entity_type, timestamp)` for efficient queries
- 90-day TTL for automatic cleanup

### Materialized Views

**fraiseql_events_hourly**: Hourly counts and unique entity counts

- Aggregates: `event_count`, `unique_entities`
- Grouped by: hour, entity_type, event_type
- Retention: 120 days

**fraiseql_org_daily**: Daily organization statistics

- Aggregates: event count, unique entities, unique users
- Grouped by: day, org_id
- Retention: 90 days

**fraiseql_event_type_stats**: Event type distribution

- Aggregates: count, rate (events/second)
- Grouped by: hour, event_type
- Retention: 120 days

## Applying Migrations

### Docker Compose (Automatic)

Place migration files in a directory and mount to ClickHouse container:

```yaml
services:
  clickhouse:
    image: clickhouse/clickhouse-server:24
    volumes:
      - ./migrations/clickhouse:/docker-entrypoint-initdb.d:ro
```

ClickHouse automatically applies all SQL files in `/docker-entrypoint-initdb.d` at startup.

### Manual Application

Connect to ClickHouse and run:

```bash
# Using clickhouse-client (native protocol)
clickhouse-client < migrations/clickhouse/001_events_table.sql

# Or via HTTP
curl -X POST "http://localhost:8123/" --data-binary @migrations/clickhouse/001_events_table.sql
```

## Common Queries

### Events for a specific entity (last 100)

```sql
-- Using helper function
SELECT * FROM get_entity_events('order-123', 100);

-- Or direct query
SELECT event_id, event_type, timestamp, data
FROM fraiseql_events
WHERE entity_id = 'order-123'
ORDER BY timestamp DESC
LIMIT 100;
```

### Event counts by entity type (last 24 hours)

```sql
-- Using helper function
SELECT * FROM count_events_by_entity_type(24);

-- Or direct query
SELECT 
    entity_type,
    sum(event_count) as total_events
FROM fraiseql_events_hourly
WHERE hour >= now() - INTERVAL 24 HOUR
GROUP BY entity_type
ORDER BY total_events DESC;
```

### Organization activity summary

```sql
-- Using helper function
SELECT * FROM org_activity_summary('org-001', 30);

-- Or direct query
SELECT 
    day,
    event_count,
    unique_entities,
    unique_users
FROM fraiseql_org_daily
WHERE org_id = 'org-001' AND day >= now() - INTERVAL 30 DAY
ORDER BY day DESC;
```

### Event rate by type (last hour)

```sql
SELECT 
    event_type,
    count as event_count,
    rate as events_per_second
FROM fraiseql_event_type_stats
WHERE hour >= now() - INTERVAL 1 HOUR
ORDER BY events_per_second DESC;
```

### Hourly breakdown (last 24 hours)

```sql
SELECT 
    hour,
    entity_type,
    event_type,
    event_count,
    unique_entities
FROM fraiseql_events_hourly
WHERE hour >= now() - INTERVAL 24 HOUR
ORDER BY hour DESC, event_count DESC;
```

## Monitoring

### Check table sizes

```sql
SELECT 
    table,
    formatReadableSize(total_bytes) as size,
    rows
FROM system.tables
WHERE database = 'default' AND name LIKE 'fraiseql%'
ORDER BY total_bytes DESC;
```

### Monitor TTL cleanup

```sql
SELECT 
    table,
    rows,
    modification_time
FROM system.parts
WHERE database = 'default' AND table = 'fraiseql_events'
ORDER BY modification_time DESC;
```

### Check materialized view lag

```sql
SELECT 
    name,
    target_table,
    is_ready
FROM system.views
WHERE database = 'default' AND name LIKE 'fraiseql%_mv';
```

### Query system.query_log for performance analysis

```sql
SELECT 
    query_start_time,
    type,
    query,
    query_duration_ms,
    read_rows,
    result_rows
FROM system.query_log
WHERE database = 'default' AND type = 2  -- QueryFinish
  AND query_start_time >= now() - INTERVAL 1 HOUR
ORDER BY query_start_time DESC
LIMIT 50;
```

## Performance Tuning

### Adjust batch size if needed

The Arrow Flight sink default batch size is 10,000 rows. Adjust if:

- Batches are taking >100ms: reduce batch_size
- Memory pressure: reduce batch_size
- Network latency high: increase batch_size

```toml
# In fraiseql config
[observers.clickhouse]
batch_size = 20000  # Default is 10,000
batch_timeout_secs = 5
```

### Monitor Bloom filter efficiency

Bloom filters are configured with `GRANULARITY 1` (very precise). If memory is constrained:

```sql
-- Update index granularity
ALTER TABLE fraiseql_events MODIFY SETTING index_granularity = 16384;
```

### Adjust TTL if needed

Change 90-day retention:

```sql
ALTER TABLE fraiseql_events MODIFY TTL timestamp + INTERVAL 120 DAY;
ALTER TABLE fraiseql_org_daily MODIFY TTL day + INTERVAL 120 DAY;
```

## Troubleshooting

### Events not appearing in ClickHouse

1. Check Arrow Flight sink is running:

   ```bash
   docker logs fraiseql-arrow
   ```

2. Verify NATS has events:

   ```bash
   nats stream info fraiseql_events
   ```

3. Check ClickHouse connectivity:

   ```bash
   curl -s http://localhost:8123/?query="SELECT 1" | head
   ```

### Slow queries on fraiseql_events

1. Verify indexes exist:

   ```sql
   SELECT * FROM system.indexes WHERE table = 'fraiseql_events';
   ```

2. Check partition pruning is working:

   ```sql
   EXPLAIN
   SELECT * FROM fraiseql_events
   WHERE timestamp >= now() - INTERVAL 7 DAY;
   ```

3. Review table settings:

   ```sql
   SHOW CREATE TABLE fraiseql_events;
   ```

### Materialized views not updating

1. Check view status:

   ```sql
   SELECT name, target_table, is_ready FROM system.views WHERE name LIKE 'fraiseql%_mv';
   ```

2. Check for errors in query log:

   ```sql
   SELECT * FROM system.query_log
   WHERE query LIKE '%fraiseql_events_hourly_mv%'
   ORDER BY query_start_time DESC;
   ```

3. Manually refresh (if stuck):

   ```sql
   -- Drop and recreate view
   DROP VIEW fraiseql_events_hourly_mv;
   CREATE MATERIALIZED VIEW fraiseql_events_hourly_mv TO fraiseql_events_hourly AS
   SELECT ...;
   ```

## Architecture

```
PostgreSQL Events
    ↓ (via NATS)
Observer Event Stream (NATS JetStream)
    ↓ (via Arrow Bridge)
Arrow RecordBatch
    ↓ (via ClickHouse Sink)
fraiseql_events table (MergeTree)
    ↓
┌───────────┬─────────────┬──────────────┐
↓           ↓             ↓              ↓
hourly      org_daily     event_type_    (future)
aggregations stats        stats          analytics
```

## Future Enhancements

- [ ] Time-series predictions using ML models
- [ ] Anomaly detection on event rates
- [ ] Custom aggregations per organization
- [ ] Event replication to S3 for long-term storage
- [ ] Integration with Grafana for dashboards
