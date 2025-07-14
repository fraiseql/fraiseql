# Production Mode JSONB Extraction Enhancement

**Date:** 2025-07-12
**Version:** 0.1.0b12+
**Feature:** Automatic JSONB data extraction in production mode

## Overview

FraiseQL now automatically extracts JSONB data in production mode, eliminating the need to use development mode just for JSONB extraction. This provides the best of both worlds: production mode's performance with correct GraphQL-compatible data structures.

## What Changed

### Before (v0.1.0b11 and earlier)
- Production mode returned entire database rows including metadata columns
- Users had to use development mode (10x slower) to get proper JSONB extraction
- Manual extraction in resolvers was required for production mode

### After (v0.1.0b12+)
- Production mode automatically detects and extracts JSONB `data` columns
- Returns just the JSONB content, matching GraphQL type expectations
- Maintains full backward compatibility with non-JSONB tables
- No performance penalty - still returns raw dicts without object instantiation

## How It Works

When querying a table/view with a JSONB `data` column:

```python
# Database row structure
{
    "id": "123",
    "tenant_id": "456",
    "created_at": "2025-07-12",
    "data": {  # JSONB column
        "id": "123",
        "name": "Product",
        "price": 99.99
    }
}

# Production mode now returns (automatically extracted)
{
    "id": "123",
    "name": "Product",
    "price": 99.99
}
```

## Usage

No code changes required! Just use production mode:

```python
# Via configuration
config = FraiseQLConfig(
    environment="production",  # Fast mode with JSONB extraction
    database_url=settings.database_url,
)

# Via environment variable
os.environ["FRAISEQL_ENV"] = "production"

# Query as normal
@fraiseql.query
async def products(info, limit: int = 10) -> list[Product]:
    db = info.context["db"]
    return await db.find("products_view", limit=limit)  # Returns extracted JSONB
```

## Benefits

1. **10x Performance Improvement**: No object instantiation overhead
2. **GraphQL Compatible**: Returns data matching type definitions
3. **Zero Configuration**: Works automatically with JSONB columns
4. **Backward Compatible**: Non-JSONB tables work as before

## Migration Guide

If you were using development mode just for JSONB extraction:

```python
# Old (slow)
config = FraiseQLConfig(
    environment="development",  # Only needed for JSONB extraction
)

# New (fast)
config = FraiseQLConfig(
    environment="production",  # Now handles JSONB extraction
)
```

If you had manual extraction workarounds:

```python
# Old workaround (remove this)
@fraiseql.query
async def products(info) -> list[dict]:
    db = info.context["db"]
    rows = await db.find("products_view")
    return [row["data"] for row in rows]  # Manual extraction

# New (automatic)
@fraiseql.query
async def products(info) -> list[Product]:
    db = info.context["db"]
    return await db.find("products_view")  # Automatic extraction
```

## Technical Details

- Detection: Checks if first row contains a `data` column
- Extraction: Returns `row["data"]` for each row
- Fallback: Returns full row if no `data` column exists
- Performance: No measurable overhead vs returning raw rows

## When to Use Each Mode

### Production Mode (with JSONB extraction)
- High-traffic APIs
- Simple data retrieval
- When you need maximum performance
- Standard JSONB-based tables

### Development Mode
- Complex business logic in type methods
- Runtime type validation needed
- Custom property getters/setters
- Development and debugging

## Best Practices

1. **Use production mode by default** for JSONB-based APIs
2. **Switch to development mode** only when you need object features
3. **Profile your specific use case** to verify performance gains
4. **Keep JSONB structure aligned** with GraphQL type definitions

## Compatibility

- Works with all existing JSONB-based tables/views
- No changes needed to GraphQL types or resolvers
- Backward compatible with non-JSONB tables
- Available in FraiseQL v0.1.0b12+
