# fraiseql-wire Migration Guide

Guide for migrating existing code to use new operators and query modifiers (Phases 0-7).

## Overview

**Good News**: This is a **non-breaking change**. Your existing code continues to work unchanged.

The new operator system is purely additive - you can:

- Keep using `where_sql()` with raw SQL strings
- Gradually migrate to type-safe operators
- Mix both approaches in the same application

## What Changed

### Field Source Support

**Before**:

- All filters assumed JSONB fields
- Direct column filtering not well documented

**After**:

- Explicit field sources: `JsonbPayload` vs `DirectColumn`
- Automatic type casting for JSONB text extraction
- Mixed filtering support in single WHERE clause

### Query Modifiers

**Before**:

```rust
// Only raw ORDER BY strings
.order_by("data->>'name' ASC")
```

**After**:

```rust
// ORDER BY with collation support
.order_by("(data->>'name') COLLATE \"en-US\" ASC")

// LIMIT and OFFSET
.limit(10)
.offset(20)
```

### Phases 2-3: Operators

**Before**:

```rust
// Hard to remember correct SQL syntax
.where_sql("(data->>'status')::text = 'active'")
.where_sql("array_length((data->'tags')::text[], 1) > 3")
.where_sql("l2_distance((data->>'embedding')::vector, $1::vector) < 0.5")
```

**After**:

```rust
// Same raw SQL still works ✓

// OR use type-safe operators (future enhancement)
.where_operator(WhereOperator::Eq(
    Field::JsonbField("status".to_string()),
    Value::String("active".to_string())
))
```

## Migration Checklist

### ✅ No Changes Required

Your existing code automatically benefits from:

- Better type casting in JSONB filters
- LIMIT/OFFSET support
- ORDER BY with collation
- Proper field source awareness

**Just run your existing tests** - everything should work.

### 📋 Optional: Update `order_by()` Calls

If you use collation in GraphQL:

```rust
// ❌ Old style (still works)
.order_by("data->>'name' ASC")

// ✅ New style (with collation)
.order_by("(data->>'name') COLLATE \"en-US\" ASC")
```

**Why**: `COLLATE` clause ensures consistent, locale-aware string sorting.

### 🚀 Optional: Add LIMIT/OFFSET

```rust
// ✅ New: Add pagination
let page = 1;
let per_page = 20;

client
    .query("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .limit(per_page)
    .offset((page - 1) * per_page)
    .execute()
    .await?
```

## Examples: Before & After

### Example 1: Basic Filtering

**Before** (still works):

```rust
let results = client
    .query::<Project>("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .where_sql("(data->>'priority')::numeric >= 5")
    .execute()
    .await?;
```

**After** (enhanced):

```rust
let results = client
    .query::<Project>("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .where_sql("(data->>'priority')::numeric >= 5")
    .order_by("(data->>'priority')::numeric DESC") // Already worked
    .limit(20)  // ✨ NEW: Add pagination
    .execute()
    .await?;
```

### Example 2: Internationalization (i18n)

**Before** (no collation):

```rust
.order_by("data->>'name' ASC")  // ASCII-only sorting
```

**After** (with locale-aware collation):

```rust
// English US
.order_by("(data->>'name') COLLATE \"en-US\" ASC")

// German (respects ö, ä, ü)
.order_by("(data->>'name') COLLATE \"de-DE\" ASC")

// Binary (fastest)
.order_by("(data->>'name') COLLATE \"C\" ASC")
```

### Example 3: Pagination

**Before** (no pagination in fraiseql-wire):

```rust
// Had to use application-level pagination
let all = client
    .query::<Project>("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .execute()
    .await?;

// Get first 20
// ... collect in application
```

**After** (SQL-level pagination):

```rust
// SQL does the pagination
let page1 = client
    .query::<Project>("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .order_by("(data->>'name') ASC")
    .limit(20)
    .offset(0)
    .execute()
    .await?;

let page2 = client
    .query::<Project>("projects")
    .where_sql("(data->>'status')::text = 'active'")
    .order_by("(data->>'name') ASC")
    .limit(20)
    .offset(20)
    .execute()
    .await?;
```

### Example 4: Array Length

**Before** (remember correct casting syntax):

```rust
.where_sql("array_length((data->'tags')::text[], 1) > 3")
```

**After** (JSONB-aware casting):

```rust
.where_sql("jsonb_array_length(data->'tags') > 3")  // ✨ Cleaner for JSONB
```

### Example 5: Full-Text Search

**Before** (raw SQL):

```rust
.where_sql("(data->>'content') @@ plainto_tsquery('english', 'machine learning')")
```

**After** (same syntax, better documented):

```rust
.where_sql("(data->>'content') @@ plainto_tsquery('english', 'machine learning')")
// See docs/OPERATORS.md for full-text search guide
```

## Breaking Changes

**None**. ✅

All existing APIs remain unchanged:

- `where_sql()` works exactly as before
- `order_by()` accepts same syntax
- No changes to `execute()` or streaming
- No changes to error types or handling

## Compatibility Matrix

| Version | Feature | Status |
|---------|---------|--------|
| 0.1.0+ | `where_sql()` | ✅ Unchanged |
| 0.1.0+ | `order_by()` | ✅ Unchanged |
| 0.1.0+ | `where_rust()` | ✅ Unchanged |
| 0.1.0+ | Streaming | ✅ Unchanged |
| 0.1.0+ | Type-safe operators | ✨ NEW |
| 0.1.0+ | `limit()` | ✨ NEW |
| 0.1.0+ | `offset()` | ✨ NEW |
| 0.1.0+ | ORDER BY collation | ✨ NEW |
| 0.1.0+ | JSONB type casting | ✨ IMPROVED |

## FAQ

### Q: Will my existing code break?

**A**: No. All changes are backward compatible.

### Q: Do I need to update my code?

**A**: Only if you want to use new features (LIMIT, OFFSET, collation).

### Q: Should I migrate to type-safe operators?

**A**: If you're building a new GraphQL adapter, yes. For existing code, `where_sql()` works fine.

### Q: What's the performance difference?

**A**: Negligible. SQL generation happens at query time, not runtime.

### Q: Can I mix `where_sql()` and `where_operator()`?

**A**: Yes (future versions will support this). Currently use one or the other.

### Q: How do I choose between collations?

**A**:

- Use `C` for binary/fastest sorting
- Use `en-US` for English locale-aware (accents, case)
- Use `de-DE` for German (ö, ä, ü)
- Omit for ASCII-only (fastest, no locale)

### Q: Do vector operators require pgvector?

**A**: Yes. Install the pgvector extension on your PostgreSQL database.

### Q: How do I handle NULL values in ORDER BY?

**A**:

```rust
.order_by("(data->>'name') ASC NULLS LAST")  // NULLs at end
.order_by("(data->>'name') ASC NULLS FIRST") // NULLs at start
```

## Database Setup

No schema changes required. The views (v_users, v_projects, etc.) work as-is.

### Optional: Enable pgvector for Vector Operations

```bash
# Connect to your PostgreSQL database
psql -U postgres -d fraiseql_test

# Install pgvector extension
CREATE EXTENSION IF NOT EXISTS vector;

# Create indexes for vector operations
CREATE INDEX ON my_table USING ivfflat ((data->>'embedding')::vector vector_cosine_ops);
```

## Testing Your Migration

```rust
#[tokio::test]
async fn test_migration() {
    let client = FraiseClient::connect(TEST_DB_URL).await?;

    // Test 1: Existing code still works
    let results1 = client
        .query("projects")
        .where_sql("(data->>'status')::text = 'active'")
        .execute()
        .await?;
    assert!(!results1.is_empty());

    // Test 2: New LIMIT works
    let results2 = client
        .query("projects")
        .limit(5)
        .execute()
        .await?;
    assert!(results2.len() <= 5);

    // Test 3: New ORDER BY with collation works
    let results3 = client
        .query("projects")
        .order_by("(data->>'name') COLLATE \"en-US\" ASC")
        .execute()
        .await?;
    assert!(!results3.is_empty());

    Ok(())
}
```

## Next Steps

1. **Review Operators Guide**: Read `docs/OPERATORS.md` for complete operator reference
2. **Add Pagination**: Use `limit()` and `offset()` for GraphQL queries
3. **Update Collation**: Add locale-aware collation to `order_by()` for i18n
4. **Use Vector Search**: Try vector distance operators with pgvector extension
5. **Full-Text Search**: Implement PostgreSQL full-text search with `@@@` operators

## Support

- **Issues**: Report bugs on GitHub
- **Documentation**: See `docs/OPERATORS.md` for complete reference
- **Examples**: Check `tests/integration_operators.rs` for usage patterns

## Related Documentation

- [Operators Reference](OPERATORS.md)
- [Streaming Guide](STREAMING.md)
- [Query Builder API](../README.md)
