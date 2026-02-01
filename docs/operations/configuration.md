# Observability Configuration Reference

## Overview

This document provides complete configuration reference for FraiseQL's observability system. All settings are **opt-in** and carefully tuned for production safety.

---

## Quick Start

### Minimal Configuration

Enable observability with defaults:

```bash
# Environment variable
export FRAISEQL_OBSERVABILITY_ENABLED=true
export FRAISEQL_DATABASE_URL=postgres://user:pass@localhost/mydb
```

Or in `fraiseql.toml`:

```toml
[observability]
enabled = true

[database]
url = "postgres://user:pass@localhost/mydb"
```

That's it! The system will use conservative defaults.

---

## Configuration Methods

FraiseQL supports three configuration methods (in order of precedence):

1. **Environment Variables** (highest priority)
2. **Configuration File** (`fraiseql.toml`)
3. **Default Values** (lowest priority)

### Environment Variables

```bash
# Core settings
export FRAISEQL_OBSERVABILITY_ENABLED=true
export FRAISEQL_OBSERVABILITY_SAMPLE_RATE=0.1
export FRAISEQL_METRICS_DATABASE_URL=postgres://...

# Retention settings
export FRAISEQL_METRICS_RETENTION_DAYS=30
export FRAISEQL_METRICS_BUFFER_SIZE=100

# Performance tuning
export FRAISEQL_METRICS_FLUSH_INTERVAL_SECS=60
export FRAISEQL_METRICS_BATCH_SIZE=100
```

### Configuration File

Create `fraiseql.toml` in your project root:

```toml
[observability]
enabled = true
sample_rate = 0.1
retention_days = 30

[observability.metrics]
buffer_size = 100
flush_interval_secs = 60
batch_size = 100

[observability.database]
# Optional: Separate database for metrics (recommended for production)
url = "postgres://metrics:pass@metrics-db:5432/fraiseql_metrics"
pool_size = 10
timeout_secs = 30

[observability.analysis]
# Default thresholds for analyze command
min_frequency = 1000
min_speedup = 5.0
min_selectivity = 0.1
```

---

## Core Configuration

### `observability.enabled`

**Type**: `boolean`
**Default**: `false`
**Environment**: `FRAISEQL_OBSERVABILITY_ENABLED`

Enable or disable observability system.

**Important**: Observability is **opt-in** for production safety. You must explicitly enable it.

```toml
[observability]
enabled = true
```

```bash
export FRAISEQL_OBSERVABILITY_ENABLED=true
```

---

### `observability.sample_rate`

**Type**: `float` (0.0 - 1.0)
**Default**: `0.1` (10%)
**Environment**: `FRAISEQL_OBSERVABILITY_SAMPLE_RATE`

Percentage of queries to collect metrics for.

**Guidelines**:

| Traffic Level | Recommended Rate | Expected Overhead |
|--------------|------------------|-------------------|
| Low (< 100 qps) | 1.0 (100%) | < 2% |
| Medium (100-1000 qps) | 0.1 (10%) | < 5% |
| High (> 1000 qps) | 0.01 (1%) | < 1% |

**Example**:

```toml
[observability]
# Sample 1% of queries in high-traffic production
sample_rate = 0.01
```

**Note**: Even at 1% sampling, patterns are reliably detected with sufficient traffic.

---

### `observability.retention_days`

**Type**: `integer`
**Default**: `30`
**Environment**: `FRAISEQL_METRICS_RETENTION_DAYS`

How long to keep metrics data before automatic cleanup.

```toml
[observability]
retention_days = 30  # Keep metrics for 30 days
```

**Storage Estimates** (per day, 10% sampling, 1000 qps):

| Database Size | Metrics Storage Per Day |
|--------------|------------------------|
| Small (< 10 tables) | ~50 MB |
| Medium (10-50 tables) | ~200 MB |
| Large (> 50 tables) | ~500 MB |

**Cleanup Query** (runs daily):

```sql
-- PostgreSQL
DELETE FROM fraiseql_metrics.query_executions
WHERE executed_at < NOW() - INTERVAL '30 days';

-- SQL Server
DELETE FROM fraiseql_metrics.query_executions
WHERE executed_at < DATEADD(day, -30, GETDATE());
```

---

## Metrics Collection

### `observability.metrics.buffer_size`

**Type**: `integer`
**Default**: `100`
**Environment**: `FRAISEQL_METRICS_BUFFER_SIZE`

In-memory buffer size before flushing to database.

```toml
[observability.metrics]
buffer_size = 100  # Flush after 100 queries
```

**Trade-offs**:

- **Smaller buffer** (50): More frequent writes, less memory, higher DB load
- **Larger buffer** (500): Fewer writes, more memory, risk of data loss on crash

**Recommendation**: Use default 100 for most cases.

---

### `observability.metrics.flush_interval_secs`

**Type**: `integer`
**Default**: `60`
**Environment**: `FRAISEQL_METRICS_FLUSH_INTERVAL_SECS`

Maximum seconds to wait before flushing buffer (even if not full).

```toml
[observability.metrics]
flush_interval_secs = 60  # Flush at least every minute
```

**Why needed**: Ensures metrics aren't delayed indefinitely during low traffic.

---

### `observability.metrics.batch_size`

**Type**: `integer`
**Default**: `100`
**Environment**: `FRAISEQL_METRICS_BATCH_SIZE`

Number of metrics to insert in a single database transaction.

```toml
[observability.metrics]
batch_size = 100
```

**Performance Impact**:

| Batch Size | Inserts/sec | Latency | Transaction Log |
|-----------|-------------|---------|-----------------|
| 1 | 100 | High | Large |
| 100 | 10,000 | Low | Small |
| 1000 | 50,000 | Very Low | Tiny |

**Recommendation**: 100-500 for balanced performance.

---

## Database Configuration

### `observability.database.url`

**Type**: `string`
**Default**: Same as main database
**Environment**: `FRAISEQL_METRICS_DATABASE_URL`

Database connection string for metrics storage.

**Options**:

1. **Same database as application** (simple):

   ```toml
   # Omit this setting to use main database
   ```

2. **Separate database on same server** (recommended):

   ```toml
   [observability.database]
   url = "postgres://app:pass@localhost:5432/fraiseql_metrics"
   ```

3. **Separate metrics server** (production best practice):

   ```toml
   [observability.database]
   url = "postgres://metrics:pass@metrics-db.internal:5432/metrics"
   ```

**Benefits of Separate Database**:

- ✅ Isolation: Metrics don't impact application database
- ✅ Scaling: Scale metrics storage independently
- ✅ Retention: Different backup/retention policies
- ✅ Security: Restricted access to metrics

---

### `observability.database.pool_size`

**Type**: `integer`
**Default**: `10`
**Environment**: `FRAISEQL_METRICS_DB_POOL_SIZE`

Connection pool size for metrics database.

```toml
[observability.database]
pool_size = 10
```

**Guidelines**:

| Traffic | Pool Size | Reasoning |
|---------|-----------|-----------|
| Low (< 100 qps) | 5 | Minimal connections needed |
| Medium (100-1000 qps) | 10 | Default works well |
| High (> 1000 qps) | 20 | More concurrent writes |

---

### `observability.database.timeout_secs`

**Type**: `integer`
**Default**: `30`
**Environment**: `FRAISEQL_METRICS_DB_TIMEOUT_SECS`

Query timeout for metrics writes (seconds).

```toml
[observability.database]
timeout_secs = 30
```

**Important**: If metrics writes timeout, they're dropped (doesn't block application queries).

---

## Analysis Configuration

These settings control the `fraiseql-cli analyze` command behavior.

### `observability.analysis.min_frequency`

**Type**: `integer`
**Default**: `1000`
**CLI Flag**: `--min-frequency`

Minimum queries per day to suggest optimization.

```toml
[observability.analysis]
min_frequency = 1000
```

```bash
fraiseql-cli analyze --min-frequency 500  # Override default
```

**Guidelines**:

| Threshold | Result | Use Case |
|-----------|--------|----------|
| 100 | Many suggestions | Development/testing |
| 1000 (default) | High-impact only | Production |
| 5000 | Critical paths only | High-traffic apps |

---

### `observability.analysis.min_speedup`

**Type**: `float`
**Default**: `5.0`
**CLI Flag**: `--min-speedup`

Minimum speedup factor (e.g., 5.0 = 5x faster) to suggest optimization.

```toml
[observability.analysis]
min_speedup = 5.0
```

```bash
fraiseql-cli analyze --min-speedup 3.0  # Lower threshold
```

**Guidelines**:

| Threshold | Result | Trade-off |
|-----------|--------|-----------|
| 2.0 | Many suggestions | More noise, smaller gains |
| 5.0 (default) | Clear wins | Conservative, high-impact |
| 10.0 | Only huge gains | May miss good optimizations |

---

### `observability.analysis.min_selectivity`

**Type**: `float` (0.0 - 1.0)
**Default**: `0.1` (10%)
**CLI Flag**: `--min-selectivity`

Minimum filter selectivity (% of rows filtered) for denormalization suggestions.

```toml
[observability.analysis]
min_selectivity = 0.1  # 10% of rows filtered
```

**Example**:

```sql
-- High selectivity (15% of rows match) → Suggest denormalization
WHERE JSON_VALUE(dimensions, '$.region') = 'US'  -- Returns 15,000 / 100,000 rows

-- Low selectivity (90% match) → Don't suggest (most rows match anyway)
WHERE JSON_VALUE(dimensions, '$.active') = 'true'  -- Returns 90,000 / 100,000 rows
```

**Guidelines**:

| Threshold | Meaning | Use Case |
|-----------|---------|----------|
| 0.01 (1%) | Very selective | Rare filters (e.g., VIP users) |
| 0.1 (10%) | Moderately selective | Default, balanced |
| 0.5 (50%) | Low selectivity | Broad filters |

---

### `observability.analysis.window`

**Type**: `string` (duration)
**Default**: `"7d"`
**CLI Flag**: `--window`

Time window for analysis.

```toml
[observability.analysis]
window = "7d"  # Last 7 days
```

```bash
fraiseql-cli analyze --window 30d  # Last 30 days
```

**Supported Formats**:

- `1d` - 1 day
- `7d` - 7 days (default)
- `30d` - 30 days
- `90d` - 90 days

**Guidelines**:

| Window | Use Case | Trade-off |
|--------|----------|-----------|
| 1d | Quick check | May miss weekly patterns |
| 7d (default) | Weekly patterns | Good balance |
| 30d | Monthly trends | Includes seasonal traffic |
| 90d | Long-term patterns | May include stale data |

---

## Privacy and Security

### `observability.privacy.collect_arguments`

**Type**: `boolean`
**Default**: `false` (NEVER enabled)
**Environment**: N/A (hardcoded to false)

Whether to collect query arguments (variables).

**IMPORTANT**: This is **always false** and cannot be enabled. FraiseQL **never logs**:

- ❌ Query arguments/variables
- ❌ User IDs or identifiers
- ❌ Personally Identifiable Information (PII)
- ❌ Actual data values

**What IS collected**:

- ✅ Query structure (operation name)
- ✅ Execution timing
- ✅ JSON paths accessed (e.g., "dimensions.region")
- ✅ Result set sizes
- ✅ Cache hit/miss

---

### `observability.security.metrics_table_permissions`

**Recommendation**: Restrict metrics tables to observability service only.

**PostgreSQL**:

```sql
-- Create dedicated metrics user
CREATE USER fraiseql_metrics WITH PASSWORD 'secure_password';

-- Grant schema access
GRANT USAGE ON SCHEMA fraiseql_metrics TO fraiseql_metrics;

-- Grant table permissions (INSERT only for collection)
GRANT INSERT ON fraiseql_metrics.query_executions TO fraiseql_metrics;
GRANT INSERT ON fraiseql_metrics.jsonb_accesses TO fraiseql_metrics;

-- Analysis user needs SELECT
CREATE USER fraiseql_analyst WITH PASSWORD 'analyst_password';
GRANT SELECT ON ALL TABLES IN SCHEMA fraiseql_metrics TO fraiseql_analyst;
```

**SQL Server**:

```sql
-- Create dedicated login and user
CREATE LOGIN fraiseql_metrics WITH PASSWORD = 'secure_password';
CREATE USER fraiseql_metrics FOR LOGIN fraiseql_metrics;

-- Grant schema access
GRANT SELECT, INSERT ON SCHEMA::fraiseql_metrics TO fraiseql_metrics;

-- Analysis user
CREATE LOGIN fraiseql_analyst WITH PASSWORD = 'analyst_password';
CREATE USER fraiseql_analyst FOR LOGIN fraiseql_analyst;
GRANT SELECT ON SCHEMA::fraiseql_metrics TO fraiseql_analyst;
```

---

## Production Configuration Examples

### Small Application (< 100 qps)

```toml
[observability]
enabled = true
sample_rate = 1.0  # 100% sampling (low traffic)
retention_days = 30

[observability.metrics]
buffer_size = 50
flush_interval_secs = 30
batch_size = 50

[observability.analysis]
min_frequency = 100  # Lower threshold (less traffic)
min_speedup = 3.0
min_selectivity = 0.1
```

---

### Medium Application (100-1000 qps)

```toml
[observability]
enabled = true
sample_rate = 0.1  # 10% sampling (default)
retention_days = 30

[observability.metrics]
buffer_size = 100
flush_interval_secs = 60
batch_size = 100

[observability.database]
# Separate database recommended
url = "postgres://metrics:pass@metrics-db:5432/fraiseql_metrics"
pool_size = 10

[observability.analysis]
min_frequency = 1000  # Default
min_speedup = 5.0
min_selectivity = 0.1
```

---

### Large Application (> 1000 qps)

```toml
[observability]
enabled = true
sample_rate = 0.01  # 1% sampling (high traffic)
retention_days = 14  # Shorter retention (lots of data)

[observability.metrics]
buffer_size = 200
flush_interval_secs = 60
batch_size = 500  # Larger batches for efficiency

[observability.database]
# Separate metrics cluster
url = "postgres://metrics:pass@metrics-cluster.internal:5432/metrics"
pool_size = 20
timeout_secs = 30

[observability.analysis]
min_frequency = 5000  # High threshold
min_speedup = 10.0  # Only huge wins
min_selectivity = 0.05
window = "30d"  # Longer window to capture patterns at 1% sampling
```

---

### Multi-Database Configuration

**PostgreSQL Primary + SQL Server Secondary**:

```toml
[database]
url = "postgres://app:pass@localhost:5432/myapp"

[database.secondary]
sql_server_url = "sqlserver://app:pass@sqlserver:1433/myapp"

[observability]
enabled = true
sample_rate = 0.1

# Metrics always go to PostgreSQL (best pg_stats support)
[observability.database]
url = "postgres://metrics:pass@metrics-db:5432/metrics"

[observability.analysis]
# Analyzer detects database type from query patterns
# Generates appropriate SQL for each database
```

---

## Database Schema Setup

### PostgreSQL

The observability system automatically creates these tables on first run:

```sql
-- Create schema
CREATE SCHEMA IF NOT EXISTS fraiseql_metrics;

-- Query execution metrics
CREATE TABLE fraiseql_metrics.query_executions (
    id BIGSERIAL PRIMARY KEY,
    query_name TEXT NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    sql_generation_time_ms FLOAT NOT NULL,
    db_round_trip_time_ms FLOAT NOT NULL,
    projection_time_ms FLOAT NOT NULL,
    rows_returned INTEGER NOT NULL,
    cache_hit BOOLEAN NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for time-series queries
CREATE INDEX idx_query_executions_name_time
    ON fraiseql_metrics.query_executions (query_name, executed_at DESC);

-- Index for analysis queries
CREATE INDEX idx_query_executions_time
    ON fraiseql_metrics.query_executions (executed_at DESC);

-- JSONB path access patterns
CREATE TABLE fraiseql_metrics.jsonb_accesses (
    id BIGSERIAL PRIMARY KEY,
    table_name TEXT NOT NULL,
    jsonb_column TEXT NOT NULL,
    path TEXT NOT NULL,
    access_type TEXT NOT NULL,  -- 'Filter', 'Sort', 'Project', 'Aggregate'
    query_name TEXT NOT NULL,
    selectivity FLOAT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for pattern analysis
CREATE INDEX idx_jsonb_accesses_lookup
    ON fraiseql_metrics.jsonb_accesses (table_name, jsonb_column, path);

-- Index for frequency counting
CREATE INDEX idx_jsonb_accesses_time
    ON fraiseql_metrics.jsonb_accesses (recorded_at DESC);

-- Aggregated statistics (updated periodically)
CREATE TABLE fraiseql_metrics.query_stats (
    query_name TEXT PRIMARY KEY,
    total_executions BIGINT NOT NULL,
    total_time_ms FLOAT NOT NULL,
    p50_ms FLOAT,
    p95_ms FLOAT,
    p99_ms FLOAT,
    avg_rows FLOAT,
    last_updated TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

### SQL Server

```sql
-- Create schema
CREATE SCHEMA fraiseql_metrics;
GO

-- Query execution metrics
CREATE TABLE fraiseql_metrics.query_executions (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    query_name NVARCHAR(256) NOT NULL,
    execution_time_ms FLOAT NOT NULL,
    sql_generation_time_ms FLOAT NOT NULL,
    db_round_trip_time_ms FLOAT NOT NULL,
    projection_time_ms FLOAT NOT NULL,
    rows_returned INT NOT NULL,
    cache_hit BIT NOT NULL,
    executed_at DATETIME2 NOT NULL DEFAULT GETDATE()
);

-- Indexes
CREATE NONCLUSTERED INDEX idx_query_executions_name_time
    ON fraiseql_metrics.query_executions (query_name, executed_at DESC);

CREATE NONCLUSTERED INDEX idx_query_executions_time
    ON fraiseql_metrics.query_executions (executed_at DESC);

-- JSON path access patterns
CREATE TABLE fraiseql_metrics.json_accesses (
    id BIGINT IDENTITY(1,1) PRIMARY KEY,
    table_name NVARCHAR(128) NOT NULL,
    json_column NVARCHAR(128) NOT NULL,
    path NVARCHAR(512) NOT NULL,
    access_type NVARCHAR(50) NOT NULL,
    query_name NVARCHAR(256) NOT NULL,
    selectivity FLOAT,
    recorded_at DATETIME2 NOT NULL DEFAULT GETDATE()
);

-- Indexes
CREATE NONCLUSTERED INDEX idx_json_accesses_lookup
    ON fraiseql_metrics.json_accesses (table_name, json_column, path);

CREATE NONCLUSTERED INDEX idx_json_accesses_time
    ON fraiseql_metrics.json_accesses (recorded_at DESC);

-- Aggregated statistics
CREATE TABLE fraiseql_metrics.query_stats (
    query_name NVARCHAR(256) PRIMARY KEY,
    total_executions BIGINT NOT NULL,
    total_time_ms FLOAT NOT NULL,
    p50_ms FLOAT,
    p95_ms FLOAT,
    p99_ms FLOAT,
    avg_rows FLOAT,
    last_updated DATETIME2 NOT NULL DEFAULT GETDATE()
);
GO
```

---

## Manual Schema Creation

If auto-creation is disabled or fails:

**PostgreSQL**:

```bash
psql -U fraiseql_metrics -d fraiseql_metrics -f schema/postgres_metrics.sql
```

**SQL Server**:

```bash
sqlcmd -S localhost -U fraiseql_metrics -P password -i schema/sqlserver_metrics.sql
```

---

## Performance Tuning

### High-Traffic Optimization

For applications with > 10,000 qps:

```toml
[observability]
sample_rate = 0.001  # 0.1% sampling

[observability.metrics]
buffer_size = 1000
batch_size = 1000
flush_interval_secs = 120  # Flush every 2 minutes

[observability.database]
pool_size = 50
timeout_secs = 10  # Fail fast
```

### Low-Latency Requirements

For applications where every millisecond counts:

```toml
[observability]
sample_rate = 0.01  # 1% sampling

[observability.metrics]
# Async writes with minimal blocking
buffer_size = 500
flush_interval_secs = 30

[observability.database]
# Separate metrics database is critical
url = "postgres://metrics:pass@async-metrics-db:5432/metrics"
pool_size = 20
timeout_secs = 5  # Drop metrics if DB is slow
```

---

## Monitoring Observability System

### Metrics to Track

**Application-Level** (via logs):

```rust
// Log metrics collection health
if metrics_buffer_full {
    warn!("Metrics buffer full, dropping oldest entries");
}

if metrics_write_timeout {
    warn!("Metrics write timed out after {}ms", timeout_ms);
}

// Periodic stats
info!(
    "Metrics: collected={}, flushed={}, dropped={}, buffer_size={}",
    collected_count, flushed_count, dropped_count, buffer_len
);
```

**Database-Level** (query metrics tables):

```sql
-- PostgreSQL: Metrics collection rate
SELECT
    DATE_TRUNC('hour', executed_at) AS hour,
    COUNT(*) AS metrics_collected
FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '24 hours'
GROUP BY hour
ORDER BY hour DESC;

-- SQL Server: Metrics collection rate
SELECT
    DATEPART(hour, executed_at) AS hour,
    COUNT(*) AS metrics_collected
FROM fraiseql_metrics.query_executions
WHERE executed_at > DATEADD(hour, -24, GETDATE())
GROUP BY DATEPART(hour, executed_at)
ORDER BY hour DESC;
```

---

## Troubleshooting Configuration

### Issue: Metrics Not Being Collected

**Check**:

1. Is observability enabled?

   ```bash
   echo $FRAISEQL_OBSERVABILITY_ENABLED
   # Should output: true
   ```

2. Is sample rate too low?

   ```bash
   echo $FRAISEQL_OBSERVABILITY_SAMPLE_RATE
   # Should be > 0.0
   ```

3. Check database connection:

   ```bash
   psql $FRAISEQL_METRICS_DATABASE_URL -c "SELECT 1"
   ```

4. Check application logs:

   ```
   grep "observability" app.log
   ```

---

### Issue: High Memory Usage

**Symptoms**: Application memory grows over time

**Cause**: Metrics buffer too large or not flushing

**Solution**:

```toml
[observability.metrics]
buffer_size = 50          # Reduce from 100
flush_interval_secs = 30  # Flush more frequently
```

---

### Issue: Database Connection Errors

**Symptoms**: "Failed to write metrics to database"

**Solutions**:

1. **Increase timeout**:

   ```toml
   [observability.database]
   timeout_secs = 60  # From 30
   ```

2. **Increase pool size**:

   ```toml
   [observability.database]
   pool_size = 20  # From 10
   ```

3. **Use separate database** (recommended):

   ```toml
   [observability.database]
   url = "postgres://metrics-only-db:5432/metrics"
   ```

---

### Issue: Analysis Shows No Suggestions

**Causes**:

1. **Insufficient data**: Run application for 24-48 hours
2. **Thresholds too high**: Lower them
3. **No JSON usage**: Observability focuses on JSON/JSONB optimization

**Solution**:

```bash
# Lower thresholds temporarily
fraiseql-cli analyze \
    --min-frequency 10 \
    --min-speedup 2.0 \
    --window 1d
```

---

## Environment Variables Reference

Complete list of all environment variables:

| Variable | Type | Default | Description |
|----------|------|---------|-------------|
| `FRAISEQL_OBSERVABILITY_ENABLED` | bool | false | Enable observability |
| `FRAISEQL_OBSERVABILITY_SAMPLE_RATE` | float | 0.1 | Sampling rate (0.0-1.0) |
| `FRAISEQL_METRICS_DATABASE_URL` | string | (main DB) | Metrics database URL |
| `FRAISEQL_METRICS_RETENTION_DAYS` | int | 30 | Metrics retention |
| `FRAISEQL_METRICS_BUFFER_SIZE` | int | 100 | In-memory buffer size |
| `FRAISEQL_METRICS_FLUSH_INTERVAL_SECS` | int | 60 | Flush interval |
| `FRAISEQL_METRICS_BATCH_SIZE` | int | 100 | Batch insert size |
| `FRAISEQL_METRICS_DB_POOL_SIZE` | int | 10 | Connection pool size |
| `FRAISEQL_METRICS_DB_TIMEOUT_SECS` | int | 30 | Query timeout |

---

## Configuration Validation

FraiseQL validates configuration on startup:

```rust
// Invalid configuration example
[observability]
enabled = true
sample_rate = 1.5  # ❌ ERROR: Must be 0.0-1.0

// Validation error:
// Error: Invalid observability configuration
//   - sample_rate must be between 0.0 and 1.0 (got 1.5)
```

**Validation Rules**:

- `sample_rate`: 0.0 ≤ x ≤ 1.0
- `retention_days`: > 0
- `buffer_size`: > 0
- `flush_interval_secs`: > 0
- `batch_size`: > 0
- `pool_size`: 1-100
- `timeout_secs`: > 0

---

## Next Steps

- **[Metrics Collection Guide](../observability/metrics-collection.md)** - What data is collected
- **[Troubleshooting](../observability/troubleshooting.md)** - Common issues and solutions
- **[Observability Guide](./observability.md)** - Complete observability setup

---

*Last updated: 2026-01-12*
