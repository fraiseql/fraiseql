<!-- Skip to main content -->
---
title: View Selection Quick Reference Card
description: - FraiseQL view naming conventions (v_*, va_*, tv_*, ta_*)
keywords: ["debugging", "implementation", "directives", "best-practices", "types", "deployment", "schema", "scalars"]
tags: ["documentation", "reference"]
---

# View Selection Quick Reference Card

**Status:** ✅ Production Ready
**Audience:** Developers, DBAs, Architects
**Reading Time:** 5-8 minutes
**Last Updated:** 2026-02-05

---

## Prerequisites

**Required Knowledge:**

- FraiseQL view naming conventions (v_*, va_*, tv_*, ta_*)
- GraphQL query complexity concepts
- JSON vs Arrow data plane differences
- Query performance expectations and latency targets
- Table-backed vs logical view trade-offs

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- No specific software required (reference guide)

**Required Infrastructure:**

- None (quick reference only - no implementation needed)

**Time Estimate:** 1-5 minutes to find the right view type for your use case

## TL;DR: Which View Should I Use?

### JSON Plane (GraphQL)

```text
<!-- Code example in TEXT -->
Simple query? → v_*
  Example: SELECT * FROM v_user WHERE id = ?
  Time: 50-100ms

Complex query (3+ JOINs)? → tv_*
  Example: User with posts, comments, likes
  Time: 100-200ms vs 2-5s without table-backed

Query slow (>1s)? → Migrate to tv_*
  Benefit: 10-50x faster
```text
<!-- Code example in TEXT -->

### Arrow Plane (Analytics)

```text
<!-- Code example in TEXT -->
Small dataset (<100K rows)? → va_*
  Example: Daily summary (10K rows)
  Time: 100-200ms

Large dataset (>1M rows)? → ta_*
  Example: 10M transactions, time-series
  Time: 50-100ms vs 5-10s without table-backed

Query slow (>1s)? → Migrate to ta_*
  Benefit: 50-100x faster
```text
<!-- Code example in TEXT -->

---

## Quick Decision Matrix

| Need | Plane | View | Setup Time | Storage | Query Time |
|------|-------|------|-----------|---------|-----------|
| User by ID | JSON | `v_user` | 5 min | None | 50-100ms |
| User + posts + comments | JSON | `tv_user_profile` | 30 min | +30% | 100-200ms |
| Daily sales (10K rows) | Arrow | `va_orders` | 10 min | None | 100-200ms |
| 10M transactions | Arrow | `ta_orders` | 45 min | +20% | 50-100ms |

---

## When to Use Each Pattern

### ✅ Use `v_*` when

- Single table or 1-2 table join
- Query already fast (<500ms)
- Query patterns vary (unpredictable)
- Storage is limited

### ✅ Use `tv_*` when

- 3+ table joins with JSONB composition
- Complex nesting (User → Posts → Comments → Likes)
- High read volume (>100 req/sec)
- Consistent query pattern
- GraphQL subscriptions need fast updates

### ✅ Use `va_*` when

- Analytics on small dataset (<100K rows)
- Query already fast (<1s)
- Query patterns vary
- Storage is limited

### ✅ Use `ta_*` when

- Analytics on large dataset (>1M rows)
- Time-series data with range queries
- Query time > 1 second
- Need sub-second response for dashboards
- Column-oriented operations (aggregations)

### ❌ Don't use table-backed views when

- Storage is severely constrained
- Write volume unpredictable (>1K/min writes)
- Query patterns are highly dynamic
- Data freshness requirements <100ms

---

## Performance Benchmarks

### JSON Plane (GraphQL)

| Query | v_* | tv_* | Winner | Speedup |
|-------|-----|------|--------|---------|
| Single user | 100ms | 120ms | v_* | - |
| User + 10 posts | 200ms | 150ms | tv_* | 1.3x |
| User + posts + comments | 2-3s | 150ms | tv_* | 15-20x |
| User + posts + comments + likes | 5-7s | 200ms | tv_* | 25-35x |

### Arrow Plane (Analytics)

| Dataset | va_* | ta_* | Winner | Speedup |
|---------|------|------|--------|---------|
| 10K rows, filter | 100ms | 100ms | Tie | - |
| 100K rows, range query | 500ms | 80ms | ta_* | 6x |
| 1M rows, aggregate | 2-3s | 100ms | ta_* | 20-30x |
| 10M rows, range + aggregate | 10-15s | 200-300ms | ta_* | 40-60x |

---

## Migration Decision Tree

### From v_*to tv_* (JSON)

```text
<!-- Code example in TEXT -->
Is query time > 1 second?
├─ YES → Migrate to tv_*
└─ NO ─→ Check read volume

Is read volume > 100 req/sec?
├─ YES → Migrate to tv_*
└─ NO ─→ Keep v_*

Are there 3+ table joins?
├─ YES → Migrate to tv_*
└─ NO ─→ Keep v_*
```text
<!-- Code example in TEXT -->

**Estimated benefit**: 10-50x faster queries

### From va_*to ta_* (Arrow)

```text
<!-- Code example in TEXT -->
Is dataset > 1M rows?
├─ YES → Migrate to ta_*
└─ NO ─→ Check query time

Is query time > 1 second?
├─ YES → Migrate to ta_*
└─ NO ─→ Keep va_*

Are queries doing aggregations on large ranges?
├─ YES → Migrate to ta_*
└─ NO ─→ Keep va_*
```text
<!-- Code example in TEXT -->

**Estimated benefit**: 50-100x faster queries

---

## Implementation Checklist

### Creating a tv_* (5 steps, ~30 min)

```sql
<!-- Code example in SQL -->
-- 1. Create intermediate composed views (reusable helpers)
CREATE VIEW v_user_posts_composed AS ...

-- 2. Create physical table
CREATE TABLE tv_user_profile (
    id TEXT PRIMARY KEY,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- 3. Add indexes
CREATE INDEX idx_tv_user_profile_data_gin ON tv_user_profile USING GIN(data);

-- 4. Create refresh trigger
CREATE TRIGGER trg_refresh_tv_user_profile ...

-- 5. Initial population
SELECT refresh_tv_user_profile();
```text
<!-- Code example in TEXT -->

**See**: `examples/sql/postgres/tv_user_profile.sql`

### Creating a ta_* (4 steps, ~30 min)

```sql
<!-- Code example in SQL -->
-- 1. Create physical table with extracted columns
CREATE TABLE ta_orders (
    id TEXT PRIMARY KEY,
    total NUMERIC,
    created_at TIMESTAMPTZ,
    ...
);

-- 2. Add BRIN index for time-series
CREATE INDEX idx_ta_orders_created_at_brin ON ta_orders USING BRIN(created_at);

-- 3. Create refresh trigger
CREATE TRIGGER trg_refresh_ta_orders ...

-- 4. Initial population
SELECT refresh_ta_orders();
```text
<!-- Code example in TEXT -->

**See**: `examples/sql/postgres/ta_orders.sql`

---

## Refresh Strategy Quick Pick

### For tv_* (JSON table-backed)

| Write Volume | Read Volume | Recommended |
|-------------|-------------|------------|
| Low (<100/min) | High | Trigger-based |
| Medium (100-1K/min) | High | Trigger + batch cleanup |
| High (>1K/min) | High | Scheduled (5-15 min) |

**Default**: Trigger-based (real-time, <100ms latency)

### For ta_* (Arrow table-backed)

| Dataset Size | Query Pattern | Recommended |
|-------------|--------------|------------|
| 1-10M rows | Time-series ranges | Trigger + BRIN index |
| 10-100M rows | Large aggregations | Scheduled (hourly/daily) |
| 100M+ rows | Heavy analytics | Batch refresh only |

**Default**: Scheduled batch (low overhead, acceptable staleness)

---

## Monitoring Essentials

### Check View Freshness

```sql
<!-- Code example in SQL -->
-- How old is the data?
SELECT MAX(updated_at) - NOW() as staleness FROM tv_user_profile;

-- Any stale profiles?
SELECT COUNT(*) FROM tv_user_profile WHERE updated_at < NOW() - INTERVAL '1 minute';
```text
<!-- Code example in TEXT -->

### Monitor Refresh Performance

```sql
<!-- Code example in SQL -->
-- How fast are refreshes?
EXPLAIN (ANALYZE) SELECT refresh_tv_user_profile();

-- Any slow triggers?
SELECT schemaname, tablename, idx_scan, idx_tup_read
FROM pg_stat_user_indexes
WHERE tablename LIKE 'tv_%' OR tablename LIKE 'ta_%';
```text
<!-- Code example in TEXT -->

### Verify Data Accuracy

```sql
<!-- Code example in SQL -->
-- Row counts match?
SELECT
    (SELECT COUNT(*) FROM tv_user_profile) as tv_count,
    (SELECT COUNT(*) FROM v_user) as v_count;

-- Totals match?
SELECT
    SUM(total) as tv_total FROM tv_order_summary
UNION
SELECT
    SUM(total) as v_total FROM v_order;
```text
<!-- Code example in TEXT -->

---

## Common Issues & Fixes

### Issue: Table-backed view empty after creation

**Fix**: Run initial population

```sql
<!-- Code example in SQL -->
SELECT refresh_tv_user_profile();
```text
<!-- Code example in TEXT -->

### Issue: Data is stale (not updating)

**Cause**: Trigger not firing
**Fix**: Manually refresh + check trigger status

```sql
<!-- Code example in SQL -->
SELECT * FROM refresh_tv_user_profile();
SELECT * FROM information_schema.triggers WHERE trigger_name LIKE 'trg_refresh%';
```text
<!-- Code example in TEXT -->

### Issue: High CPU from triggers

**Cause**: Too many writes + per-row refresh
**Fix**: Switch to scheduled batch

```sql
<!-- Code example in SQL -->
-- Drop per-row trigger
DROP TRIGGER trg_refresh_tv_user_profile_on_user ON tb_user;

-- Schedule batch instead
SELECT cron.schedule('refresh-tv-profile', '*/5 * * * *', 'SELECT refresh_tv_user_profile();');
```text
<!-- Code example in TEXT -->

### Issue: Query still slow after migration

**Cause**: Schema mismatch, missing index, or wrong view
**Fix**: Verify using EXPLAIN

```sql
<!-- Code example in SQL -->
EXPLAIN (ANALYZE, BUFFERS)
SELECT * FROM tv_user_profile WHERE id = ?;

-- Check indexes exist
SELECT * FROM pg_indexes WHERE tablename = 'tv_user_profile';
```text
<!-- Code example in TEXT -->

### Issue: Storage growing too fast

**Cause**: JSONB duplication + excess indexes
**Fix**: Review DDL + consider scheduled batch only

```sql
<!-- Code example in SQL -->
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as size
FROM pg_tables
WHERE tablename LIKE 'tv_%' OR tablename LIKE 'ta_%'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;
```text
<!-- Code example in TEXT -->

---

## Code Examples

### GraphQL Schema Binding

```python
<!-- Code example in Python -->
# Use logical view (default, fast enough)
@FraiseQL.type()
class User:
    id: UUID  # UUID v4 for GraphQL ID
    name: str

# Use table-backed view (complex nesting)
@FraiseQL.type(view="tv_user_profile")
class UserProfile:
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    posts: list[Post]
    comments: list[Comment]
```text
<!-- Code example in TEXT -->

### Arrow Flight Query

```python
<!-- Code example in Python -->
import pyarrow.flight as flight
import json

client = flight.connect("grpc://localhost:50051")

# Use logical view
ticket = {"view": "va_orders", "limit": 10000}
stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))

# Use table-backed view
ticket = {"view": "ta_orders", "limit": 1000000}
stream = client.do_get(flight.Ticket(json.dumps(ticket).encode()))
```text
<!-- Code example in TEXT -->

---

## Key Files Reference

| Purpose | File |
|---------|------|
| Detailed decision guide | `docs/architecture/database/view-selection-guide.md` |
| tv_* deep dive | `docs/architecture/database/tv-table-pattern.md` |
| ta_* deep dive | `docs/architecture/database/ta-table-pattern.md` |
| API patterns | `docs/api/view-selection-api.md` |
| tv_* example | `examples/sql/postgres/tv_user_profile.sql` |
| ta_* example | `examples/sql/postgres/ta_orders.sql` |
| Schema conventions | `docs/specs/schema-conventions.md` |

---

## Decision Flowchart

```text
<!-- Code example in TEXT -->
START: Do you have a performance problem?
  ├─ NO → Use v_* or va_*, move on
  └─ YES → Measure query time with EXPLAIN

Query time > 1 second?
  ├─ NO → Problem elsewhere (indexing, query logic)
  └─ YES → Continue

What plane?
  ├─ JSON (GraphQL) → Check joins
  │   ├─ <3 joins → Optimize query/indexes first
  │   └─ 3+ joins → Create tv_*
  │
  └─ Arrow (Analytics) → Check dataset size
      ├─ <1M rows → Optimize query/indexes first
      └─ >1M rows → Create ta_*

Create table-backed view:
  1. Design composition view
  2. Create physical table
  3. Add indexes
  4. Set up refresh (trigger or scheduled)
  5. Benchmark: Compare old vs new

Did query time improve >5x?
  ├─ YES → Deploy, monitor freshness
  └─ NO → Investigate other bottlenecks
```text
<!-- Code example in TEXT -->

---

## Troubleshooting Quick Reference

### "I'm not sure which view to use"

**Use the decision tree above.** Most common patterns:

- **Simple query (<3 joins)**: Use v_* (logical view)
- **Complex query (3+ joins)**: Use tv_* (materialized table)
- **Analytics query**: Use ta_* (Arrow columnar)
- **Query slow?**: Profile first (EXPLAIN ANALYZE), then consider tv_*

### "Table-backed view not faster than logical view"

**Diagnosis**: Run EXPLAIN on both and compare. Most likely cause: missing indexes.

**Solution**: Add index to base table on JOIN columns

```sql
<!-- Code example in SQL -->
CREATE INDEX idx_base_col ON base_table(col);
```text
<!-- Code example in TEXT -->

### "Materialized view refresh is blocking queries"

**Solution**: Use CONCURRENT refresh (PostgreSQL 9.5+) or schedule during low-traffic window

### "Arrow (ta_*) query still slow"

**Not ready for Arrow yet.** Prerequisites:

- Data >1M rows ✅
- Using ClickHouse backend ✅
- Query doesn't have subqueries ✅

If all true: Optimize query or increase ClickHouse resources.

### "I don't know current query performance"

**Measure first**: Run with v_* for one week, collect metrics

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:8000/graphql -d '{your_query}' \
  | jq '.extensions.timing'
```text
<!-- Code example in TEXT -->

Then compare after migration to tv_*.

---

## Related Guides

**Detailed Documentation**:

- [Full View Selection Guide](../architecture/database/view-selection-guide.md) — Comprehensive decision guide with all 4 patterns
- [tv_* Table Pattern](../architecture/database/tv-table-pattern.md) — Deep dive into JSON plane table-backed views
- [ta_* Table Pattern](../architecture/database/ta-table-pattern.md) — Deep dive into Arrow plane table-backed views
- [Schema Conventions](../specs/schema-conventions.md) — Database naming and structure conventions

**Supplementary Guides**:

- [Migration Checklist](./view-selection-migration-checklist.md) — Step-by-step workflow for v_→tv_ or va_→ta_ migrations
- [Performance Testing](./view-selection-performance-testing.md) — Methodology for benchmarking and validating improvements

**How to Use These Guides**:

1. **New to view selection?** Start here (Quick Reference) → then read Full View Selection Guide
2. **Ready to migrate?** Use Migration Checklist → benchmark with Performance Testing guide
3. **Deep technical details?** Read tv_*and ta_* patterns
4. **Setting up schema?** Refer to Schema Conventions
