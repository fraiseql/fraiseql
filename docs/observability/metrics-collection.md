# Metrics Collection Guide

## Overview

This document explains **what data** FraiseQL's observability system collects, **how** it's collected, and **why** each metric matters for schema optimization.

**Privacy First**: FraiseQL **never logs** query arguments, user data, or PII. Only structural patterns and timing information are collected.

---

## What is Collected

### Summary Table

| Metric Category | What | Why | Storage |
|----------------|------|-----|---------|
| **Query Timing** | Execution time, SQL generation time | Identify slow queries | `query_executions` table |
| **JSON Access Patterns** | Path, access type, frequency | Suggest denormalization | `json_accesses` / `jsonb_accesses` table |
| **Database Statistics** | Row counts, cardinality, index usage | Cost estimation | Read from system catalogs |
| **Cache Performance** | Hit rate, evictions | Optimize caching strategy | In-memory only |

---

## Metric 1: Query Execution Timing

### Purpose

Track per-query performance to identify:

- Slow queries (candidates for optimization)
- Performance trends over time
- Impact of schema changes

### Data Collected

```rust
pub struct QueryMetrics {
    pub query_name: String,           // e.g., "users", "salesByRegion"
    pub execution_time_ms: f64,       // Total end-to-end time
    pub sql_generation_time_ms: f64,  // Time to generate SQL from GraphQL
    pub db_round_trip_time_ms: f64,   // Database query + network time
    pub projection_time_ms: f64,      // Time to transform DB rows to GraphQL
    pub rows_returned: usize,         // Result set size
    pub cache_hit: bool,              // Was result cached?
    pub timestamp: SystemTime,        // When query executed
}
```text

### Example

**GraphQL Query**:

```graphql
query {
  users(where: { region: "US" }) {
    id
    name
    email
  }
}
```text

**Collected Metrics**:

```json
{
  "query_name": "users",
  "execution_time_ms": 1250.5,
  "sql_generation_time_ms": 2.3,
  "db_round_trip_time_ms": 1240.1,
  "projection_time_ms": 8.1,
  "rows_returned": 15234,
  "cache_hit": false,
  "timestamp": "2026-01-12T14:32:15Z"
}
```text

**Breakdown**:

- **Total time**: 1250.5ms (user-perceived latency)
- **SQL generation**: 2.3ms (compiler overhead, typically < 5ms)
- **Database query**: 1240.1ms (where optimization happens!)
- **Projection**: 8.1ms (transform rows to JSON, typically < 10ms)

### Storage (PostgreSQL)

```sql
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

CREATE INDEX idx_query_executions_name_time
    ON fraiseql_metrics.query_executions (query_name, executed_at DESC);
```text

### Storage (SQL Server)

```sql
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

CREATE NONCLUSTERED INDEX idx_query_executions_name_time
    ON fraiseql_metrics.query_executions (query_name, executed_at DESC);
```text

---

## Metric 2: JSON Path Access Patterns

### Purpose

Identify which JSON/JSONB paths are frequently accessed, enabling:

- Denormalization suggestions (JSON → direct column)
- Index recommendations on computed columns
- Query pattern analysis

### Data Collected

```rust
pub struct JsonAccessPattern {
    pub table_name: String,      // "tf_sales", "users"
    pub json_column: String,      // "dimensions", "metadata"
    pub path: String,             // "region", "location.city"
    pub access_type: JsonAccessType,
    pub query_name: String,       // Which query accessed this
    pub selectivity: f64,         // For filters: % of rows matched
    pub timestamp: SystemTime,
}

pub enum JsonAccessType {
    Filter,     // WHERE clause
    Sort,       // ORDER BY clause
    Project,    // SELECT clause
    Aggregate,  // GROUP BY or aggregate function
}
```text

### Example: PostgreSQL JSONB

**GraphQL Query**:

```graphql
query {
  sales(
    where: { dimensions: { region: "US" } }
    orderBy: { dimensions: { date: DESC } }
    groupBy: ["dimensions.category"]
  ) {
    revenue
    quantity
  }
}
```text

**Generated SQL**:

```sql
SELECT
    SUM(revenue) AS revenue,
    SUM(quantity) AS quantity,
    dimensions->>'category' AS category
FROM tf_sales
WHERE dimensions->>'region' = 'US'
ORDER BY (dimensions->>'date')::date DESC
GROUP BY dimensions->>'category'
```text

**Collected Patterns**:

```json
[
  {
    "table_name": "tf_sales",
    "json_column": "dimensions",
    "path": "region",
    "access_type": "Filter",
    "query_name": "sales",
    "selectivity": 0.15,
    "timestamp": "2026-01-12T14:32:15Z"
  },
  {
    "table_name": "tf_sales",
    "json_column": "dimensions",
    "path": "date",
    "access_type": "Sort",
    "query_name": "sales",
    "selectivity": null,
    "timestamp": "2026-01-12T14:32:15Z"
  },
  {
    "table_name": "tf_sales",
    "json_column": "dimensions",
    "path": "category",
    "access_type": "Aggregate",
    "query_name": "sales",
    "selectivity": null,
    "timestamp": "2026-01-12T14:32:15Z"
  }
]
```text

### Example: SQL Server JSON

**GraphQL Query**:

```graphql
query {
  users(where: { metadata: { country: "USA" } }) {
    id
    name
  }
}
```text

**Generated SQL**:

```sql
SELECT id, name
FROM users
WHERE JSON_VALUE(metadata, '$.country') = 'USA'
```text

**Collected Pattern**:

```json
{
  "table_name": "users",
  "json_column": "metadata",
  "path": "country",
  "access_type": "Filter",
  "query_name": "users",
  "selectivity": 0.32,
  "timestamp": "2026-01-12T14:35:20Z"
}
```text

### How Selectivity is Calculated

**Selectivity** = (Rows Matched) ÷ (Total Rows)

**Example**:

- Query filters on `region = 'US'`
- Returns 15,000 rows
- Table has 100,000 total rows
- **Selectivity** = 15,000 ÷ 100,000 = **0.15 (15%)**

**Why it matters**:

- **High selectivity** (1-20%): Good candidate for denormalization + index
- **Medium selectivity** (20-50%): Index may help
- **Low selectivity** (50-100%): Not worth indexing (most rows match)

### Pattern Detection Logic

#### PostgreSQL JSONB Operators

```rust
impl JsonPathParser for PostgresJsonPathParser {
    fn extract_paths(&self, sql: &str) -> Vec<JsonAccessPattern> {
        // Detect: dimensions->>'region'
        let text_extract = Regex::new(r"(\w+)->>'(\w+)'").unwrap();

        // Detect: dimensions->'location'
        let json_extract = Regex::new(r"(\w+)->'(\w+)'").unwrap();

        // Detect: dimensions#>>'{location,city}'
        let path_text = Regex::new(r"(\w+)#>>'\{([^}]+)\}'").unwrap();

        // Detect: dimensions#>'{location,city}'
        let path_json = Regex::new(r"(\w+)#>'\{([^}]+)\}'").unwrap();

        // ... parse and classify
    }
}
```text

#### SQL Server JSON Functions

```rust
impl JsonPathParser for SqlServerJsonPathParser {
    fn extract_paths(&self, sql: &str) -> Vec<JsonAccessPattern> {
        // Detect: JSON_VALUE(dimensions, '$.region')
        let json_value = Regex::new(
            r"JSON_VALUE\((\w+),\s*'\$\.([^']+)'\)"
        ).unwrap();

        // Detect: JSON_QUERY(dimensions, '$.location')
        let json_query = Regex::new(
            r"JSON_QUERY\((\w+),\s*'\$\.([^']+)'\)"
        ).unwrap();

        // Parse nested paths: $.location.city → "location.city"
        // ... parse and classify
    }
}
```text

### Storage (PostgreSQL)

```sql
CREATE TABLE fraiseql_metrics.jsonb_accesses (
    id BIGSERIAL PRIMARY KEY,
    table_name TEXT NOT NULL,
    jsonb_column TEXT NOT NULL,
    path TEXT NOT NULL,
    access_type TEXT NOT NULL,
    query_name TEXT NOT NULL,
    selectivity FLOAT,
    recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_jsonb_accesses_lookup
    ON fraiseql_metrics.jsonb_accesses (table_name, jsonb_column, path);
```text

### Storage (SQL Server)

```sql
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

CREATE NONCLUSTERED INDEX idx_json_accesses_lookup
    ON fraiseql_metrics.json_accesses (table_name, json_column, path);
```text

---

## Metric 3: Database Statistics

### Purpose

Gather **real database statistics** for accurate cost modeling:

- Table sizes and row counts
- Column cardinality (distinct values)
- Index usage and effectiveness
- Storage costs

**Note**: These are **read from system catalogs**, not stored in metrics tables.

### PostgreSQL Statistics

#### Table Statistics

**Query**:

```sql
SELECT
    schemaname,
    relname AS table_name,
    n_live_tup AS row_count,
    n_dead_tup AS dead_rows,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze,
    pg_total_relation_size(schemaname || '.' || relname) AS size_bytes
FROM pg_stat_user_tables
WHERE relname = $1;
```text

**Data Collected**:

```rust
pub struct TableStatistics {
    pub table_name: String,
    pub row_count: u64,               // Live rows
    pub dead_rows: u64,               // MVCC garbage (PostgreSQL-specific)
    pub last_vacuum: Option<SystemTime>,
    pub last_analyze: Option<SystemTime>,
    pub size_bytes: u64,              // Total table + TOAST + indexes
}
```text

**Example**:

```json
{
  "table_name": "tf_sales",
  "row_count": 1500000,
  "dead_rows": 50000,
  "last_vacuum": "2026-01-11T03:00:00Z",
  "last_analyze": "2026-01-12T01:00:00Z",
  "size_bytes": 524288000
}
```text

#### Column Statistics

**Query**:

```sql
SELECT
    attname AS column_name,
    n_distinct,          -- Cardinality estimate
    null_frac,           -- Fraction of NULLs
    avg_width,           -- Average column width (bytes)
    most_common_vals::text[] AS mcv
FROM pg_stats
WHERE tablename = $1 AND attname = $2;
```text

**Data Collected**:

```rust
pub struct ColumnStatistics {
    pub column_name: String,
    pub distinct_values: i64,         // -1 = unique, >0 = estimate, 0 = unknown
    pub null_fraction: f64,           // 0.0-1.0
    pub avg_width_bytes: i32,
    pub most_common_values: Vec<String>,
}
```text

**Example**:

```json
{
  "column_name": "region_id",
  "distinct_values": 52,
  "null_fraction": 0.002,
  "avg_width_bytes": 4,
  "most_common_values": ["US", "EU", "APAC"]
}
```text

#### Index Statistics

**Query**:

```sql
SELECT
    i.indexrelname AS index_name,
    array_agg(a.attname ORDER BY a.attnum) AS columns,
    i.idx_scan AS scans,
    i.idx_tup_read AS tuples_read,
    i.idx_tup_fetch AS tuples_fetched,
    pg_relation_size(i.indexrelid) AS size_bytes,
    ix.indisunique AS is_unique
FROM pg_stat_user_indexes i
JOIN pg_index ix ON i.indexrelid = ix.indexrelid
JOIN pg_attribute a ON a.attrelid = i.relid AND a.attnum = ANY(ix.indkey)
WHERE i.relname = $1
GROUP BY i.indexrelname, i.idx_scan, i.idx_tup_read,
         i.idx_tup_fetch, i.indexrelid, ix.indisunique;
```text

**Data Collected**:

```rust
pub struct IndexStatistics {
    pub index_name: String,
    pub columns: Vec<String>,
    pub scans: u64,                   // Times index was used
    pub tuples_read: u64,
    pub tuples_fetched: u64,
    pub size_bytes: u64,
    pub is_unique: bool,
}
```text

**Example**:

```json
{
  "index_name": "idx_tf_sales_region",
  "columns": ["region_id"],
  "scans": 125000,
  "tuples_read": 18750000,
  "tuples_fetched": 18750000,
  "size_bytes": 45875200,
  "is_unique": false
}
```text

---

### SQL Server Statistics

#### Table Statistics

**Query**:

```sql
SELECT
    t.name AS table_name,
    SUM(p.rows) AS row_count,
    SUM(a.total_pages) * 8 * 1024 AS size_bytes,
    MAX(s.last_updated) AS last_analyze
FROM sys.tables t
INNER JOIN sys.partitions p ON t.object_id = p.object_id
INNER JOIN sys.allocation_units a ON p.partition_id = a.container_id
LEFT JOIN sys.stats s ON t.object_id = s.object_id
WHERE t.name = @P1 AND p.index_id IN (0, 1)
GROUP BY t.name;
```text

**Data Collected**:

```rust
pub struct TableStatistics {
    pub table_name: String,
    pub row_count: u64,
    pub dead_rows: u64,               // Always 0 (SQL Server has no MVCC)
    pub last_vacuum: Option<SystemTime>,  // Always None
    pub last_analyze: Option<SystemTime>,
    pub size_bytes: u64,
}
```text

#### Column Statistics

**Query** (uses DBCC):

```sql
DBCC SHOW_STATISTICS('table_name', 'stat_name') WITH STAT_HEADER;
```text

**Data Collected**: Similar structure to PostgreSQL, but obtained through different queries.

#### Index Statistics

**Query**:

```sql
SELECT
    i.name AS index_name,
    STRING_AGG(c.name, ',') AS columns,
    s.user_seeks + s.user_scans + s.user_lookups AS scans,
    s.user_seeks AS tuples_read,
    s.user_lookups AS tuples_fetched,
    SUM(p.used_page_count) * 8 * 1024 AS size_bytes,
    i.is_unique
FROM sys.indexes i
INNER JOIN sys.dm_db_index_usage_stats s
    ON i.object_id = s.object_id AND i.index_id = s.index_id
INNER JOIN sys.index_columns ic
    ON i.object_id = ic.object_id AND i.index_id = ic.index_id
INNER JOIN sys.columns c
    ON ic.object_id = c.object_id AND ic.column_id = c.column_id
INNER JOIN sys.dm_db_partition_stats p
    ON i.object_id = p.object_id AND i.index_id = p.index_id
INNER JOIN sys.tables t ON i.object_id = t.object_id
WHERE t.name = @P1
GROUP BY i.name, s.user_seeks, s.user_scans, s.user_lookups,
         s.user_seeks, s.user_lookups, i.is_unique;
```text

---

## Metric 4: Cache Performance

### Purpose

Track query result caching effectiveness:

- Hit rate (% of queries served from cache)
- Cache size and memory usage
- Eviction rate

**Note**: Cache metrics are **in-memory only**, not persisted to database.

### Data Collected

```rust
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub size_bytes: usize,
    pub entry_count: usize,
}

impl CacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            return 0.0;
        }
        self.hits as f64 / total as f64
    }
}
```text

**Example**:

```json
{
  "hits": 45000,
  "misses": 5000,
  "evictions": 1200,
  "size_bytes": 104857600,
  "entry_count": 2500,
  "hit_rate": 0.9
}
```text

---

## Aggregated Statistics

### Query Stats Table

To avoid analyzing millions of raw metrics, FraiseQL periodically aggregates statistics:

**PostgreSQL**:

```sql
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
```text

**SQL Server**:

```sql
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
```text

**Aggregation Query** (runs hourly):

```sql
-- PostgreSQL
INSERT INTO fraiseql_metrics.query_stats
    (query_name, total_executions, total_time_ms, p50_ms, p95_ms, p99_ms, avg_rows, last_updated)
SELECT
    query_name,
    COUNT(*) AS total_executions,
    SUM(execution_time_ms) AS total_time_ms,
    PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY execution_time_ms) AS p50_ms,
    PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY execution_time_ms) AS p95_ms,
    PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY execution_time_ms) AS p99_ms,
    AVG(rows_returned) AS avg_rows,
    NOW() AS last_updated
FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '1 hour'
GROUP BY query_name
ON CONFLICT (query_name) DO UPDATE SET
    total_executions = query_stats.total_executions + EXCLUDED.total_executions,
    total_time_ms = query_stats.total_time_ms + EXCLUDED.total_time_ms,
    p50_ms = EXCLUDED.p50_ms,
    p95_ms = EXCLUDED.p95_ms,
    p99_ms = EXCLUDED.p99_ms,
    avg_rows = EXCLUDED.avg_rows,
    last_updated = EXCLUDED.last_updated;
```text

---

## What is NOT Collected

### Privacy and Security

FraiseQL **never logs**:

❌ **Query Arguments/Variables**

```graphql
query {
  user(id: "123e4567-e89b-12d3-a456-426614174000") {  # ❌ NOT logged
    name
  }
}
```text

❌ **Actual Data Values**

```sql
-- Query result:

-- id    | name          | email
-- ------|---------------|------------------
-- 1001  | Alice Johnson | alice@example.com  # ❌ NOT logged
```text

❌ **Personally Identifiable Information**

- User IDs
- Email addresses
- Names
- IP addresses
- Session tokens

❌ **Authentication Data**

- Passwords
- API keys
- OAuth tokens

### What IS Logged

✅ **Query Structure**

```graphql
query {
  user(id: $userId) {  # ✅ Structure logged, $userId value NOT logged
    name
  }
}
```text

✅ **Performance Metrics**

```json
{
  "query_name": "user",
  "execution_time_ms": 45.2,
  "rows_returned": 1
}
```text

✅ **JSON Path Patterns**

```json
{
  "path": "metadata.country",  # ✅ Path logged, NOT values
  "access_type": "Filter"
}
```text

---

## Collection Frequency

### Real-Time Metrics

Collected on **every sampled query** (e.g., 10% of queries):

- Query execution timing
- JSON path accesses
- Cache hits/misses

### Periodic Metrics

Collected **on-demand** during analysis:

- Database statistics (read from system catalogs)
- Index usage statistics

### Aggregated Metrics

Computed **hourly** from raw metrics:

- Query p50/p95/p99 percentiles
- Average execution times
- Total query counts

---

## Data Retention

### Default Retention Policy

**Raw metrics**: 30 days (configurable)

**Cleanup Query** (runs daily at 2 AM):

```sql
-- PostgreSQL
DELETE FROM fraiseql_metrics.query_executions
WHERE executed_at < NOW() - INTERVAL '30 days';

DELETE FROM fraiseql_metrics.jsonb_accesses
WHERE recorded_at < NOW() - INTERVAL '30 days';
```text

```sql
-- SQL Server
DELETE FROM fraiseql_metrics.query_executions
WHERE executed_at < DATEADD(day, -30, GETDATE());

DELETE FROM fraiseql_metrics.json_accesses
WHERE recorded_at < DATEADD(day, -30, GETDATE());
```text

**Aggregated stats**: Indefinite (small footprint)

### Storage Estimates

**Per day, 10% sampling, 1000 qps**:

| Metric Type | Rows Per Day | Storage (PostgreSQL) | Storage (SQL Server) |
|-------------|-------------|---------------------|---------------------|
| Query Executions | ~8.6M | ~500 MB | ~600 MB |
| JSON Accesses | ~2M | ~150 MB | ~180 MB |
| **Total** | **~10.6M** | **~650 MB** | **~780 MB** |

**30-day retention**: ~20 GB (PostgreSQL), ~24 GB (SQL Server)

---

## Sampling Strategy

### How Sampling Works

```rust
pub struct MetricsCollector {
    sample_rate: f64,  // 0.0-1.0
    rng: SmallRng,
}

impl MetricsCollector {
    pub fn should_sample(&mut self) -> bool {
        self.rng.gen::<f64>() < self.sample_rate
    }
}

// Usage in query execution
if self.metrics.should_sample() {
    self.metrics.record(query_metrics).await;
}
```text

### Sample Rate Guidelines

| Sample Rate | Queries/Day Sampled (at 1000 qps) | Use Case |
|-------------|-----------------------------------|----------|
| 1.0 (100%) | 86,400,000 | Development/testing only |
| 0.1 (10%) | 8,640,000 | Production default |
| 0.01 (1%) | 864,000 | High-traffic production |
| 0.001 (0.1%) | 86,400 | Extreme scale (> 100K qps) |

### Statistical Validity

**Question**: Is 10% sampling enough to detect patterns?

**Answer**: Yes! With 1000 qps and 10% sampling:

- **8.6M samples per day**
- **60M samples per week**

This is **statistically significant** for detecting:

- Queries with frequency > 1000/day (detected with 99.9% confidence)
- Slow queries (p95, p99 accurate within 5%)
- JSON access patterns (accurate frequency counts)

---

## Performance Impact

### Target Overhead

< 5% latency increase with default settings

### Measurement

**Baseline** (observability OFF):

```text
Query execution: 45.2ms
Throughput: 1000 qps
```text

**With Observability** (10% sampling):

```text
Query execution: 47.1ms  (+1.9ms, 4.2% increase)
Throughput: 980 qps  (2% decrease)
```text

### Overhead Breakdown

| Operation | Time (per sample) | Impact at 10% sampling |
|-----------|-------------------|------------------------|
| Timing measurement | 0.5µs | Negligible |
| JSON path parsing | 50µs | 5µs per query |
| Buffer write | 100µs | 10µs per query |
| Database flush (batched) | 5ms / 100 queries | 5µs per query |
| **Total** | **~150µs** | **~20µs per query** |

**Conclusion**: Overhead is dominated by **database flush** (batched), keeping impact minimal.

---

## Metrics Export

### HTTP Endpoint

Export metrics as JSON for offline analysis:

```bash
curl http://localhost:8080/metrics/export > metrics.json
```text

**Response Format**:

```json
{
  "version": "1.0",
  "exported_at": "2026-01-12T16:30:00Z",
  "window": {
    "start": "2026-01-05T00:00:00Z",
    "end": "2026-01-12T16:30:00Z"
  },
  "query_stats": [
    {
      "query_name": "users",
      "count": 15234,
      "avg_time_ms": 45.3,
      "p50_ms": 38.1,
      "p95_ms": 120.5,
      "p99_ms": 250.0,
      "avg_rows": 1523
    }
  ],
  "json_patterns": [
    {
      "table": "tf_sales",
      "json_column": "dimensions",
      "path": "region",
      "access_type": "Filter",
      "frequency": 8500,
      "selectivity": 0.15
    }
  ],
  "cache_metrics": {
    "hits": 45000,
    "misses": 5000,
    "hit_rate": 0.9
  }
}
```text

### CLI Export

```bash
# Export from database
fraiseql-cli export-metrics \
  --database postgres://... \
  --output metrics.json \
  --window 7d
```text

---

## Monitoring Metrics Collection

### Health Checks

**Check if metrics are being collected**:

```sql
-- PostgreSQL
SELECT
    COUNT(*) AS metrics_today,
    MIN(executed_at) AS first_metric,
    MAX(executed_at) AS last_metric
FROM fraiseql_metrics.query_executions
WHERE executed_at > NOW() - INTERVAL '1 day';
```text

```sql
-- SQL Server
SELECT
    COUNT(*) AS metrics_today,
    MIN(executed_at) AS first_metric,
    MAX(executed_at) AS last_metric
FROM fraiseql_metrics.query_executions
WHERE executed_at > DATEADD(day, -1, GETDATE());
```text

**Expected Result** (10% sampling, 1000 qps):

```text
metrics_today: ~8,640,000
first_metric:  2026-01-12 00:00:00
last_metric:   2026-01-12 23:59:58
```text

### Alert Thresholds

Set up alerts for:

1. **No metrics collected** (1 hour):

   ```sql
   SELECT MAX(executed_at) < NOW() - INTERVAL '1 hour'
   FROM fraiseql_metrics.query_executions;
   ```text

2. **Metrics lag** (> 5 minutes behind):

   ```sql
   SELECT MAX(executed_at) < NOW() - INTERVAL '5 minutes'
   FROM fraiseql_metrics.query_executions;
   ```text

3. **Low sample rate** (< expected):

   ```sql
   SELECT COUNT(*) < 7200000  -- Expected: 8.6M per day at 10% sampling
   FROM fraiseql_metrics.query_executions
   WHERE executed_at > NOW() - INTERVAL '1 day';
   ```text

---

## Next Steps

- **[Operations Guide](./migration-workflow.md)** - Apply optimizations
- **[Optimization Suggestions](./optimization-suggestions.md)** - Performance tuning
- **[Troubleshooting](./troubleshooting.md)** - Common issues

---

*Last updated: 2026-01-12*
