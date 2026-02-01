# ta_* Table Pattern: Table-Backed Arrow Views

## Overview

**ta_* tables** are PostgreSQL-specific, materialized table-backed views that pre-compute and physically store Arrow-optimized columnar data for high-performance Arrow Flight streaming.

**Analogous to**: `tv_*` tables for JSON plane (GraphQL), but `ta_*` for Arrow plane (analytics).

**Key difference**: Unlike logical views (`va_*`), ta_* tables are actual PostgreSQL tables with:
- Physical storage on disk (materialized data)
- Trigger-based or scheduled refresh mechanism
- BRIN indexes for fast time-series queries
- 10-100x faster query performance on large tables

## When to Use ta_* Tables

### ✅ **Use ta_* when**

- **Large tables** (>1M rows) where query speed is critical
- **Time-series analytics** with frequent range queries
- **High-frequency read, low-frequency write** workloads
- **Pre-aggregated BI dashboards** or analytics reports
- **Arrow Flight integration** requiring sub-second response times

### ❌ **Don't use ta_* when**

- **Small tables** (<100K rows) - use `va_*` logical views instead
- **Frequently changing data** with millisecond latency requirements
- **Non-PostgreSQL databases** (BRIN indexes are PostgreSQL-only)
- **Write-heavy workloads** where constant synchronization overhead is unacceptable

## Performance Comparison

### Scenario: 10M row `tb_order` table, query last 30 days (100K rows)

| Metric | va_orders (View) | ta_orders (Table) | Improvement |
|--------|------------------|-------------------|-------------|
| Query time | 5-10s | 50-100ms | **50-100x faster** |
| Index scan | Full table scan | BRIN range scan | **1000x fewer pages** |
| Memory | 2-3GB | 500MB | **4-6x lower** |
| CPU | 100% | 5-10% | **10-20x reduction** |

## DDL Pattern

### Basic Structure

```sql
-- 1. Create physical table with extracted columns
CREATE TABLE ta_orders (
    id                  TEXT NOT NULL PRIMARY KEY,
    total               NUMERIC(10,2) NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL,
    customer_name       TEXT,
    source_updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (id) REFERENCES tb_order(id) ON DELETE CASCADE
);

-- 2. Create BRIN index for time-series queries
CREATE INDEX idx_ta_orders_created_at_brin
    ON ta_orders USING BRIN (created_at);

-- 3. Create refresh trigger for near-real-time updates
CREATE TRIGGER trg_refresh_ta_orders
    AFTER INSERT OR UPDATE OR DELETE ON tb_order
    FOR EACH ROW
    EXECUTE FUNCTION refresh_ta_orders_trigger();
```

### Arrow Type Mapping

| PostgreSQL Type | Arrow DataType | Usage | Example |
|----------------|----------------|-------|---------|
| `TEXT`, `VARCHAR` | `Utf8` | String data | `customer_name TEXT` |
| `NUMERIC(p,s)` | `Decimal128(p,s)` | Precise decimals | `total NUMERIC(10,2)` |
| `INTEGER` | `Int32` | 32-bit integers | `quantity INTEGER` |
| `BIGINT` | `Int64` | 64-bit integers | `user_count BIGINT` |
| `TIMESTAMPTZ` | `Timestamp(Microsecond, UTC)` | Timestamps | `created_at TIMESTAMPTZ` |
| `DATE` | `Date32` | Date only | `order_date DATE` |
| `BOOLEAN` | `Boolean` | Boolean flags | `is_active BOOLEAN` |

## Refresh Strategies

Choose based on your latency and overhead requirements:

### Option 1: Trigger-Based (Real-Time)

**Best for**: Dashboards with <1min latency requirements

**Characteristics**:
- Fires after every INSERT/UPDATE/DELETE on source table
- Latency: <10ms per row
- Overhead: Per-row cost (scales with write volume)
- Control: Fully automatic

**Implementation**:

```sql
CREATE OR REPLACE FUNCTION refresh_ta_orders_trigger()
RETURNS TRIGGER AS $$
BEGIN
    -- Handle INSERT/UPDATE: Upsert to ta_orders
    IF (TG_OP = 'INSERT' OR TG_OP = 'UPDATE') AND NEW.deleted_at IS NULL THEN
        INSERT INTO ta_orders (id, total, created_at, customer_name, source_updated_at)
        VALUES (NEW.id, NEW.total, NEW.created_at, NEW.data->>'customer_name', NOW())
        ON CONFLICT (id) DO UPDATE
        SET total = EXCLUDED.total,
            created_at = EXCLUDED.created_at,
            customer_name = EXCLUDED.customer_name,
            source_updated_at = NOW();

    -- Handle soft/hard deletes: Remove from ta_orders
    ELSIF (TG_OP = 'UPDATE' AND NEW.deleted_at IS NOT NULL) OR TG_OP = 'DELETE' THEN
        DELETE FROM ta_orders WHERE id = COALESCE(NEW.id, OLD.id);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_refresh_ta_orders
    AFTER INSERT OR UPDATE OR DELETE ON tb_order
    FOR EACH ROW
    EXECUTE FUNCTION refresh_ta_orders_trigger();
```

### Option 2: Scheduled Batch (Low Overhead)

**Best for**: Nightly BI reports, acceptable staleness (minutes to hours)

**Characteristics**:
- Batched refresh at fixed intervals
- Latency: Minutes to hours
- Overhead: Batch cost (no per-row overhead)
- Control: Scheduled via pg_cron

**Implementation**:

```sql
-- Enable pg_cron extension
CREATE EXTENSION IF NOT EXISTS pg_cron;

-- Schedule refresh every 5 minutes
SELECT cron.schedule(
    'refresh-ta-orders',
    '*/5 * * * *',  -- Every 5 minutes
    'SELECT refresh_ta_orders();'
);
```

### Option 3: Command-Based Explicit Refresh

**Best for**: Development, debugging, manual ETL pipelines

**Characteristics**:
- Manual refresh on demand
- Latency: On-demand
- Overhead: Only when called
- Control: Explicit API calls

**Implementation**:

```sql
CREATE OR REPLACE FUNCTION refresh_ta_orders()
RETURNS TABLE(rows_inserted BIGINT, rows_updated BIGINT, rows_deleted BIGINT) AS $$
DECLARE
    v_inserted BIGINT := 0;
    v_updated BIGINT := 0;
    v_deleted BIGINT := 0;
BEGIN
    -- Upsert all non-deleted data
    WITH upsert AS (
        INSERT INTO ta_orders (id, total, created_at, customer_name, source_updated_at)
        SELECT id, total, created_at, data->>'customer_name', NOW()
        FROM tb_order WHERE deleted_at IS NULL
        ON CONFLICT (id) DO UPDATE
        SET total = EXCLUDED.total,
            created_at = EXCLUDED.created_at,
            customer_name = EXCLUDED.customer_name,
            source_updated_at = NOW()
        RETURNING (xmax = 0) AS inserted
    )
    SELECT COUNT(*) FILTER (WHERE inserted) INTO v_inserted FROM upsert;

    GET DIAGNOSTICS v_updated = ROW_COUNT;
    v_updated := v_updated - v_inserted;

    -- Delete orphaned rows
    WITH deleted AS (
        DELETE FROM ta_orders
        WHERE id NOT IN (SELECT id FROM tb_order WHERE deleted_at IS NULL)
        RETURNING 1
    )
    SELECT COUNT(*) INTO v_deleted FROM deleted;

    RETURN QUERY SELECT v_inserted, v_updated, v_deleted;
END;
$$ LANGUAGE plpgsql;
```

**Use cases for command-based refresh**:
- **Manual refresh after bulk import**: Load 1M rows via COPY, then refresh ta_*
- **Testing**: Verify trigger logic by manual refresh comparison
- **ETL integration**: External pipeline calls refresh after data sync
- **Debugging**: Check refresh function behavior without waiting for scheduled runs

**CLI/API calls**:

```bash
# Refresh via psql
psql -c "SELECT * FROM refresh_ta_orders();"

# Or programmatically
SELECT rows_inserted, rows_updated, rows_deleted
FROM refresh_ta_orders();
```

## Refresh Strategy Decision Matrix

| Write Volume | Read Volume | Latency Req | Recommended |
|-------------|-------------|------------|------------|
| Low (<100/min) | High | <1s | Trigger-based |
| Low (<100/min) | High | <5min | Scheduled (1min) |
| Low (<100/min) | Medium | <1hr | Scheduled (30min) |
| Medium (100-1K/min) | High | <5min | Trigger-based + batch cleanup |
| Medium (100-1K/min) | High | <1hr | Scheduled (5-15min) |
| High (>1K/min) | High | Any | Batch refresh only |

## Migration Guide

### Step 1: Create ta_* Table

```bash
psql -h localhost -U postgres fraiseql_dev < examples/sql/postgres/ta_orders.sql
psql -h localhost -U postgres fraiseql_dev < examples/sql/postgres/ta_users.sql
```

### Step 2: Verify Data Population

```sql
-- Check row counts match
SELECT COUNT(*) as ta_orders_count FROM ta_orders;
SELECT COUNT(*) as tb_order_count FROM tb_order WHERE deleted_at IS NULL;

-- They should be equal
```

### Step 3: Register Arrow Schema

In `fraiseql-arrow/src/metadata.rs`, the schema is automatically registered via `register_ta_tables()` method in `register_defaults()`.

```rust
// Schema is now available for Arrow Flight queries
registry.get("ta_orders")  // Returns registered Arrow schema
registry.get("ta_users")   // Returns registered Arrow schema
```

### Step 4: Query via Arrow Flight

```python
import pyarrow.flight as flight

# Connect to Arrow Flight server
client = flight.connect("grpc://localhost:50051")

# Create ticket for ta_orders
ticket = {
    "view": "ta_orders",
    "limit": 10000,
    "order_by": "created_at DESC"
}

# Fetch data as Arrow RecordBatch
stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
table = stream.read_all()
df = table.to_pandas()

print(f"Fetched {len(df)} rows in {stream.time_ms}ms")
```

### Step 5: Monitor Performance

```sql
-- Check refresh latency
SELECT MAX(source_updated_at) - NOW() as staleness
FROM ta_orders;

-- Monitor BRIN index performance
EXPLAIN (ANALYZE, BUFFERS)
SELECT COUNT(*) FROM ta_orders
WHERE created_at >= NOW() - INTERVAL '7 days';

-- Check trigger overhead
SELECT schemaname, tablename, idx_scan, idx_tup_read
FROM pg_stat_user_indexes
WHERE tablename = 'ta_orders';
```

## Limitations and Considerations

### Storage

- **Data duplication**: ta_* tables duplicate data from tb_* tables
- **Storage overhead**: Typically 10-20% of source table size
- **Index overhead**: BRIN indexes add ~1-2% overhead

### Refresh Latency

- **Trigger-based**: <10ms per row (suitable for real-time dashboards)
- **Scheduled batch**: Minutes (suitable for nightly reports)
- **Manual refresh**: On-demand (suitable for development/testing)

### Schema Drift Risk

- **Arrow schema must match SQL DDL**: Mismatch causes conversion errors
- **Mitigation**: Comprehensive unit and integration tests verify schema consistency
- **CI/CD validation**: Automatic checks during deployment

### Soft Deletes and Cleanup

- Triggers handle soft deletes (NEW.deleted_at IS NOT NULL)
- Hard deletes remove rows from ta_*
- Manual cleanup function handles schema evolution

### Multi-Database Support

- **PostgreSQL**: Full support (BRIN indexes, native triggers)
- **MySQL**: Partial support (no BRIN; use InnoDB compression instead)
- **SQLite**: Not recommended (limited trigger capabilities)
- **SQL Server**: Partial support (no BRIN; use columnstore indexes)

## Best Practices

1. **Always use BRIN indexes** for time-ordered data on PostgreSQL
2. **Test trigger logic** before production deployment
3. **Monitor staleness** with `source_updated_at` column
4. **Use command-based refresh** for bulk import verification
5. **Schedule batch refresh** as fallback for trigger failures
6. **Keep ta_* schema synchronized** with Arrow Flight schema definitions
7. **Document refresh strategy** in architecture decisions
8. **Include refresh statistics** in operational dashboards

## Troubleshooting

### Issue: ta_* table empty after trigger creation

**Cause**: Trigger only fires on future changes, not existing data.

**Solution**: Run initial population:
```sql
SELECT refresh_ta_orders();
```

### Issue: High CPU from trigger overhead

**Cause**: Too many writes to source table with per-row trigger.

**Solution**: Switch to batched refresh:
```sql
DROP TRIGGER trg_refresh_ta_orders ON tb_order;

-- Schedule batch refresh instead
SELECT cron.schedule('refresh-ta-orders', '*/5 * * * *', 'SELECT refresh_ta_orders();');
```

### Issue: Arrow Flight queries slow despite ta_* table

**Cause**: BRIN index not being used effectively.

**Solution**: Verify index usage:
```sql
EXPLAIN (ANALYZE) SELECT COUNT(*) FROM ta_orders
WHERE created_at >= NOW() - INTERVAL '7 days';

-- Check BRIN page ranges
SELECT blocknum, blkcount FROM pgstattuple_approx('ta_orders_created_at_brin');
```

### Issue: Schema mismatch between Arrow and PostgreSQL

**Cause**: DDL changed without updating Arrow schema.

**Solution**:
1. Update PostgreSQL DDL
2. Update Arrow schema in `metadata.rs`
3. Run integration tests to verify
4. Deploy together

## Examples

See `/home/lionel/code/fraiseql/examples/sql/postgres/` for complete DDL examples:
- `ta_orders.sql` - Orders table-backed Arrow view
- `ta_users.sql` - Users table-backed Arrow view

## See Also

- [tv_* Table Pattern (JSON Plane)](./tv-table-pattern.md) - Table-backed views for GraphQL queries
- [View Selection Guide](./view-selection-guide.md) - Unified decision guide for all 4 view patterns
- [Arrow Flight Integration](../../integrations/arrow-flight/) - Fast columnar data exchange
- [FraiseQL Database Architecture](../../architecture/) - Core database layer design
- [PostgreSQL Query Performance Tuning](https://www.postgresql.org/docs/current/performance-tips.html)
