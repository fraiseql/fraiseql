<!-- Skip to main content -->
---
title: SQL Projection Optimization Guide
description: FraiseQL automatically optimizes GraphQL queries by projecting only requested fields at the database level using SQL `jsonb_build_object()`. This reduces networ
keywords: []
tags: ["documentation", "reference"]
---

# SQL Projection Optimization Guide

**Version**: 2.0.0-a1
**Status**: Production Ready
**Performance Impact**: **42-55% latency reduction**

## Overview

FraiseQL automatically optimizes GraphQL queries by projecting only requested fields at the database level using SQL `jsonb_build_object()`. This reduces network payload and JSON deserialization overhead by 40-55%.

## Why This Matters

Traditional GraphQL servers fetch full objects from the database, then filter fields on the server. FraiseQL projects fields at the database level:

```text
<!-- Code example in TEXT -->
Traditional:     Database → Full JSON → Network → GraphQL Filtering
FraiseQL:        Database → Projected JSON → Network (smaller!)
```text
<!-- Code example in TEXT -->

### Performance Impact

Real-world measurements on PostgreSQL with 1M rows:

| Data Size | Before | After | Improvement |
|-----------|--------|-------|-------------|
| 100 rows | 161 µs | 93 µs | **42%** ⚡ |
| 1000 rows | 1.65 ms | 958 µs | **42%** ⚡ |
| 10,000 rows | 26.1 ms | 10.4 ms | **55%** ⚡ |

**Throughput improvement**: 1.67-1.78x faster element processing

## How It Works

When you request specific fields in a GraphQL query:

```graphql
<!-- Code example in GraphQL -->
query {
  users(limit: 100) {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

FraiseQL generates optimized SQL:

```sql
<!-- Code example in SQL -->
-- Before (full JSONB)
SELECT data FROM v_user LIMIT 100

-- After (projected fields only)
SELECT jsonb_build_object(
  'id', data->>'id',
  'name', data->>'name',
  'email', data->>'email'
) FROM v_user LIMIT 100
```text
<!-- Code example in TEXT -->

The database returns only the fields you need. Unused fields (like `metadata`, `created_at`, etc.) never leave the database.

## When Projection Applies

Projection **applies automatically** to:

- ✅ All GraphQL queries (automatically detected)
- ✅ Queries with nested fields (selects all dependencies)
- ✅ Queries with aliases
- ✅ Queries with fragments

Projection **does not apply** to:

- ❌ Queries with `__typename` (requires full object)
- ❌ Queries with introspection (`__schema`, `__type`)
- ❌ Raw SQL operations (explicit only)

## Configuration

### Enable (Default)

Projection is **enabled by default**. No configuration needed.

```rust
<!-- Code example in RUST -->
// Projection automatically applied
let results = executor.execute(query, variables).await?;
```text
<!-- Code example in TEXT -->

### Disable (For Debugging)

To disable projection and test with full JSONB:

**Environment variable**:

```bash
<!-- Code example in BASH -->
FRAISEQL_DISABLE_PROJECTION=true cargo run
```text
<!-- Code example in TEXT -->

**In code**:

```rust
<!-- Code example in RUST -->
// Note: execute_where_query() bypasses projection
// Use execute_with_projection(view, None, clause, limit) to disable
```text
<!-- Code example in TEXT -->

## Performance Characteristics

### Per-Operation Overhead

Field projection overhead is minimal and consistent:

| Operation | Latency | Variance |
|-----------|---------|----------|
| Single field | 563 ns | ±1 ns |
| 5 fields | 1.2 µs | ±1 ns |
| 10 fields | 1.5 µs | ±1 ns |
| 20 fields | 2.7 µs | ±1 ns |

**Pattern**: ~130ns per field, ultra-consistent performance

### Scaling Behavior

Projection scales linearly with data size:

```text
<!-- Code example in TEXT -->
Latency = 130ns × num_fields + 200ns base overhead
```text
<!-- Code example in TEXT -->

No exponential degradation even with complex queries.

### Memory Impact

Projection **reduces memory usage** by filtering unused fields:

- Network payload: **42-55% smaller**
- JSON deserialization: **proportionally faster**
- Cache efficiency: **better with smaller objects**

## Database Support

### PostgreSQL ✅ (Optimized)

Full optimization using `jsonb_build_object()`:

```text
<!-- Code example in TEXT -->
Improvement: 42-55% latency reduction
```text
<!-- Code example in TEXT -->

### MySQL, SQLite, SQL Server ⏳ (Fallback)

Currently falls back to fetching full objects and filtering server-side.

Estimated improvement when optimized: **30-50%** (database-specific optimizations pending)

### FraiseWire Protocol ⏳ (Streaming)

Streaming protocol handles projection via field selection.

Estimated improvement: **20-30%** (protocol-level optimization in progress)

## Troubleshooting

### Projection Not Working?

Check these in order:

1. **Database Support**: PostgreSQL is fully optimized. MySQL, SQLite, SQL Server fall back to server-side filtering.

   ```bash
<!-- Code example in BASH -->
   # Check which database you're using
   echo $DATABASE_URL
   ```text
<!-- Code example in TEXT -->

2. **Enable Logging**: See what SQL is generated

   ```bash
<!-- Code example in BASH -->
   RUST_LOG=fraiseql_core=debug cargo run
   ```text
<!-- Code example in TEXT -->

3. **Disable Temporarily**

   ```bash
<!-- Code example in BASH -->
   FRAISEQL_DISABLE_PROJECTION=true cargo run
   ```text
<!-- Code example in TEXT -->

### Performance Not Improving?

**Possible causes**:

1. **Network-bound queries** - Projection helps most when queries return many unused fields
   - Solution: Use projection in queries with selective field lists

2. **Small result sets** - Overhead is per-query, not per-row
   - Solution: Batch related queries together

3. **Unoptimized WHERE clauses** - Database may be spending time filtering
   - Solution: Add indexes on frequently filtered fields

4. **Field expansion** - Requesting nested objects defeats projection
   - Solution: Use fragments to be explicit about field needs

## Best Practices

### 1. Be Specific with Fields

✅ **Good** - Request only what you need:

```graphql
<!-- Code example in GraphQL -->
query {
  users {
    id
    name
    email
  }
}
```text
<!-- Code example in TEXT -->

❌ **Bad** - Force full object fetch:

```graphql
<!-- Code example in GraphQL -->
query {
  users {
    ...AllUserFields
  }
}

fragment AllUserFields on User {
  # All 50+ fields
}
```text
<!-- Code example in TEXT -->

### 2. Use Nested Queries When Needed

✅ **Good** - Separate queries for different use cases:

```graphql
<!-- Code example in GraphQL -->
# For list view (minimal fields)
query UserList {
  users { id, name }
}

# For detail view (all fields)
query UserDetail($id: ID!) {
  user(id: $id) { ...AllUserFields }
}
```text
<!-- Code example in TEXT -->

### 3. Monitor Query Performance

Use the logging output to verify projection is working:

```bash
<!-- Code example in BASH -->
RUST_LOG=fraiseql_core::runtime=debug cargo run
```text
<!-- Code example in TEXT -->

Look for in logs:

```text
<!-- Code example in TEXT -->
DEBUG fraiseql_core::runtime::executor: SQL with projection = jsonb_build_object(...)
```text
<!-- Code example in TEXT -->

### 4. Profile Your Queries

For production deployments:

```bash
<!-- Code example in BASH -->
# Capture query metrics
curl -H "X-Debug: true" http://localhost:3000/graphql \
  -d '{"query": "..."}'
```text
<!-- Code example in TEXT -->

Results show projection impact in response headers.

## Examples

### Example 1: Simple Query (Automatic Projection)

**GraphQL Query**:

```graphql
<!-- Code example in GraphQL -->
query {
  users(limit: 10) {
    id
    email
  }
}
```text
<!-- Code example in TEXT -->

**Generated SQL** (automatic):

```sql
<!-- Code example in SQL -->
SELECT jsonb_build_object(
  'id', data->>'id',
  'email', data->>'email'
) AS data FROM v_user LIMIT 10
```text
<!-- Code example in TEXT -->

**Result**: 42% latency reduction automatically

### Example 2: Complex Query (Nested Fields)

**GraphQL Query**:

```graphql
<!-- Code example in GraphQL -->
query {
  posts(limit: 100) {
    id
    title
    author {
      id
      name
    }
    comments {
      id
      text
    }
  }
}
```text
<!-- Code example in TEXT -->

**Generated SQL** (automatic):

```sql
<!-- Code example in SQL -->
SELECT jsonb_build_object(
  'id', data->>'id',
  'title', data->>'title',
  'author', jsonb_build_object(
    'id', data->'author'->>'id',
    'name', data->'author'->>'name'
  ),
  'comments', (
    SELECT jsonb_agg(
      jsonb_build_object(
        'id', elem->>'id',
        'text', elem->>'text'
      )
    ) FROM jsonb_array_elements(data->'comments') elem
  )
) AS data FROM v_post LIMIT 100
```text
<!-- Code example in TEXT -->

**Result**: 54% latency reduction on 100-row results

### Example 3: Disabling Projection (Debugging)

If you need full JSONB for debugging:

```bash
<!-- Code example in BASH -->
# Via environment
FRAISEQL_DISABLE_PROJECTION=true cargo run

# Via code (fetch full object)
// The adapter will use execute_where_query() internally
// which skips projection optimization
```text
<!-- Code example in TEXT -->

## Migration Guide

### From Unoptimized GraphQL Servers

If migrating from another GraphQL server:

1. **No changes required** - Projection is automatic
2. **Performance improves** - Same queries run 42-55% faster
3. **Behavior is identical** - Results are the same shape
4. **Rollback is safe** - Set `FRAISEQL_DISABLE_PROJECTION=true` if needed

### Testing

```bash
<!-- Code example in BASH -->
# Before: Record baseline performance
wrk -t4 -c100 -d30s http://localhost:3000/graphql \
  -s load.lua

# After: Compare results (should be 40-55% faster)
# No query changes needed!
```text
<!-- Code example in TEXT -->

## Technical Details

### Projection SQL Generation

Projection uses `PostgresProjectionGenerator`:

```rust
<!-- Code example in RUST -->
let generator = PostgresProjectionGenerator::new();
let fields = vec!["id".to_string(), "email".to_string()];
let sql = generator.generate_projection_sql(&fields)?;
// Returns: jsonb_build_object('id', data->>'id', 'email', data->>'email')
```text
<!-- Code example in TEXT -->

### Integration Point

Projection is integrated in the query executor:

**File**: `crates/FraiseQL-core/src/runtime/executor.rs`

```rust
<!-- Code example in RUST -->
// Automatically generates projection from requested fields
let projection_hint = if !plan.projection_fields.is_empty() {
    let generator = PostgresProjectionGenerator::new();
    let projection_sql = generator.generate_projection_sql(&plan.projection_fields)?;
    Some(SqlProjectionHint {
        database: "postgresql".to_string(),
        projection_template: projection_sql,
        estimated_reduction_percent: 50,
    })
} else {
    None
};

// Executes with projection (or falls back if not supported)
let results = self.adapter.execute_with_projection(
    sql_source,
    projection_hint.as_ref(),
    None,
    None,
).await?;
```text
<!-- Code example in TEXT -->

## FAQ

**Q: Does projection work with mutations?**
A: No, mutations return full objects for consistency. The server needs all fields for response building.

**Q: Can I force full object queries?**
A: Yes, request all fields explicitly or use `FRAISEQL_DISABLE_PROJECTION=true`.

**Q: What about subscriptions?**
A: Subscriptions use projection automatically for the selected fields only.

**Q: How much network bandwidth is saved?**
A: 42-55% reduction in network payload on average. Larger savings with selective field lists (10 of 50 fields = 80% savings).

**Q: Is projection cached?**
A: Yes! Cached results include the projection, so repeated queries benefit from both caching AND projection.

**Q: Performance on other databases?**
A: Currently PostgreSQL is fully optimized. MySQL/SQLite/SQL Server use server-side projection (still faster than alternatives, but not as optimized). We're working on database-specific optimizations.

## Support Matrix

| Feature | PostgreSQL | MySQL | SQLite | SQL Server |
|---------|-----------|-------|--------|-----------|
| Automatic projection | ✅ Yes | ⏳ Soon | ⏳ Soon | ⏳ Soon |
| Server-side fallback | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Caching with projection | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Performance improvement | 42-55% | 30-50%* | 30-50%* | 30-50%* |

*Estimated when database-specific optimizations are added

## Next Steps

1. **Monitor your queries** - Use logging to see projection in action
2. **Profile your workloads** - Measure improvement in your use case
3. **Report issues** - If you find projection not working as expected
4. **Provide feedback** - Let us know about additional database support needs

---

**Learn More**:

- [Performance Baselines](./projection-baseline-results.md) - Detailed benchmark data
- [Architecture](../architecture/) - How FraiseQL works under the hood
