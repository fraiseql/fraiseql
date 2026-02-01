# Analysis Guide: Using the `analyze` Command

## Overview

The `fraiseql-cli analyze` command analyzes runtime metrics and generates schema optimization suggestions. This guide covers:

- Running analysis with different data sources
- Interpreting results
- Customizing thresholds
- Filtering suggestions
- Output formats

---

## Prerequisites

Before running analysis, ensure:

1. ✅ Observability enabled for 24-48 hours minimum
2. ✅ Metrics database accessible (or exported JSON file)
3. ✅ FraiseQL CLI installed (`cargo install fraiseql-cli`)

---

## Quick Start

### Basic Analysis (PostgreSQL)

```bash
fraiseql-cli analyze --database postgres://user:pass@localhost:5432/mydb
```

### Basic Analysis (SQL Server)

```bash
fraiseql-cli analyze --database sqlserver://user:pass@localhost:1433/mydb
```

**Output**:

```
📊 Observability Analysis Report

Database: PostgreSQL
Window: Last 7 days
Analyzed: 8,500,000 query executions

🚀 High-Impact Optimizations (3):

  1. Denormalize JSONB → Direct Column
     Table: tf_sales
     Path:  dimensions->>'region'
     → New column: region_id (TEXT)

     Impact:
     • 8,500 queries/day affected
     • Estimated speedup: 12.5x
     • Current p95: 1,250ms → Projected: 100ms
     • Storage cost: +15 MB

     Reason: Frequently filtered with high selectivity (8%)
```

---

## Command Syntax

```bash
fraiseql-cli analyze [OPTIONS]
```

### Required Options (Pick One)

| Option | Description | Example |
|--------|-------------|---------|
| `--database <URL>` | Analyze from metrics database | `--database postgres://...` |
| `--metrics <FILE>` | Analyze from exported JSON | `--metrics metrics.json` |

**Note**: `--database` is recommended (more accurate, uses DB statistics).

---

## Analysis Options

### Time Window

**`--window <DURATION>`** (default: `7d`)

Analyze metrics from the last N days/hours.

```bash
# Last 24 hours
fraiseql-cli analyze --database postgres://... --window 1d

# Last 30 days
fraiseql-cli analyze --database postgres://... --window 30d

# Last 7 days (default)
fraiseql-cli analyze --database postgres://...
```

**Supported formats**:

- `1h`, `6h`, `12h` - Hours
- `1d`, `7d`, `30d`, `90d` - Days

**Guidelines**:

| Window | Use Case | Trade-off |
|--------|----------|-----------|
| 1d | Quick check during development | May miss weekly patterns |
| 7d (default) | Weekly patterns | Balanced |
| 30d | Monthly trends | Includes seasonal traffic |
| 90d | Long-term patterns | May include stale data |

---

### Frequency Threshold

**`--min-frequency <N>`** (default: `1000`)

Minimum queries per day to suggest optimization.

```bash
# Lower threshold for low-traffic apps
fraiseql-cli analyze \
  --database postgres://... \
  --min-frequency 100

# Higher threshold for high-traffic apps
fraiseql-cli analyze \
  --database postgres://... \
  --min-frequency 5000
```

**Guidelines**:

| Threshold | Suggestions | Use Case |
|-----------|-------------|----------|
| 10-100 | Many | Development/testing |
| 1000 (default) | High-impact | Production |
| 5000+ | Critical paths only | High-traffic apps |

**Example Impact**:

With `--min-frequency 100`:

- Suggests optimizing paths accessed 100+ times/day
- Result: ~20 suggestions (more noise)

With `--min-frequency 5000`:

- Only suggests paths accessed 5000+ times/day
- Result: ~3 suggestions (clear wins)

---

### Speedup Threshold

**`--min-speedup <FACTOR>`** (default: `5.0`)

Minimum estimated speedup factor (e.g., 5.0 = 5x faster).

```bash
# Lower threshold (more suggestions)
fraiseql-cli analyze \
  --database postgres://... \
  --min-speedup 2.0

# Higher threshold (only huge wins)
fraiseql-cli analyze \
  --database postgres://... \
  --min-speedup 10.0
```

**Guidelines**:

| Threshold | Meaning | Use Case |
|-----------|---------|----------|
| 2.0 | 2x faster | Aggressive optimization |
| 5.0 (default) | 5x faster | Balanced (recommended) |
| 10.0 | 10x faster | Only massive improvements |

---

### Selectivity Threshold

**`--min-selectivity <FRACTION>`** (default: `0.1`)

Minimum filter selectivity (fraction of rows filtered).

```bash
# Very selective filters only
fraiseql-cli analyze \
  --database postgres://... \
  --min-selectivity 0.01  # 1% of rows
```

**Selectivity Explained**:

Selectivity = (Rows Matched) ÷ (Total Rows)

**Examples**:

```sql
-- High selectivity (10%): Good candidate
WHERE JSON_VALUE(metadata, '$.country') = 'USA'
-- Returns 10,000 / 100,000 rows → selectivity = 0.1

-- Low selectivity (90%): NOT a good candidate
WHERE JSON_VALUE(metadata, '$.active') = 'true'
-- Returns 90,000 / 100,000 rows → selectivity = 0.9
```

**Why selectivity matters**:

- **High selectivity** → Index helps significantly
- **Low selectivity** → Index doesn't help (most rows match anyway)

---

## Output Formats

### Text Format (Default)

**`--format text`**

Human-readable output for terminal viewing.

```bash
fraiseql-cli analyze --database postgres://... --format text
```

**Output**:

```
📊 Observability Analysis Report

Database: PostgreSQL
Window: Last 7 days
Analyzed: 8,500,000 query executions
Query patterns: 250 unique queries

🚀 High-Impact Optimizations (3):

  1. Denormalize JSONB → Direct Column
     Table: tf_sales
     Path:  dimensions->>'region'
     → New column: region_id (TEXT)

     Impact:
     • 8,500 queries/day affected
     • Estimated speedup: 12.5x
     • Current p95: 1,250ms → Projected: 100ms
     • Storage cost: +15 MB

     Reason: Frequently filtered with high selectivity (8%)

  2. Add Index
     Table: users
     Column: created_at

     Impact:
     • 3,200 queries/day affected
     • Estimated speedup: 8x
     • Current p95: 850ms → Projected: 106ms
     • Storage cost: +5 MB

     Reason: Sorted in 90% of queries, no index exists

  3. Denormalize JSON → Direct Column
     Table: orders
     Path:  metadata.customer_tier
     → New column: customer_tier (TEXT)

     Impact:
     • 2,100 queries/day affected
     • Estimated speedup: 6.2x
     • Current p95: 620ms → Projected: 100ms
     • Storage cost: +8 MB

     Reason: Used in filters and aggregates

---

💡 Next Steps:
   1. Generate migration SQL: fraiseql-cli analyze --format sql > optimize.sql
   2. Review changes: less optimize.sql
   3. Test in staging: psql staging < optimize.sql
   4. Apply to production: psql production < optimize.sql
```

---

### JSON Format

**`--format json`**

Machine-readable output for CI/CD integration.

```bash
fraiseql-cli analyze --database postgres://... --format json > report.json
```

**Output**:

```json
{
  "version": "1.0",
  "analyzed_at": "2026-01-12T16:30:00Z",
  "database_type": "postgresql",
  "window": {
    "start": "2026-01-05T16:30:00Z",
    "end": "2026-01-12T16:30:00Z",
    "days": 7
  },
  "metrics": {
    "total_executions": 8500000,
    "unique_queries": 250,
    "avg_execution_time_ms": 125.3
  },
  "suggestions": [
    {
      "type": "denormalize",
      "priority": "high",
      "table": "tf_sales",
      "json_column": "dimensions",
      "path": "region",
      "new_column": "region_id",
      "new_type": "TEXT",
      "reason": "Frequently filtered with high selectivity (8%)",
      "impact": {
        "queries_per_day": 8500,
        "estimated_speedup": 12.5,
        "current_p95_ms": 1250.0,
        "projected_p95_ms": 100.0,
        "storage_mb": 15.0
      }
    },
    {
      "type": "add_index",
      "priority": "high",
      "table": "users",
      "columns": ["created_at"],
      "reason": "Sorted in 90% of queries, no index exists",
      "impact": {
        "queries_per_day": 3200,
        "estimated_speedup": 8.0,
        "current_p95_ms": 850.0,
        "projected_p95_ms": 106.0,
        "storage_mb": 5.0
      }
    }
  ]
}
```

**Use in CI/CD**:

```bash
# Run analysis and parse results
SUGGESTIONS=$(fraiseql-cli analyze --database postgres://... --format json | jq '.suggestions | length')

if [ $SUGGESTIONS -gt 0 ]; then
  echo "⚠️  Found $SUGGESTIONS optimization opportunities"
  echo "Run 'fraiseql-cli analyze --format text' for details"
fi
```

---

### SQL Format

**`--format sql`**

Ready-to-execute migration SQL.

```bash
fraiseql-cli analyze --database postgres://... --format sql > optimize.sql
```

**PostgreSQL Output**:

```sql
-- ============================================================
-- FraiseQL Observability-Driven Schema Optimization
-- Generated: 2026-01-12 16:30:00 UTC
-- Database: PostgreSQL
-- Window: 2026-01-05 to 2026-01-12 (7 days)
-- ============================================================

-- ------------------------------------------------------------
-- Migration 1: Denormalize dimensions->>'region'
-- Table: tf_sales
-- Impact: 8,500 queries/day, 12.5x speedup
-- Storage: +15 MB
-- ------------------------------------------------------------

-- Step 1: Add new column
ALTER TABLE tf_sales ADD COLUMN region_id TEXT;

-- Step 2: Backfill data from JSONB
UPDATE tf_sales SET region_id = dimensions->>'region';

-- Step 3: Create index (CONCURRENTLY to avoid blocking writes)
CREATE INDEX CONCURRENTLY idx_tf_sales_region_id
  ON tf_sales (region_id);

-- Step 4: Analyze for statistics
ANALYZE tf_sales;

-- Rollback (if needed):
-- DROP INDEX IF EXISTS idx_tf_sales_region_id;
-- ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;


-- ------------------------------------------------------------
-- Migration 2: Add index on users.created_at
-- Table: users
-- Impact: 3,200 queries/day, 8x speedup
-- Storage: +5 MB
-- ------------------------------------------------------------

-- Step 1: Create index
CREATE INDEX CONCURRENTLY idx_users_created_at
  ON users (created_at);

-- Step 2: Analyze for statistics
ANALYZE users;

-- Rollback (if needed):
-- DROP INDEX IF EXISTS idx_users_created_at;


-- ============================================================
-- Post-Migration Steps
-- ============================================================
-- 1. Update application schema.json to use new columns
-- 2. Recompile: fraiseql-cli compile schema.json
-- 3. Monitor query performance
-- ============================================================
```

**SQL Server Output**:

```sql
-- ============================================================
-- FraiseQL Observability-Driven Schema Optimization
-- Generated: 2026-01-12 16:30:00 UTC
-- Database: SQL Server
-- Window: 2026-01-05 to 2026-01-12 (7 days)
-- ============================================================

-- ------------------------------------------------------------
-- Migration 1: Denormalize JSON_VALUE(dimensions, '$.region')
-- Table: tf_sales
-- Impact: 8,500 queries/day, 12.5x speedup
-- Storage: +15 MB
-- ------------------------------------------------------------

-- Step 1: Add computed column
ALTER TABLE tf_sales ADD region_id AS JSON_VALUE(dimensions, '$.region');
GO

-- Step 2: Materialize computed column
ALTER TABLE tf_sales ALTER COLUMN region_id ADD PERSISTED;
GO

-- Step 3: Create nonclustered index (ONLINE to avoid blocking)
CREATE NONCLUSTERED INDEX idx_tf_sales_region_id
  ON tf_sales (region_id)
  WITH (ONLINE = ON);
GO

-- Step 4: Update statistics
UPDATE STATISTICS tf_sales WITH FULLSCAN;
GO

-- Rollback (if needed):
-- DROP INDEX IF EXISTS idx_tf_sales_region_id ON tf_sales;
-- ALTER TABLE tf_sales DROP COLUMN IF EXISTS region_id;
-- GO
```

---

## Analyzing from Exported Metrics

### Step 1: Export Metrics

**Via HTTP endpoint**:

```bash
curl http://localhost:8080/metrics/export > metrics.json
```

**Via CLI** (if server not running):

```bash
fraiseql-cli export-metrics \
  --database postgres://... \
  --output metrics.json \
  --window 7d
```

### Step 2: Analyze Offline

```bash
fraiseql-cli analyze --metrics metrics.json
```

**Why export?**

✅ **Offline analysis**: No need for live database connection
✅ **CI/CD integration**: Check metrics in automated pipelines
✅ **Archival**: Keep historical analysis for comparison
✅ **Security**: Avoid exposing production database credentials

**Limitations**:

⚠️ **Less accurate**: Can't access `pg_stats` / `sys.stats` for precise estimates
⚠️ **No real-time data**: Metrics frozen at export time

---

## Filtering Suggestions

### By Table

**`--table <NAME>`**

Only analyze specific table(s).

```bash
# Analyze only tf_sales table
fraiseql-cli analyze \
  --database postgres://... \
  --table tf_sales
```

### By Suggestion Type

**`--type <TYPE>`**

Filter by suggestion type: `denormalize`, `add_index`, `drop_index`.

```bash
# Only denormalization suggestions
fraiseql-cli analyze \
  --database postgres://... \
  --type denormalize
```

### By Impact

**`--min-impact <SCORE>`**

Filter by impact score (frequency × speedup).

```bash
# Only suggestions with impact > 10,000
fraiseql-cli analyze \
  --database postgres://... \
  --min-impact 10000
```

**Impact Score Calculation**:

```
Impact = (Queries Per Day) × (Speedup Factor)

Example:
- 8,500 queries/day × 12.5x speedup = 106,250 impact score
```

---

## Comparing Analysis Over Time

### Track Optimization Progress

```bash
# Week 1: Before optimization
fraiseql-cli analyze --database postgres://... --format json > week1.json

# Apply migrations
psql production < optimize.sql

# Week 2: After optimization
fraiseql-cli analyze --database postgres://... --format json > week2.json

# Compare
fraiseql-cli diff-analysis week1.json week2.json
```

**Output**:

```
📊 Analysis Comparison

Week 1 (2026-01-05):
  - 3 high-impact suggestions
  - Total potential speedup: 26.7x
  - Projected latency reduction: 6,200ms → 850ms

Week 2 (2026-01-12):
  - 1 high-impact suggestion
  - Total potential speedup: 3.2x
  - Projected latency reduction: 850ms → 265ms

✅ Applied optimizations:
  - tf_sales.region_id (12.5x speedup) ✅
  - users.created_at index (8x speedup) ✅

⏳ Remaining opportunities:
  - orders.customer_tier (3.2x speedup)
```

---

## Advanced Analysis

### Custom Thresholds for Development

Low-traffic apps may need relaxed thresholds:

```bash
fraiseql-cli analyze \
  --database postgres://... \
  --min-frequency 10 \       # Just 10 queries/day
  --min-speedup 2.0 \        # 2x speedup
  --min-selectivity 0.05 \   # 5% selectivity
  --window 1d                # Last 24 hours
```

### High-Traffic Production

High-traffic apps need stricter thresholds:

```bash
fraiseql-cli analyze \
  --database postgres://... \
  --min-frequency 10000 \    # 10K queries/day
  --min-speedup 10.0 \       # 10x speedup minimum
  --min-selectivity 0.2 \    # 20% selectivity
  --window 30d               # 30-day window
```

---

## Continuous Analysis

### Scheduled Analysis

Run analysis weekly and alert on new suggestions:

**Cron job** (every Monday at 2 AM):

```bash
# /etc/cron.d/fraiseql-analysis
0 2 * * 1 fraiseql fraiseql-cli analyze \
  --database postgres://metrics:pass@metrics-db:5432/metrics \
  --format json > /var/log/fraiseql/analysis-$(date +\%Y\%m\%d).json
```

**Alerting script**:

```bash
#!/bin/bash
SUGGESTIONS=$(fraiseql-cli analyze --database postgres://... --format json | \
  jq '.suggestions | length')

if [ $SUGGESTIONS -gt 0 ]; then
  echo "⚠️  Found $SUGGESTIONS optimization opportunities" | \
    mail -s "FraiseQL Analysis Alert" [email protected]
fi
```

---

## Troubleshooting Analysis

### Issue: No Suggestions Generated

**Symptoms**: Analysis returns 0 suggestions

**Possible Causes**:

1. **Insufficient data**

   ```bash
   # Check metrics count
   psql postgres://... -c "
     SELECT COUNT(*) FROM fraiseql_metrics.query_executions
     WHERE executed_at > NOW() - INTERVAL '7 days'
   "
   ```

   **Solution**: Wait for 24-48 hours of metrics collection

2. **Thresholds too high**

   ```bash
   # Try lower thresholds
   fraiseql-cli analyze \
     --database postgres://... \
     --min-frequency 10 \
     --min-speedup 2.0
   ```

3. **No JSON usage**

   Observability focuses on JSON/JSONB optimization. If your app doesn't use JSON columns, suggestions will be limited.

---

### Issue: Unrealistic Speedup Estimates

**Symptoms**: "Estimated speedup: 100x" seems too high

**Explanation**: Cost model uses theoretical O(n) vs O(log n) calculations.

**Reality Check**:

- **Filter speedup**: 5-20x typical, 50x+ possible for large tables
- **Sort speedup**: 10-50x typical
- **Aggregate speedup**: 5-15x typical

**What to do**: Treat estimates as relative importance, not absolute guarantees. Always test in staging.

---

### Issue: Analysis Takes Too Long

**Symptoms**: `analyze` command runs for > 5 minutes

**Causes**:

- Large metrics tables (> 100M rows)
- Complex aggregations on unindexed metrics tables

**Solutions**:

1. **Add indexes to metrics tables**:

   ```sql
   CREATE INDEX idx_query_name ON fraiseql_metrics.query_executions (query_name);
   CREATE INDEX idx_jsonb_path ON fraiseql_metrics.jsonb_accesses (table_name, path);
   ```

2. **Use shorter time window**:

   ```bash
   fraiseql-cli analyze --database postgres://... --window 1d
   ```

3. **Export and analyze offline**:

   ```bash
   fraiseql-cli export-metrics --database postgres://... --output metrics.json
   fraiseql-cli analyze --metrics metrics.json
   ```

---

## Best Practices

### 1. Regular Analysis Schedule

Run analysis **weekly** to catch performance regressions early.

### 2. Start Conservative

Use default thresholds (1000 queries/day, 5x speedup) initially. Lower thresholds if needed.

### 3. Test in Staging First

Always apply migrations to staging environment before production:

```bash
# Generate SQL
fraiseql-cli analyze --database postgres://prod --format sql > optimize.sql

# Test in staging
psql staging < optimize.sql

# Benchmark queries
fraiseql-cli benchmark --database postgres://staging

# If successful, apply to production
psql production < optimize.sql
```

### 4. Monitor After Migrations

Track query performance for 24-48 hours after applying migrations:

```bash
# Compare before/after
fraiseql-cli analyze --database postgres://... --window 7d > before.txt
# (apply migrations)
fraiseql-cli analyze --database postgres://... --window 7d > after.txt
diff before.txt after.txt
```

### 5. Keep Historical Reports

Archive analysis reports for trend analysis:

```bash
fraiseql-cli analyze --database postgres://... --format json > \
  reports/analysis-$(date +%Y-%m-%d).json
```

---

## Example Workflows

### Workflow 1: Weekly Production Analysis

```bash
#!/bin/bash
# weekly-analysis.sh

# 1. Analyze production metrics
fraiseql-cli analyze \
  --database postgres://prod-metrics:5432/metrics \
  --format json > /tmp/analysis.json

# 2. Check for high-priority suggestions
HIGH_PRIORITY=$(jq '.suggestions | map(select(.priority == "high")) | length' /tmp/analysis.json)

if [ $HIGH_PRIORITY -gt 0 ]; then
  # 3. Generate migration SQL
  fraiseql-cli analyze \
    --database postgres://prod-metrics:5432/metrics \
    --format sql > migrations/optimize-$(date +%Y%m%d).sql

  # 4. Alert team
  echo "⚠️  Found $HIGH_PRIORITY high-priority optimizations" | \
    slack-notify --channel=#db-ops
fi
```

### Workflow 2: Development Iteration

```bash
# 1. Start with low thresholds
fraiseql-cli analyze \
  --database postgres://localhost:5432/dev \
  --min-frequency 10 \
  --window 1d

# 2. Apply suggestions
fraiseql-cli analyze --database postgres://localhost:5432/dev \
  --format sql | psql dev

# 3. Update schema
vim schema.json  # Add denormalized columns

# 4. Recompile
fraiseql-cli compile schema.json

# 5. Test queries
npm test
```

---

## Next Steps

- **[Optimization Suggestions](../observability/optimization-suggestions.md)** - Understanding output in detail
- **[Migration Workflow](../observability/migration-workflow.md)** - Safely applying changes

---

*Last updated: 2026-01-12*
