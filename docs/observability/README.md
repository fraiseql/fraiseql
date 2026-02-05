# Observability-Driven Schema Optimization

## What is Observability-Driven Optimization?

Traditional database optimization relies on static analysis and developer intuition. You might guess which columns need indexes or which queries are slow, but without runtime data, you're operating blind.

**Observability-Driven Optimization** changes this by analyzing **actual query patterns** from your production traffic. FraiseQL's observability system:

1. **Collects metrics** on query execution (timing, frequency, JSONB path access)
2. **Identifies patterns** (frequently filtered fields, expensive JSONB extractions)
3. **Suggests optimizations** (denormalize JSONB to direct columns, add indexes)
4. **Generates migrations** (production-ready SQL to apply changes)

### Key Benefits

- **10-50x query speedup** in real-world cases
- **Data-driven decisions** based on actual usage, not guesses
- **Zero application changes** - schema improves transparently
- **Conservative suggestions** - only high-impact, low-risk changes
- **Automated SQL generation** - no manual migration writing

### How It Differs from Static Analysis

| Static Analysis | Observability-Driven |
|----------------|---------------------|
| Analyzes schema structure | Analyzes actual queries |
| Guesses common patterns | Measures real traffic |
| Generic suggestions | Specific to your workload |
| One-time scan | Continuous improvement |

---

## Quick Start

Get optimization suggestions in 5 minutes:

### 1. Enable Observability

Add to your environment or config:

```bash
# Environment variable
export FRAISEQL_OBSERVABILITY_ENABLED=true
export FRAISEQL_DATABASE_URL=postgres://user:pass@localhost/mydb

# Or in fraiseql.toml
[observability]
enabled = true
sample_rate = 0.1  # 10% sampling
```text

### 2. Run Your Application

Let it collect metrics for **24-48 hours** with normal traffic. The system will:

- Track query execution times
- Monitor JSONB path accesses
- Record filter selectivity
- Measure aggregate query performance

**Privacy Note**: Only query structure is logged, never user data or query arguments.

### 3. Analyze Metrics

```bash
# Analyze directly from metrics database
fraiseql-cli analyze --database postgres://...

# Or export and analyze offline
curl http://localhost:8080/metrics/export > metrics.json
fraiseql-cli analyze --metrics metrics.json
```text

### 4. Review Suggestions

Example output:

```text
📊 Observability Analysis Report

🚀 High-Impact Optimizations (2):

  1. Denormalize JSONB → Direct Column
     Table: tf_sales
     Path:  dimensions->>'region'
     → New column: region_id (TEXT)

     Impact:
     • 5,000 queries/day affected
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
     • Storage cost: +5 MB

     Reason: Sorted in 90% of queries, no index exists
```text

### 5. Generate and Apply Migrations

```bash
# Generate SQL
fraiseql-cli analyze --database postgres://... --format sql > optimize.sql

# Review
less optimize.sql

# Test in staging
psql staging < optimize.sql

# Apply to production
psql production < optimize.sql

# Update schema and recompile
fraiseql-cli compile schema.json
```text

---

## When to Use

Observability-Driven Optimization is most valuable for:

### ✅ High-Traffic JSONB Queries

- Analytics dashboards with dimension filtering
- Multi-tenant SaaS with JSONB metadata
- Event tracking with nested JSON properties
- User profiles with flexible attributes

### ✅ Repeated Filtering on Nested Fields

```sql
-- Slow: JSONB extraction on every query
WHERE dimensions->>'region' = 'US'

-- After optimization: Direct column lookup
WHERE region_id = 'US'  -- 10-15x faster
```text

### ✅ Aggregate Queries on Dimensions

```sql
-- Slow: GROUP BY on JSONB expression
GROUP BY dimensions->>'category'

-- After optimization: Indexed column
GROUP BY category_id  -- 5-10x faster
```text

### ✅ Slow Query Alerts

- Queries consistently over 1000ms
- P95/P99 latency spikes
- High database CPU usage
- Long-running analytics jobs

---

## Prerequisites

Before using observability:

- **FraiseQL v2.0+** (observability system introduced in v2.0)
- **PostgreSQL 12+** (for metrics storage and pg_stats access)
- **Basic SQL knowledge** (to review generated migrations)
- **Production/staging traffic** (at least 24 hours of queries)

---

## Example: Real-World Impact

**Scenario**: E-commerce analytics dashboard

**Before**:

```python
@fraiseql.fact_table(
    table_name='tf_sales',
    dimension_column='dimensions'  # JSONB
)
class SalesMetrics:
    revenue: float
    quantity: int
    dimensions: dict  # {region, category, date}
```text

**Metrics**:

- 8,500 queries/day filtering on `region`
- Average: 850ms
- P95: 1,250ms

**Suggested Optimization**:

```sql
ALTER TABLE tf_sales ADD COLUMN region_id TEXT;
UPDATE tf_sales SET region_id = dimensions->>'region';
CREATE INDEX idx_tf_sales_region ON tf_sales (region_id);
```text

**After**:

```python
@fraiseql.fact_table(
    table_name='tf_sales',
    dimension_column='dimensions',
    denormalized_filters=['region_id']  # NEW
)
class SalesMetrics:
    revenue: float
    quantity: int
    region_id: str  # Direct column (indexed)
    dimensions: dict
```text

**Results**:

- Average: 68ms (12.5x faster)
- P95: 95ms
- **95% reduction in query time**
- Additional storage: 15 MB

---

## Next Steps

### Core Documentation

- **[Metrics Collection](metrics-collection.md)** - What data is collected and how to use it

### Advanced Topics

- **[Optimization Suggestions](optimization-suggestions.md)** - Understanding output
- **[Migration Workflow](migration-workflow.md)** - Applying changes safely
- **[Troubleshooting](troubleshooting.md)** - Common issues and solutions

### Examples

- **[Basic Denormalization](examples/basic-denormalization.md)** - Simple JSONB → column
- **[Analytics Optimization](examples/analytics-optimization.md)** - Complex aggregates
- **[Production Deployment](examples/production-deployment.md)** - High-traffic setup

---

## Key Concepts

### Denormalization

Moving data from JSONB column to dedicated column for faster access:

```sql
-- Before: Slow JSONB extraction
dimensions->>'region'

-- After: Fast column lookup
region_id
```text

### Metrics Collection

Observability tracks:

- **Query timing**: Execution, SQL generation, projection
- **JSONB accesses**: Which paths, how often, selectivity
- **Database stats**: Row counts, cardinality, index usage

### Analysis Thresholds

Conservative defaults (configurable):

- **Frequency**: 1000+ queries/day
- **Speedup**: 5x+ improvement
- **Selectivity**: Filters that reduce result set

### Migration Safety

Multi-stage process:

1. Generate SQL
2. Review changes
3. Test in staging
4. Backup production
5. Apply migration
6. Monitor results

---

## FAQ

### Q: Does observability slow down my application?

**A**: Overhead is <5% with default 10% sampling. High-traffic applications can use 1% sampling (configurable).

### Q: What data is collected?

**A**: Only query structure, timing, and patterns. **Never** user data, query arguments, or PII.

### Q: Can I use this in production?

**A**: Yes! Observability is opt-in and production-safe. Many users run it 24/7 with sampling.

### Q: What if I don't want PostgreSQL metrics storage?

**A**: You can export metrics to JSON and analyze offline. DB connection is optional (but recommended for better accuracy).

### Q: Are suggestions automatically applied?

**A**: No. All suggestions require manual review and approval. This is intentional for safety.

### Q: What databases are supported?

**A**: Currently PostgreSQL (primary), MySQL, SQLite, and SQL Server. Metrics collection and analysis work on all; pg_stats integration is PostgreSQL-only.

---

## Getting Help

- **GitHub Issues**: [github.com/fraiseql/fraiseql/issues](https://github.com/fraiseql/fraiseql/issues)
- **Discord**: [discord.gg/fraiseql](https://discord.gg/fraiseql)
- **Docs**: [docs.fraiseql.com](https://docs.fraiseql.com)
- **Email**: <support@fraiseql.com>

---

## Contributing

Found an issue or want to improve the documentation?

1. Fork the repository
2. Make your changes
3. Submit a pull request

See [CONTRIBUTING.md](../../CONTRIBUTING.md) for guidelines.

---

## License

FraiseQL is open source under the Apache 2.0 License.

---

*Last updated: 2026-02-05*
