# ClickHouse Migrations

This directory contains SQL migrations for FraiseQL's ClickHouse analytics database.

## Overview

The migrations set up:

1. **fraiseql_events** - Main event table (MergeTree)
   - Raw analytics events from observers
   - Partitioned by month for efficient deletion
   - 90-day retention via TTL
   - Bloom filter indexes on frequently-filtered columns

2. **fraiseql_events_hourly** - Hourly aggregation (SummingMergeTree)
   - Event counts by type and entity
   - Unique entity counts per hour
   - Auto-aggregates on insert

3. **fraiseql_org_daily** - Daily organization statistics (SummingMergeTree)
   - Events per organization per day
   - Unique user and entity counts
   - Create/Update/Delete event breakdowns

4. **fraiseql_event_type_stats** - Event type performance metrics (SummingMergeTree)
   - Hourly event type distribution
   - Average and max JSON data sizes
   - Useful for monitoring and capacity planning

## Applying Migrations

### Option 1: Docker Compose (Recommended for Development)

The `docker-compose.clickhouse.yml` file auto-applies migrations:

```bash
docker-compose -f docker-compose.clickhouse.yml up -d
```

ClickHouse will automatically execute all SQL files in `/docker-entrypoint-initdb.d/` on startup.

### Option 2: Manual Application via clickhouse-client

Connect to ClickHouse and apply the migration:

```bash
# From the fraiseql project root
clickhouse-client -h localhost \
  --multiquery < migrations/clickhouse/001_events_table.sql
```

Or connect interactively:

```bash
clickhouse-client -h localhost
```

Then paste the SQL from `001_events_table.sql`.

### Option 3: Using Python clickhouse-driver

```python
from clickhouse_driver import Client

client = Client('localhost')

with open('migrations/clickhouse/001_events_table.sql') as f:
    sql = f.read()

client.execute(sql)
```

## Verification

After applying migrations, verify the setup:

```bash
# Check tables exist
clickhouse-client -h localhost \
  --query "SELECT name FROM system.tables WHERE database='default' AND name LIKE 'fraiseql%'"

# Check main table schema
clickhouse-client -h localhost \
  --query "DESCRIBE TABLE fraiseql_events"

# Check materialized views
clickhouse-client -h localhost \
  --query "SELECT name FROM system.tables WHERE table_type='MaterializedView' AND database='default'"
```

## Sample Queries

### Insert Test Events

```sql
INSERT INTO fraiseql_events (event_id, event_type, entity_type, entity_id, timestamp, data, user_id, org_id)
VALUES
    ('evt-001', 'created', 'User', 'user-123', 1700000000000000, '{"name":"Alice"}', 'admin-1', 'org-1'),
    ('evt-002', 'updated', 'User', 'user-123', 1700001000000000, '{"email":"alice@example.com"}', 'admin-1', 'org-1'),
    ('evt-003', 'created', 'Order', 'order-456', 1700002000000000, '{"total":99.99}', 'user-123', 'org-1');
```

### Query Hourly Aggregations

```sql
SELECT
    hour,
    event_type,
    entity_type,
    event_count,
    unique_entities
FROM fraiseql_events_hourly
ORDER BY hour DESC
LIMIT 10;
```

### Query Organization Daily Stats

```sql
SELECT
    day,
    org_id,
    event_count,
    unique_users,
    unique_entities,
    created_count,
    updated_count,
    deleted_count
FROM fraiseql_org_daily
WHERE day >= today() - INTERVAL 7 DAY
ORDER BY day DESC, event_count DESC;
```

### Query Event Type Performance Metrics

```sql
SELECT
    hour,
    event_type,
    event_count,
    formatReadableSize(avg_data_size_bytes) as avg_size,
    formatReadableSize(max_data_size_bytes) as max_size
FROM fraiseql_event_type_stats
WHERE hour >= subtractHours(now(), 24)
ORDER BY hour DESC, event_count DESC;
```

### Check Table Size and TTL Status

```sql
-- Table size
SELECT
    name,
    formatReadableSize(total_bytes) as size,
    partition_count,
    part_count
FROM system.tables
WHERE database = 'default' AND name = 'fraiseql_events'
FORMAT Vertical;

-- Rows in main table
SELECT formatReadableQuantity(count()) as total_rows
FROM fraiseql_events;

-- Partition info (TTL cleanup status)
SELECT
    partition,
    partition_key,
    rows,
    bytes
FROM system.parts
WHERE database = 'default' AND table = 'fraiseql_events'
ORDER BY partition DESC
LIMIT 10;
```

## Performance Tuning

### Index Settings

The migration creates Bloom filter indexes on:
- `user_id` - for filtering by user
- `org_id` - for filtering by organization
- `event_type` - for filtering by event type

These indexes are useful for the `WHERE` clauses in the aggregations. Adjust `GRANULARITY` if needed:
- `GRANULARITY 1` - index every row (precise, larger index)
- `GRANULARITY 8192` - index every 8192 rows (smaller, less precise)

### Partitioning Strategy

Current: Monthly partitions by `toYYYYMM(timestamp)`

Alternatives:
- Daily: `toYYYYMMDD(timestamp)` - more granular deletion, more partitions
- Weekly: Custom function - balanced approach
- Yearly: `toYear(timestamp)` - fewer partitions, coarser deletion

### TTL Settings

Current: 90-day retention (`INTERVAL 90 DAY`)

To adjust:
```sql
ALTER TABLE fraiseql_events MODIFY TTL timestamp + INTERVAL 365 DAY;
```

To disable TTL:
```sql
ALTER TABLE fraiseql_events REMOVE TTL;
```

### Aggregation Settings

The SummingMergeTree tables automatically aggregate on background merges. To force immediate aggregation:

```sql
OPTIMIZE TABLE fraiseql_events_hourly FINAL;
OPTIMIZE TABLE fraiseql_org_daily FINAL;
OPTIMIZE TABLE fraiseql_event_type_stats FINAL;
```

## Troubleshooting

### Materialized View Not Updating

If aggregation views are empty after inserts:

```sql
-- Check if view is processing correctly
SELECT count() FROM fraiseql_events;

-- Check view status
SELECT * FROM system.tables
WHERE name = 'fraiseql_events_hourly_mv' FORMAT Vertical;

-- Try manual optimization
OPTIMIZE TABLE fraiseql_events FINAL;
OPTIMIZE TABLE fraiseql_events_hourly FINAL;
```

### Storage Issues

Monitor disk usage:

```sql
SELECT
    table,
    formatReadableSize(sum(bytes)) as size
FROM system.parts
WHERE database = 'default' AND table LIKE 'fraiseql%'
GROUP BY table
ORDER BY size DESC;
```

Clean old partitions manually:

```sql
-- Delete partitions older than 120 days
ALTER TABLE fraiseql_events DELETE WHERE timestamp < now64Milli() - INTERVAL 120 DAY;
```

### Connection Issues

If Docker container is running but clickhouse-client can't connect:

```bash
# Check container is running
docker ps | grep clickhouse

# Check logs
docker logs <container-id>

# Test connectivity
docker exec fraiseql-clickhouse clickhouse-client --query "SELECT 1"

# Access via Docker network
docker run --network fraiseql_default clickhouse/clickhouse-client:latest \
  --host clickhouse --query "SELECT 1"
```
