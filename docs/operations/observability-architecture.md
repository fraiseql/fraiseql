# Observability System Architecture

## Table of Contents

1. [Overview](#overview)
2. [System Architecture](#system-architecture)
3. [Multi-Database Support Strategy](#multi-database-support-strategy)
4. [Component 1: Metrics Collection Layer](#component-1-metrics-collection-layer)
5. [Component 2: Pattern Analysis Engine](#component-2-pattern-analysis-engine)
6. [Component 3: CLI Integration](#component-3-cli-integration)
7. [Performance Impact](#performance-impact)
8. [Design Decisions](#design-decisions)
9. [Testing Strategy](#testing-strategy)
10. [Future Enhancements](#future-enhancements)
11. [Summary](#summary)

---

## Overview

FraiseQL's observability system enables **runtime-informed schema optimization** by collecting query performance metrics, analyzing patterns, and suggesting database schema improvements.

**Key Innovation**: Move from static compile-time analysis to dynamic runtime analysis based on actual production workload.

---

## System Architecture

### High-Level Components

```text
┌─────────────────────────────────────────────────────────────────┐
│                     RUNTIME LAYER (Rust)                         │
│  ┌────────────────┐  ┌────────────────┐  ┌──────────────────┐  │
│  │ Query Timer    │  │ SQL Profiler   │  │ JSON Tracker     │  │
│  │ (execution)    │  │ (generation)   │  │ (path access)    │  │
│  └────────┬───────┘  └────────┬───────┘  └────────┬─────────┘  │
│           │                    │                    │             │
│           └────────────────────┼────────────────────┘             │
│                                ↓                                  │
│                    ┌───────────────────────┐                     │
│                    │  Metrics Aggregator   │                     │
│                    │  (batch writes)       │                     │
│                    └───────────┬───────────┘                     │
└────────────────────────────────┼─────────────────────────────────┘
                                 ↓
┌─────────────────────────────────────────────────────────────────┐
│                   METRICS STORAGE (PostgreSQL/SQL Server)        │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ fraiseql_metrics.query_executions                          │ │
│  │ - Query timing (execution, SQL gen, DB roundtrip)         │ │
│  │ - Result size, cache hits, timestamps                     │ │
│  └────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ fraiseql_metrics.jsonb_accesses (PostgreSQL)               │ │
│  │ fraiseql_metrics.json_accesses (SQL Server)                │ │
│  │ - JSON/JSONB path extraction patterns                      │ │
│  │ - Access type (Filter, Sort, Project, Aggregate)          │ │
│  │ - Frequency counts, selectivity estimates                 │ │
│  └────────────────────────────────────────────────────────────┘ │
└────────────────────────────────┬────────────────────────────────┘
                                 ↓
┌─────────────────────────────────────────────────────────────────┐
│                    ANALYSIS ENGINE (CLI)                         │
│                                                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Pattern Analyzer                                           │ │
│  │ - Detect frequent JSON path accesses                      │ │
│  │ - Identify high-selectivity filters                       │ │
│  │ - Find repeated aggregations on nested fields             │ │
│  │ - Discover expensive sorting operations                   │ │
│  └───────────────────────────┬────────────────────────────────┘ │
│                               ↓                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Database Introspection (Multi-Database)                    │ │
│  │ PostgreSQL: pg_stats, pg_stat_user_tables                 │ │
│  │ SQL Server: sys.stats, sys.dm_db_partition_stats          │ │
│  │ MySQL: information_schema.statistics                       │ │
│  │ SQLite: sqlite_stat1, ANALYZE results                     │ │
│  └───────────────────────────┬────────────────────────────────┘ │
│                               ↓                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Cost Estimation Model                                      │ │
│  │ - JSON extraction cost vs direct column lookup            │ │
│  │ - Speedup factor calculation (5-50x typical)              │ │
│  │ - Storage overhead estimation                             │ │
│  └───────────────────────────┬────────────────────────────────┘ │
│                               ↓                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Suggestion Generator                                       │ │
│  │ - Denormalization proposals (JSON → direct column)        │ │
│  │ - Index recommendations                                    │ │
│  │ - Materialized view suggestions (future)                  │ │
│  └───────────────────────────┬────────────────────────────────┘ │
│                               ↓                                   │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │ Migration Generator (Database-Specific SQL)                │ │
│  │ - ALTER TABLE, CREATE INDEX statements                    │ │
│  │ - Data backfill (UPDATE with JSON extraction)             │ │
│  │ - Rollback scripts                                         │ │
│  └────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```text

---

## Multi-Database Support Strategy

### Design Philosophy

FraiseQL supports **four database engines** with a unified observability interface:

| Database | JSON Support | Metrics Storage | Statistics API | Priority |
|----------|--------------|-----------------|----------------|----------|
| **PostgreSQL** | JSONB (binary) | ✅ Primary | pg_stats, pg_stat_* | 1 (Reference) |
| **SQL Server** | JSON (text) | ✅ Supported | sys.stats, DMVs | 2 (Full support) |
| **MySQL** | JSON (native) | ⚠️ Future | information_schema | 3 (Planned) |
| **SQLite** | JSON (text) | ⚠️ Future | sqlite_stat* | 4 (Planned) |

### Database-Agnostic Architecture

**Trait-Based Design**:

```rust
// Core abstraction for all databases
pub trait DatabaseStatistics: Send + Sync {
    /// Get table-level statistics (row count, size, last analyzed)
    async fn get_table_stats(&self, table: &str) -> Result<TableStatistics>;

    /// Get column-level statistics (cardinality, null fraction)
    async fn get_column_stats(&self, table: &str, column: &str)
        -> Result<ColumnStatistics>;

    /// Get index usage statistics
    async fn get_index_usage(&self, table: &str) -> Result<Vec<IndexStatistics>>;

    /// Parse JSON path extraction from SQL
    fn parse_json_path(&self, sql: &str) -> Vec<JsonAccessPattern>;

    /// Generate migration SQL (database-specific dialect)
    fn generate_migration(&self, suggestion: &DenormalizationSuggestion)
        -> Vec<String>;
}
```text

**Implementation Strategy**: Each database has its own implementation with identical interface.

---

## Component 1: Metrics Collection Layer

### 1.1 Query Execution Timing

**File**: `crates/FraiseQL-core/src/runtime/metrics.rs`

**Purpose**: Track per-query performance metrics in production

**Data Structure**:

```rust
pub struct QueryMetrics {
    pub query_name: String,
    pub execution_time_ms: f64,      // Total end-to-end time
    pub sql_generation_time_ms: f64,  // Time to generate SQL
    pub db_round_trip_time_ms: f64,   // Database query + network
    pub projection_time_ms: f64,      // Result transformation time
    pub rows_returned: usize,
    pub cache_hit: bool,
    pub timestamp: SystemTime,
}

pub struct MetricsCollector {
    /// Database connection for metrics persistence
    db: DatabasePool,

    /// In-memory buffer for batch writes (flush every 100 queries or 60s)
    buffer: Arc<Mutex<VecDeque<QueryMetrics>>>,

    /// Configuration (sampling rate, retention)
    config: ObservabilityConfig,
}
```text

**Sampling Strategy**:

- Default: **10% sampling** (configurable 0.1-1.0)
- Overhead target: **<5% latency increase**
- Trade-off: Less precision, more performance

**Batch Writing**:

```rust
impl MetricsCollector {
    async fn flush_to_database(&self) -> Result<()> {
        let mut buffer = self.buffer.lock().await;

        if buffer.is_empty() {
            return Ok(());
        }

        // Batch insert (100 queries at once)
        let batch: Vec<_> = buffer.drain(..).collect();

        let query = "
            INSERT INTO fraiseql_metrics.query_executions
            (query_name, execution_time_ms, sql_generation_time_ms,
             db_round_trip_time_ms, rows_returned, cache_hit, executed_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
        ";

        for metrics in batch {
            self.db.execute(query, &[
                &metrics.query_name,
                &metrics.execution_time_ms,
                // ... other fields
            ]).await?;
        }

        Ok(())
    }
}
```text

**Integration Points**:

```rust
// In crates/FraiseQL-core/src/runtime/executor.rs
pub async fn execute_query(&self, query: &str, variables: Variables)
    -> Result<ExecutionResult> {

    let start = Instant::now();

    // 1. SQL generation timing
    let sql_gen_start = Instant::now();
    let sql = self.planner.generate_sql(query, &variables)?;
    let sql_gen_time = sql_gen_start.elapsed();

    // 2. Database query timing
    let db_start = Instant::now();
    let rows = self.db.query(&sql).await?;
    let db_time = db_start.elapsed();

    // 3. Projection timing
    let proj_start = Instant::now();
    let result = self.projector.project(rows)?;
    let proj_time = proj_start.elapsed();

    let total_time = start.elapsed();

    // 4. Record metrics (if observability enabled)
    if self.config.observability_enabled {
        self.metrics.record(QueryMetrics {
            query_name: query.operation_name(),
            execution_time_ms: total_time.as_secs_f64() * 1000.0,
            sql_generation_time_ms: sql_gen_time.as_secs_f64() * 1000.0,
            db_round_trip_time_ms: db_time.as_secs_f64() * 1000.0,
            rows_returned: rows.len(),
            cache_hit: false,
            timestamp: SystemTime::now(),
        }).await;
    }

    Ok(result)
}
```text

---

### 1.2 JSON Path Access Tracking

**File**: `crates/FraiseQL-core/src/runtime/json_tracker.rs`

**Purpose**: Identify which JSON paths are frequently accessed in queries

**PostgreSQL JSONB vs SQL Server JSON**:

| Feature | PostgreSQL | SQL Server |
|---------|-----------|------------|
| **Storage** | Binary (JSONB) | Text (JSON) |
| **Extraction** | `->`, `->>`, `#>`, `#>>` | `JSON_VALUE()`, `JSON_QUERY()` |
| **Indexing** | GIN indexes on JSONB | Computed columns + indexes |
| **Path Syntax** | `dimensions->>'region'` | `JSON_VALUE(dimensions, '$.region')` |

**Pattern Detection**:

```rust
pub struct JsonAccessPattern {
    pub table_name: String,
    pub json_column: String,       // "dimensions"
    pub path: String,               // "region" or "location.city"
    pub access_type: JsonAccessType,
    pub frequency: u64,
    pub selectivity: f64,           // For filters: % of rows matched
}

pub enum JsonAccessType {
    Filter,     // WHERE clause
    Sort,       // ORDER BY clause
    Project,    // SELECT clause
    Aggregate,  // GROUP BY or aggregate function
}

/// Database-specific JSON path parser
pub trait JsonPathParser: Send + Sync {
    fn extract_paths(&self, sql: &str) -> Vec<JsonAccessPattern>;
}
```text

**PostgreSQL Implementation**:

```rust
pub struct PostgresJsonPathParser;

impl JsonPathParser for PostgresJsonPathParser {
    fn extract_paths(&self, sql: &str) -> Vec<JsonAccessPattern> {
        let mut patterns = Vec::new();

        // Regex patterns for JSONB operators
        let operators = [
            r"(\w+)->>'(\w+)'",              // Text extraction
            r"(\w+)->'(\w+)'",               // JSON extraction
            r"(\w+)#>'\{([^}]+)\}'",         // Path extraction
            r"(\w+)#>>'\{([^}]+)\}'",        // Path as text
        ];

        for op in operators {
            let re = Regex::new(op).unwrap();
            for cap in re.captures_iter(sql) {
                patterns.push(JsonAccessPattern {
                    table_name: infer_table(&cap[1]),
                    json_column: cap[1].to_string(),
                    path: cap[2].to_string(),
                    access_type: infer_access_type(sql, &cap[0]),
                    frequency: 1,
                    selectivity: 0.0,  // Computed later
                });
            }
        }

        patterns
    }
}
```text

**SQL Server Implementation**:

```rust
pub struct SqlServerJsonPathParser;

impl JsonPathParser for SqlServerJsonPathParser {
    fn extract_paths(&self, sql: &str) -> Vec<JsonAccessPattern> {
        let mut patterns = Vec::new();

        // Regex patterns for SQL Server JSON functions
        let json_value_re = Regex::new(
            r"JSON_VALUE\((\w+),\s*'\$\.([^']+)'\)"
        ).unwrap();

        let json_query_re = Regex::new(
            r"JSON_QUERY\((\w+),\s*'\$\.([^']+)'\)"
        ).unwrap();

        // Extract JSON_VALUE() calls
        for cap in json_value_re.captures_iter(sql) {
            patterns.push(JsonAccessPattern {
                table_name: infer_table(&cap[1]),
                json_column: cap[1].to_string(),
                path: cap[2].to_string(),  // "region" or "location.city"
                access_type: infer_access_type(sql, &cap[0]),
                frequency: 1,
                selectivity: 0.0,
            });
        }

        // Extract JSON_QUERY() calls (nested objects)
        for cap in json_query_re.captures_iter(sql) {
            patterns.push(JsonAccessPattern {
                table_name: infer_table(&cap[1]),
                json_column: cap[1].to_string(),
                path: cap[2].to_string(),
                access_type: JsonAccessType::Project,  // Usually projection
                frequency: 1,
                selectivity: 0.0,
            });
        }

        patterns
    }
}
```text

**Access Type Inference**:

```rust
fn infer_access_type(sql: &str, json_expr: &str) -> JsonAccessType {
    let sql_upper = sql.to_uppercase();
    let expr_pos = sql.find(json_expr).unwrap_or(0);

    // Check context around the JSON expression
    let context = &sql[expr_pos.saturating_sub(50)..expr_pos + 50.min(sql.len() - expr_pos)];
    let context_upper = context.to_uppercase();

    if context_upper.contains("WHERE") || context_upper.contains("AND") {
        JsonAccessType::Filter
    } else if context_upper.contains("ORDER BY") {
        JsonAccessType::Sort
    } else if context_upper.contains("GROUP BY") {
        JsonAccessType::Aggregate
    } else {
        JsonAccessType::Project
    }
}
```text

---

### 1.3 Database Statistics Collection

**File**: `crates/FraiseQL-core/src/db/introspection/statistics.rs`

**Purpose**: Gather real database statistics for accurate cost modeling

**Multi-Database Statistics Trait**:

```rust
pub trait DatabaseStatistics: Send + Sync {
    async fn get_table_stats(&self, table: &str) -> Result<TableStatistics>;
    async fn get_column_stats(&self, table: &str, column: &str)
        -> Result<ColumnStatistics>;
    async fn get_index_usage(&self, table: &str) -> Result<Vec<IndexStatistics>>;
}

pub struct TableStatistics {
    pub table_name: String,
    pub row_count: u64,
    pub dead_rows: u64,              // PostgreSQL-specific (garbage)
    pub last_vacuum: Option<SystemTime>,  // PostgreSQL-specific
    pub last_analyze: Option<SystemTime>,
    pub size_bytes: u64,
}

pub struct ColumnStatistics {
    pub column_name: String,
    pub distinct_values: i64,        // -1 = unknown, >0 = estimate
    pub null_fraction: f64,          // 0.0-1.0
    pub avg_width_bytes: i32,
    pub most_common_values: Vec<String>,  // Top N values
}

pub struct IndexStatistics {
    pub index_name: String,
    pub columns: Vec<String>,
    pub scans: u64,                  // Number of times used
    pub tuples_read: u64,
    pub tuples_fetched: u64,
    pub size_bytes: u64,
    pub is_unique: bool,
}
```text

---

#### PostgreSQL Implementation

**File**: `crates/FraiseQL-core/src/db/introspection/postgres_statistics.rs`

```rust
pub struct PostgresStatistics {
    pool: PgPool,
}

impl DatabaseStatistics for PostgresStatistics {
    async fn get_table_stats(&self, table: &str) -> Result<TableStatistics> {
        let row = sqlx::query!(
            r#"
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
            WHERE relname = $1
            "#,
            table
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TableStatistics {
            table_name: row.table_name,
            row_count: row.row_count.unwrap_or(0) as u64,
            dead_rows: row.dead_rows.unwrap_or(0) as u64,
            last_vacuum: row.last_vacuum.or(row.last_autovacuum),
            last_analyze: row.last_analyze.or(row.last_autoanalyze),
            size_bytes: row.size_bytes.unwrap_or(0) as u64,
        })
    }

    async fn get_column_stats(&self, table: &str, column: &str)
        -> Result<ColumnStatistics> {
        let row = sqlx::query!(
            r#"
            SELECT
                attname AS column_name,
                n_distinct,
                null_frac,
                avg_width,
                most_common_vals::text[] AS mcv
            FROM pg_stats
            WHERE tablename = $1 AND attname = $2
            "#,
            table,
            column
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(ColumnStatistics {
            column_name: row.column_name,
            distinct_values: row.n_distinct.unwrap_or(-1.0) as i64,
            null_fraction: row.null_frac.unwrap_or(0.0),
            avg_width_bytes: row.avg_width.unwrap_or(0),
            most_common_values: row.mcv.unwrap_or_default(),
        })
    }

    async fn get_index_usage(&self, table: &str) -> Result<Vec<IndexStatistics>> {
        let rows = sqlx::query!(
            r#"
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
            JOIN pg_attribute a ON a.attrelid = i.relid
                AND a.attnum = ANY(ix.indkey)
            WHERE i.relname = $1
            GROUP BY i.indexrelname, i.idx_scan, i.idx_tup_read,
                     i.idx_tup_fetch, i.indexrelid, ix.indisunique
            "#,
            table
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| IndexStatistics {
            index_name: row.index_name,
            columns: row.columns.unwrap_or_default(),
            scans: row.scans.unwrap_or(0) as u64,
            tuples_read: row.tuples_read.unwrap_or(0) as u64,
            tuples_fetched: row.tuples_fetched.unwrap_or(0) as u64,
            size_bytes: row.size_bytes.unwrap_or(0) as u64,
            is_unique: row.is_unique.unwrap_or(false),
        }).collect())
    }
}
```text

---

#### SQL Server Implementation

**File**: `crates/FraiseQL-core/src/db/introspection/sqlserver_statistics.rs`

**SQL Server System Views**:

- `sys.dm_db_partition_stats` - Table/index sizes and row counts
- `sys.dm_db_index_usage_stats` - Index usage statistics
- `sys.stats` + `sys.stats_columns` - Column statistics
- `DBCC SHOW_STATISTICS` - Detailed histogram data

```rust
pub struct SqlServerStatistics {
    pool: MssqlPool,
}

impl DatabaseStatistics for SqlServerStatistics {
    async fn get_table_stats(&self, table: &str) -> Result<TableStatistics> {
        let row = sqlx::query!(
            r#"
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
            GROUP BY t.name
            "#,
            table
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(TableStatistics {
            table_name: row.table_name,
            row_count: row.row_count.unwrap_or(0) as u64,
            dead_rows: 0,  // SQL Server doesn't have equivalent (no MVCC)
            last_vacuum: None,  // SQL Server doesn't vacuum
            last_analyze: row.last_analyze,
            size_bytes: row.size_bytes.unwrap_or(0) as u64,
        })
    }

    async fn get_column_stats(&self, table: &str, column: &str)
        -> Result<ColumnStatistics> {
        // SQL Server requires DBCC SHOW_STATISTICS for detailed stats
        let stats_query = format!(
            "DBCC SHOW_STATISTICS('{}', '{}') WITH STAT_HEADER",
            table, column
        );

        let row = sqlx::query(&stats_query)
            .fetch_one(&self.pool)
            .await?;

        // Parse DBCC output (varies by version)
        let distinct_values: i64 = row.try_get("Rows")?;
        let null_fraction: f64 = 0.0;  // Requires histogram analysis
        let avg_width: i32 = row.try_get("Average Key Length")?;

        Ok(ColumnStatistics {
            column_name: column.to_string(),
            distinct_values,
            null_fraction,
            avg_width_bytes: avg_width,
            most_common_values: vec![],  // Requires histogram parsing
        })
    }

    async fn get_index_usage(&self, table: &str) -> Result<Vec<IndexStatistics>> {
        let rows = sqlx::query!(
            r#"
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
                     s.user_seeks, s.user_lookups, i.is_unique
            "#,
            table
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| IndexStatistics {
            index_name: row.index_name,
            columns: row.columns
                .unwrap_or_default()
                .split(',')
                .map(String::from)
                .collect(),
            scans: row.scans.unwrap_or(0) as u64,
            tuples_read: row.tuples_read.unwrap_or(0) as u64,
            tuples_fetched: row.tuples_fetched.unwrap_or(0) as u64,
            size_bytes: row.size_bytes.unwrap_or(0) as u64,
            is_unique: row.is_unique,
        }).collect())
    }
}
```text

**Key Differences**:

| Feature | PostgreSQL | SQL Server |
|---------|-----------|------------|
| **Row Count** | `pg_stat_user_tables.n_live_tup` | `sys.partitions.rows` |
| **Dead Rows** | `n_dead_tup` (MVCC garbage) | N/A (no MVCC) |
| **Table Size** | `pg_total_relation_size()` | `sys.allocation_units.total_pages * 8KB` |
| **Column Stats** | `pg_stats` view | `DBCC SHOW_STATISTICS` command |
| **Index Usage** | `pg_stat_user_indexes` | `sys.dm_db_index_usage_stats` DMV |
| **Last Analyze** | `last_analyze` timestamp | `sys.stats.last_updated` |

---

## Component 2: Pattern Analysis Engine

### 2.1 Denormalization Analyzer

**File**: `crates/FraiseQL-cli/src/schema/observability_optimizer.rs`

**Purpose**: Analyze metrics and suggest schema optimizations

**Analysis Configuration**:

```rust
pub struct AnalysisConfig {
    /// Minimum queries per day to consider optimization
    pub min_frequency: u64,  // Default: 1000

    /// Minimum speedup factor to suggest denormalization
    pub min_speedup: f64,  // Default: 5.0

    /// Minimum selectivity for filter suggestions (0.0-1.0)
    pub min_selectivity: f64,  // Default: 0.1 (10% of rows filtered)

    /// Analysis time window
    pub window_days: u32,  // Default: 7

    /// Target database type (affects SQL generation)
    pub database_type: DatabaseType,
}

pub enum DatabaseType {
    PostgreSQL,
    SqlServer,
    MySQL,
    SQLite,
}
```text

**Core Analysis Algorithm**:

```rust
pub fn analyze_denormalization_opportunities(
    metrics: &MetricsCollector,
    json_patterns: &[JsonAccessPattern],
    db_stats: &impl DatabaseStatistics,
    config: &AnalysisConfig,
) -> Result<Vec<OptimizationSuggestion>> {

    let mut suggestions = Vec::new();

    // Group JSON accesses by (table, column, path)
    let grouped = group_patterns_by_path(json_patterns);

    for (key, patterns) in grouped {
        let total_frequency: u64 = patterns.iter().map(|p| p.frequency).sum();

        // Filter 1: High frequency (conservative threshold)
        if total_frequency < config.min_frequency {
            continue;
        }

        // Get database statistics for cost modeling
        let table_stats = db_stats.get_table_stats(&key.table).await?;
        let column_stats = db_stats.get_column_stats(&key.table, &key.column).await?;

        // Analyze each access type
        for pattern in patterns {
            match pattern.access_type {
                JsonAccessType::Filter => {
                    // Filter 2: High selectivity (reduces result set significantly)
                    if pattern.selectivity < config.min_selectivity {
                        continue;
                    }

                    let estimated_speedup = estimate_filter_speedup(
                        pattern,
                        &table_stats,
                        config.database_type,
                    );

                    // Filter 3: Meets speedup threshold
                    if estimated_speedup < config.min_speedup {
                        continue;
                    }

                    suggestions.push(create_denormalization_suggestion(
                        pattern,
                        &table_stats,
                        &column_stats,
                        estimated_speedup,
                        config.database_type,
                    ));
                }

                JsonAccessType::Sort => {
                    // Sorting on JSON is ALWAYS slow - always suggest
                    let estimated_speedup = estimate_sort_speedup(
                        pattern,
                        &table_stats,
                        config.database_type,
                    );

                    suggestions.push(create_denormalization_suggestion(
                        pattern,
                        &table_stats,
                        &column_stats,
                        estimated_speedup,
                        config.database_type,
                    ));
                }

                JsonAccessType::Aggregate => {
                    // GROUP BY on JSON path - high impact
                    let estimated_speedup = estimate_aggregate_speedup(
                        pattern,
                        &table_stats,
                        config.database_type,
                    );

                    if estimated_speedup >= config.min_speedup {
                        suggestions.push(create_denormalization_suggestion(
                            pattern,
                            &table_stats,
                            &column_stats,
                            estimated_speedup,
                            config.database_type,
                        ));
                    }
                }

                JsonAccessType::Project => {
                    // Projection alone rarely benefits from denormalization
                    // unless combined with other access types
                    continue;
                }
            }
        }
    }

    // Sort by impact (frequency × speedup)
    suggestions.sort_by(|a, b| {
        let impact_a = a.frequency as f64 * a.estimated_speedup;
        let impact_b = b.frequency as f64 * b.estimated_speedup;
        impact_b.partial_cmp(&impact_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(suggestions)
}
```text

**Suggestion Types**:

```rust
pub enum OptimizationSuggestion {
    Denormalize(DenormalizationSuggestion),
    AddIndex(IndexSuggestion),
    DropUnusedIndex(DropIndexSuggestion),
    MaterializeView(MaterializationSuggestion),  // Future
}

pub struct DenormalizationSuggestion {
    pub table: String,
    pub json_column: String,       // "dimensions" or "metadata"
    pub path: String,               // "region" or "location.city"
    pub new_column_name: String,   // "region_id" or "location_city"
    pub new_column_type: String,   // Inferred: "INTEGER", "TEXT", "TIMESTAMP"
    pub reason: String,
    pub frequency: u64,             // Queries per day
    pub estimated_speedup: f64,     // e.g., 12.5 = 12.5x faster
    pub estimated_storage_mb: f64,
    pub current_p95_ms: f64,
    pub projected_p95_ms: f64,
    pub migration: Vec<String>,     // SQL DDL statements
}

pub struct IndexSuggestion {
    pub table: String,
    pub columns: Vec<String>,
    pub index_type: IndexType,
    pub reason: String,
    pub estimated_speedup: f64,
    pub estimated_storage_mb: f64,
}

pub enum IndexType {
    BTree,           // Standard index
    Gin,             // PostgreSQL GIN (for JSON/arrays)
    Gist,            // PostgreSQL GIST (for spatial)
    Nonclustered,    // SQL Server nonclustered
    Clustered,       // SQL Server clustered
}
```text

---

### 2.2 Cost Estimation Model

**File**: `crates/FraiseQL-cli/src/schema/cost_model.rs`

**Purpose**: Estimate performance impact of suggested changes

**Cost Model Philosophy**:

- **JSON extraction**: Expensive (parse on every row)
- **Direct column**: Fast (native type, indexed)
- **Speedup factor**: JSON cost ÷ Column cost

**Filter Speedup Estimation**:

```rust
pub fn estimate_filter_speedup(
    pattern: &JsonAccessPattern,
    stats: &TableStatistics,
    db_type: DatabaseType,
) -> f64 {
    // JSONB/JSON filter cost (full table scan with parsing)
    let json_cost = match db_type {
        DatabaseType::PostgreSQL => {
            // JSONB is faster than text JSON
            stats.row_count as f64 * 0.05  // 0.05ms per row (binary format)
        }
        DatabaseType::SqlServer => {
            // JSON is text-based, slower parsing
            stats.row_count as f64 * 0.1  // 0.1ms per row
        }
        _ => stats.row_count as f64 * 0.08,
    };

    // Direct column cost (indexed B-tree lookup)
    let selectivity = pattern.selectivity.max(0.01);  // Prevent division by zero
    let rows_matched = (stats.row_count as f64 * selectivity) as u64;

    let column_cost = match db_type {
        DatabaseType::PostgreSQL | DatabaseType::MySQL => {
            // B-tree index lookup: O(log n) + scan matched rows
            (stats.row_count as f64).log2() * 0.001  // Index lookup
                + rows_matched as f64 * 0.0001        // Scan matched rows
        }
        DatabaseType::SqlServer => {
            // SQL Server nonclustered index + RID lookup
            (stats.row_count as f64).log2() * 0.001  // Index seek
                + rows_matched as f64 * 0.0002        // RID lookups (slower)
        }
        _ => (stats.row_count as f64).log2() * 0.001,
    };

    // Return speedup factor
    (json_cost / column_cost.max(0.001)).min(100.0)  // Cap at 100x
}
```text

**Sort Speedup Estimation**:

```rust
pub fn estimate_sort_speedup(
    pattern: &JsonAccessPattern,
    stats: &TableStatistics,
    db_type: DatabaseType,
) -> f64 {
    let n = stats.row_count as f64;

    // JSON sort cost: O(n log n) comparisons with JSON parsing each time
    let json_sort_cost = match db_type {
        DatabaseType::PostgreSQL => n * n.log2() * 0.05,  // JSONB parsing
        DatabaseType::SqlServer => n * n.log2() * 0.1,    // Text JSON parsing
        _ => n * n.log2() * 0.08,
    };

    // Direct column sort cost: Index scan (already sorted)
    let column_sort_cost = n * 0.0001;  // Sequential index scan

    (json_sort_cost / column_sort_cost.max(0.001)).min(100.0)
}
```text

**Aggregate Speedup Estimation**:

```rust
pub fn estimate_aggregate_speedup(
    pattern: &JsonAccessPattern,
    stats: &TableStatistics,
    db_type: DatabaseType,
) -> f64 {
    let n = stats.row_count as f64;

    // JSON GROUP BY cost: Parse + sort + aggregate
    let json_agg_cost = match db_type {
        DatabaseType::PostgreSQL => {
            // Hash aggregate possible but still needs parsing
            n * 0.05        // Parse all rows
                + n * 0.01  // Hash aggregate
        }
        DatabaseType::SqlServer => {
            // Often falls back to sort-based aggregate
            n * 0.1                  // Parse
                + n * n.log2() * 0.01  // Sort
        }
        _ => n * 0.08 + n * 0.01,
    };

    // Direct column GROUP BY cost: Index scan + aggregate
    let column_agg_cost = n * 0.0001  // Sequential scan
                        + n * 0.001;  // Hash aggregate

    (json_agg_cost / column_agg_cost.max(0.001)).min(50.0)
}
```text

**Storage Overhead Estimation**:

```rust
pub fn estimate_storage_overhead(
    path: &str,
    column_type: &str,
    stats: &TableStatistics,
) -> f64 {
    // Bytes per row for new column
    let bytes_per_row = match column_type {
        "BOOLEAN" | "TINYINT" => 1,
        "SMALLINT" => 2,
        "INTEGER" | "INT" => 4,
        "BIGINT" => 8,
        "REAL" | "FLOAT" => 4,
        "DOUBLE PRECISION" | "FLOAT(53)" => 8,
        "TIMESTAMP" | "DATETIME" | "DATETIME2" => 8,
        "DATE" => 4,
        "UUID" | "UNIQUEIDENTIFIER" => 16,
        "TEXT" | "VARCHAR" | "NVARCHAR" => 20,  // Average string length
        _ => 10,  // Default estimate
    };

    // Total column storage
    let column_storage = stats.row_count as f64 * bytes_per_row as f64;

    // Index storage (B-tree: ~2-3x column size)
    let index_storage = column_storage * 2.5;

    // Total in MB
    (column_storage + index_storage) / 1_048_576.0
}
```text

---

### 2.3 Migration Generator

**File**: `crates/FraiseQL-cli/src/schema/migration_generator.rs`

**Purpose**: Generate database-specific SQL for applying suggestions

**Multi-Database Migration Trait**:

```rust
pub trait MigrationGenerator: Send + Sync {
    fn generate_denormalization_migration(
        &self,
        suggestion: &DenormalizationSuggestion,
    ) -> Vec<String>;

    fn generate_index_migration(
        &self,
        suggestion: &IndexSuggestion,
    ) -> Vec<String>;

    fn generate_rollback_migration(
        &self,
        suggestion: &OptimizationSuggestion,
    ) -> Vec<String>;
}
```text

---

#### PostgreSQL Migration Generator

```rust
pub struct PostgresMigrationGenerator;

impl MigrationGenerator for PostgresMigrationGenerator {
    fn generate_denormalization_migration(
        &self,
        suggestion: &DenormalizationSuggestion,
    ) -> Vec<String> {
        vec![
            // Step 1: Add new column
            format!(
                "ALTER TABLE {} ADD COLUMN {} {};",
                suggestion.table,
                suggestion.new_column_name,
                suggestion.new_column_type
            ),

            // Step 2: Backfill data from JSONB
            format!(
                "UPDATE {} SET {} = ({}->>'{}')::{};",
                suggestion.table,
                suggestion.new_column_name,
                suggestion.json_column,
                suggestion.path,
                suggestion.new_column_type
            ),

            // Step 3: Create index
            format!(
                "CREATE INDEX CONCURRENTLY idx_{}_{} ON {} ({});",
                suggestion.table,
                suggestion.new_column_name,
                suggestion.table,
                suggestion.new_column_name
            ),

            // Step 4: Optionally set NOT NULL (if null fraction < 1%)
            if suggestion.null_fraction < 0.01 {
                format!(
                    "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL;",
                    suggestion.table,
                    suggestion.new_column_name
                )
            } else {
                String::new()
            },

            // Step 5: Analyze for statistics
            format!(
                "ANALYZE {};",
                suggestion.table
            ),
        ]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect()
    }

    fn generate_rollback_migration(
        &self,
        suggestion: &OptimizationSuggestion,
    ) -> Vec<String> {
        match suggestion {
            OptimizationSuggestion::Denormalize(s) => vec![
                format!(
                    "DROP INDEX IF EXISTS idx_{}_{};",
                    s.table, s.new_column_name
                ),
                format!(
                    "ALTER TABLE {} DROP COLUMN IF EXISTS {};",
                    s.table, s.new_column_name
                ),
            ],
            _ => vec![],
        }
    }
}
```text

---

#### SQL Server Migration Generator

```rust
pub struct SqlServerMigrationGenerator;

impl MigrationGenerator for SqlServerMigrationGenerator {
    fn generate_denormalization_migration(
        &self,
        suggestion: &DenormalizationSuggestion,
    ) -> Vec<String> {
        let sql_server_type = map_to_sqlserver_type(&suggestion.new_column_type);

        vec![
            // Step 1: Add new column (computed initially for zero downtime)
            format!(
                "ALTER TABLE {} ADD {} AS JSON_VALUE({}, '$.{}');",
                suggestion.table,
                suggestion.new_column_name,
                suggestion.json_column,
                suggestion.path
            ),

            // Step 2: Materialize computed column
            format!(
                "ALTER TABLE {} ALTER COLUMN {} ADD PERSISTED;",
                suggestion.table,
                suggestion.new_column_name
            ),

            // Step 3: Create nonclustered index
            format!(
                "CREATE NONCLUSTERED INDEX idx_{}_{} ON {} ({}) \
                 WITH (ONLINE = ON);",
                suggestion.table,
                suggestion.new_column_name,
                suggestion.table,
                suggestion.new_column_name
            ),

            // Step 4: Update statistics
            format!(
                "UPDATE STATISTICS {} WITH FULLSCAN;",
                suggestion.table
            ),
        ]
    }

    fn generate_rollback_migration(
        &self,
        suggestion: &OptimizationSuggestion,
    ) -> Vec<String> {
        match suggestion {
            OptimizationSuggestion::Denormalize(s) => vec![
                format!(
                    "DROP INDEX IF EXISTS idx_{}_{} ON {};",
                    s.table, s.new_column_name, s.table
                ),
                format!(
                    "ALTER TABLE {} DROP COLUMN IF EXISTS {};",
                    s.table, s.new_column_name
                ),
            ],
            _ => vec![],
        }
    }
}

/// Map generic SQL types to SQL Server types
fn map_to_sqlserver_type(generic_type: &str) -> &str {
    match generic_type {
        "TEXT" => "NVARCHAR(MAX)",
        "INTEGER" => "INT",
        "BIGINT" => "BIGINT",
        "BOOLEAN" => "BIT",
        "TIMESTAMP" => "DATETIME2",
        "UUID" => "UNIQUEIDENTIFIER",
        _ => generic_type,
    }
}
```text

**Key SQL Server Differences**:

1. **Computed Columns**: SQL Server can create persisted computed columns directly from JSON
2. **`PERSISTED` keyword**: Materializes the computed value
3. **`WITH (ONLINE = ON)`**: Allows index creation without locking table
4. **Type Mapping**: `TEXT` → `NVARCHAR(MAX)`, `TIMESTAMP` → `DATETIME2`, etc.

---

## Component 3: CLI Integration

### 3.1 Analyze Command

**File**: `crates/FraiseQL-cli/src/commands/analyze.rs`

```rust
pub async fn run(
    database: Option<String>,
    metrics_file: Option<String>,
    format: OutputFormat,
    window: String,
    min_frequency: u64,
    min_speedup: f64,
) -> Result<()> {

    // 1. Load configuration
    let config = AnalysisConfig {
        min_frequency,
        min_speedup,
        min_selectivity: 0.1,
        window_days: parse_window(&window)?,
        database_type: detect_database_type(&database)?,
    };

    // 2. Load metrics (from DB or JSON file)
    let metrics = if let Some(db_url) = &database {
        load_metrics_from_database(db_url, config.window_days).await?
    } else if let Some(file) = &metrics_file {
        load_metrics_from_json(file)?
    } else {
        anyhow::bail!("Must provide either --database or --metrics");
    };

    // 3. Connect to database for statistics (optional)
    let db_stats: Box<dyn DatabaseStatistics> = if let Some(db_url) = &database {
        match config.database_type {
            DatabaseType::PostgreSQL => {
                Box::new(PostgresStatistics::connect(db_url).await?)
            }
            DatabaseType::SqlServer => {
                Box::new(SqlServerStatistics::connect(db_url).await?)
            }
            _ => anyhow::bail!("Database type not supported yet"),
        }
    } else {
        // Use metrics-only stats (less accurate)
        Box::new(MetricsOnlyStatistics::new(&metrics))
    };

    // 4. Run analysis
    println!("📊 Analyzing query patterns...");
    let suggestions = analyze_denormalization_opportunities(
        &metrics,
        &metrics.json_patterns,
        db_stats.as_ref(),
        &config,
    )?;

    // 5. Output results
    match format {
        OutputFormat::Text => print_text_report(&suggestions),
        OutputFormat::Json => print_json_report(&suggestions),
        OutputFormat::Sql => print_sql_migrations(&suggestions, config.database_type),
    }

    Ok(())
}

fn detect_database_type(url: &Option<String>) -> Result<DatabaseType> {
    match url {
        Some(url) if url.starts_with("postgres://") => Ok(DatabaseType::PostgreSQL),
        Some(url) if url.starts_with("sqlserver://") => Ok(DatabaseType::SqlServer),
        Some(url) if url.starts_with("mysql://") => Ok(DatabaseType::MySQL),
        Some(url) if url.starts_with("sqlite://") => Ok(DatabaseType::SQLite),
        None => Ok(DatabaseType::PostgreSQL),  // Default
        _ => anyhow::bail!("Unknown database URL format"),
    }
}
```text

**Output Formats**:

1. **Text Format** (human-readable):

```text
📊 Observability Analysis Report

🚀 High-Impact Optimizations (3):

  1. Denormalize JSON → Direct Column
     Table: tf_sales
     Path:  dimensions->>'region' (PostgreSQL)
            JSON_VALUE(dimensions, '$.region') (SQL Server)
     → New column: region_id (INTEGER)

     Impact:
     • 8,500 queries/day affected
     • Estimated speedup: 12.5x
     • Current p95: 1,250ms → Projected: 100ms
     • Storage cost: +15 MB

     Reason: Frequently filtered with high selectivity (8%)
```text

1. **JSON Format** (for CI/CD):

```json
{
  "version": "1.0",
  "analyzed_at": "2026-01-12T16:30:00Z",
  "database_type": "sqlserver",
  "window_days": 7,
  "suggestions": [
    {
      "type": "denormalize",
      "table": "tf_sales",
      "json_column": "dimensions",
      "path": "region",
      "new_column": "region_id",
      "new_type": "INTEGER",
      "frequency": 8500,
      "estimated_speedup": 12.5,
      "estimated_storage_mb": 15.0,
      "current_p95_ms": 1250.0,
      "projected_p95_ms": 100.0
    }
  ]
}
```text

1. **SQL Format** (ready to apply):

```sql
-- Migration 1: Denormalize tf_sales.dimensions->>'region'
-- Database: SQL Server
-- Estimated impact: 8,500 queries/day, 12.5x speedup

-- Step 1: Add computed column
ALTER TABLE tf_sales ADD region_id AS JSON_VALUE(dimensions, '$.region');

-- Step 2: Persist computed column
ALTER TABLE tf_sales ALTER COLUMN region_id ADD PERSISTED;

-- Step 3: Create index
CREATE NONCLUSTERED INDEX idx_tf_sales_region_id
  ON tf_sales (region_id)
  WITH (ONLINE = ON);

-- Step 4: Update statistics
UPDATE STATISTICS tf_sales WITH FULLSCAN;
```text

---

## Performance Impact

### Metrics Collection Overhead

**Target**: < 5% latency increase

**Measurement Strategy**:

```rust
// Benchmark with observability OFF
cargo bench --bench query_execution -- --baseline off

// Benchmark with observability ON (10% sampling)
cargo bench --bench query_execution -- --baseline on

// Compare results
```text

**Expected Overhead**:

| Sampling Rate | Latency Increase | Memory Usage |
|---------------|------------------|--------------|
| 1% | < 1% | +10 MB |
| 10% (default) | < 5% | +50 MB |
| 100% | ~15% | +200 MB |

**Mitigation Strategies**:

1. **Sampling**: Only collect metrics for X% of queries
2. **Batch Writes**: Buffer 100 metrics, write once
3. **Async Flushing**: Write to database in background task
4. **Aggregation**: Compute p50/p95/p99 in memory, write summaries

---

## Design Decisions

### Decision 1: Opt-In by Default

**Rationale**: Production safety - metrics collection disabled unless explicitly enabled

**Configuration**:

```bash
# Must explicitly enable
export FRAISEQL_OBSERVABILITY_ENABLED=true

# Or in FraiseQL.toml
[observability]
enabled = true
```text

---

### Decision 2: PostgreSQL/SQL Server Metrics Storage

**Rationale**:

- Persistent across restarts
- Time-series queries for analysis
- Production-grade (replicated, backed up)
- SQL analysis queries

**Trade-off**:

- ✅ Durability, queryability
- ❌ Requires database connection (optional, can export to JSON)

---

### Decision 3: Optional Database Connection for Analysis

**Rationale**: Flexibility - can analyze offline from exported JSON

**Modes**:

1. **With DB connection** (best): Uses `pg_stats` / `sys.stats` for accurate estimates
2. **Without DB** (good): Uses metrics-only heuristics

---

### Decision 4: Conservative Thresholds

**Rationale**: Reduce noise, only suggest clear wins

**Defaults**:

- Frequency: 1000+ queries/day
- Speedup: 5x+ improvement
- Selectivity: 10%+ filtered rows

**User-configurable via CLI flags**.

---

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_model_filter_speedup() {
        let pattern = JsonAccessPattern {
            table_name: "users".to_string(),
            json_column: "metadata".to_string(),
            path: "region".to_string(),
            access_type: JsonAccessType::Filter,
            frequency: 5000,
            selectivity: 0.15,
        };

        let stats = TableStatistics {
            table_name: "users".to_string(),
            row_count: 1_000_000,
            dead_rows: 0,
            last_vacuum: None,
            last_analyze: Some(SystemTime::now()),
            size_bytes: 500_000_000,
        };

        let speedup = estimate_filter_speedup(
            &pattern,
            &stats,
            DatabaseType::SqlServer,
        );

        assert!(speedup >= 5.0, "Speedup should be at least 5x");
        assert!(speedup <= 100.0, "Speedup capped at 100x");
    }

    #[test]
    fn test_sqlserver_json_path_parsing() {
        let sql = "SELECT id, JSON_VALUE(dimensions, '$.region') AS region \
                   FROM tf_sales \
                   WHERE JSON_VALUE(dimensions, '$.category') = 'Electronics'";

        let parser = SqlServerJsonPathParser;
        let patterns = parser.extract_paths(sql);

        assert_eq!(patterns.len(), 2);
        assert_eq!(patterns[0].path, "region");
        assert_eq!(patterns[0].access_type, JsonAccessType::Project);
        assert_eq!(patterns[1].path, "category");
        assert_eq!(patterns[1].access_type, JsonAccessType::Filter);
    }
}
```text

### Integration Tests

```rust
#[tokio::test]
async fn test_end_to_end_analysis_sqlserver() {
    // 1. Setup test SQL Server database with metrics
    let pool = setup_sqlserver_test_db().await;
    seed_metrics(&pool, DatabaseType::SqlServer).await;

    // 2. Run analysis
    let config = AnalysisConfig {
        min_frequency: 100,
        min_speedup: 2.0,
        database_type: DatabaseType::SqlServer,
        ..Default::default()
    };

    let db_stats = SqlServerStatistics::new(pool.clone());
    let metrics = load_metrics_from_database(&pool, 7).await.unwrap();

    let suggestions = analyze_denormalization_opportunities(
        &metrics,
        &metrics.json_patterns,
        &db_stats,
        &config,
    ).unwrap();

    // 3. Verify suggestions
    assert!(!suggestions.is_empty());

    let first = &suggestions[0];
    if let OptimizationSuggestion::Denormalize(s) = first {
        assert_eq!(s.table, "tf_sales");
        assert!(s.estimated_speedup >= 2.0);

        // 4. Generate and test SQL Server migration
        let generator = SqlServerMigrationGenerator;
        let migration_sql = generator.generate_denormalization_migration(s);

        assert!(migration_sql[0].contains("ALTER TABLE"));
        assert!(migration_sql[0].contains("JSON_VALUE"));
        assert!(migration_sql.iter().any(|s| s.contains("PERSISTED")));
    } else {
        panic!("Expected denormalization suggestion");
    }
}
```text

---

## Future Enhancements

### Phase 2 Features

1. **Materialized View Suggestions** - Detect expensive aggregates
2. **Query Plan Analysis** - Parse `EXPLAIN` output for bottlenecks
3. **Auto-Scaling Suggestions** - Read replicas, partitioning
4. **Cost Optimization** - Suggest dropping unused indexes

### Phase 3 Features

1. **Machine Learning** - Predict future query patterns
2. **A/B Testing** - Compare performance before/after migrations
3. **Cloud Integration** - Export to Prometheus/Grafana/DataDog
4. **Real-Time Alerts** - Notify when optimization opportunities detected

---

## Summary

FraiseQL's observability system provides:

✅ **Runtime-informed optimization** based on actual query patterns
✅ **Multi-database support** (PostgreSQL, SQL Server, MySQL, SQLite)
✅ **Database-agnostic architecture** with per-database implementations
✅ **Conservative suggestions** (1000+ queries/day, 5x+ speedup)
✅ **Low overhead** (<5% latency with 10% sampling)
✅ **Production-safe** (opt-in, manual review required)
✅ **Automated SQL generation** for applying changes

**Next**: See [Configuration Guide](configuration.md) for complete setup reference.

---

*Last updated: 2026-01-12*
