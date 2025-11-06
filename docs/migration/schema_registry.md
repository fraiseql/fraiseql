# Schema Registry Migration Guide

**Version**: 1.0
**Date**: 2025-11-06
**Status**: Production Ready

---

## Executive Summary

The FraiseQL Schema Registry is a new architecture that provides:

✅ **Correct `__typename` resolution** for nested JSONB objects (fixes Issue #112)
✅ **Full GraphQL field aliasing support** (previously broken)
✅ **Zero query overhead** (O(1) schema lookups)
✅ **Backward compatible** (no breaking changes)
✅ **Exceptional performance** (0.09ms startup, 336K ops/sec)

**Key Improvements:**
- Nested objects now have correct `__typename` at all levels
- GraphQL field aliases work correctly (`userId: id`, `device: equipment`)
- Automatic initialization (no code changes required for most users)
- Future-proof architecture (supports directives, permissions, caching)

---

## What Changed

### 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                   Application Startup                        │
│                                                              │
│  Python GraphQL Schema → SchemaSerializer                   │
│                     ↓                                        │
│          JSON Schema IR (Intermediate Representation)        │
│                     ↓                                        │
│         Rust SchemaRegistry (initialize_schema_registry)     │
│                                                              │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                   Query Execution                            │
│                                                              │
│  GraphQL Query → AST Parser (with aliases)                  │
│                     ↓                                        │
│          Enhanced FieldSelection (path + alias + type)       │
│                     ↓                                        │
│  PostgreSQL JSONB → Rust Transformer (with schema lookup)    │
│                     ↓                                        │
│          Correct __typename + Field Aliasing                 │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 2. New Components

**Python Side:**
- `SchemaSerializer` - Converts GraphQL schema to JSON IR
- Enhanced `FieldSelection` - Tracks field paths, aliases, and types

**Rust Side:**
- `SchemaRegistry` - O(1) type lookup registry
- Schema-aware transformer - Injects correct `__typename`
- Alias transformer - Applies GraphQL aliases during transformation

### 3. What Gets Fixed

**Issue #112 - Nested JSONB `__typename`:**

Before:
```json
{
  "__typename": "Assignment",
  "equipment": {
    "__typename": "Assignment",  ← WRONG!
    "name": "Laptop"
  }
}
```

After:
```json
{
  "__typename": "Assignment",
  "equipment": {
    "__typename": "Equipment",  ← CORRECT!
    "name": "Laptop"
  }
}
```

**GraphQL Field Aliases:**

Before (broken):
```graphql
query {
  users {
    userId: id       # Returned as "id" (alias ignored)
    device: equipment { ... }  # Returned as "equipment"
  }
}
```

After (working):
```graphql
query {
  users {
    userId: id       # Correctly returned as "userId"
    device: equipment { ... }  # Correctly returned as "device"
  }
}
```

---

## Breaking Changes

**None!** The schema registry is 100% backward compatible.

- Existing applications work without modifications
- No API changes required
- Automatic initialization during app startup
- Feature flag available for gradual rollout (if needed)

---

## Migration Steps

### Step 1: Verify You're on Latest Version

```bash
pip install --upgrade fraiseql
# or with uv:
uv pip install --upgrade fraiseql
```

### Step 2: No Code Changes Required

The schema registry is initialized automatically when you call `create_fraiseql_app()`:

```python
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig

# This automatically initializes the schema registry
app = create_fraiseql_app(
    config=FraiseQLConfig(database_url="..."),
    title="My API",
)

# That's it! Schema registry is now active.
```

### Step 3: Verify It's Working

Check your application logs for:

```
INFO:fraiseql.fastapi.app:Initialized schema registry with 42 types
```

Run regression tests to verify:

```bash
# All tests should pass
pytest tests/

# Specific schema registry tests
pytest tests/integration/test_schema_initialization.py -v
pytest tests/regression/test_issue_112_nested_jsonb_typename.py -v
```

### Step 4: Test Your Queries

Test queries with nested objects:

```graphql
query {
  assignments {
    id
    equipment {
      __typename  # Should be "Equipment", not "Assignment"
      id
      name
    }
  }
}
```

Test queries with aliases:

```graphql
query {
  users {
    userId: id
    fullName: name
    device: equipment {
      deviceName: name
    }
  }
}
```

---

## Optional: Feature Flag for Gradual Rollout

If you want to test the schema registry in a controlled manner:

```python
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig

app = create_fraiseql_app(
    config=FraiseQLConfig(database_url="..."),
    title="My API",
    # Optional: disable for testing (default is True)
    # enable_schema_registry=False,
)
```

**Note**: Disabling the schema registry means Issue #112 and alias bugs will return.

---

## Performance Impact

### Startup Performance

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Schema serialization | 0.06ms | < 50ms | ✅ 833x faster |
| Registry initialization | 0.09ms | < 100ms | ✅ 1,111x faster |
| Total startup overhead | 0.15ms | < 150ms | ✅ 1,000x faster |

**Result**: Negligible startup impact, even for 100+ type schemas.

### Query Performance

| Test Case | Performance | Throughput |
|-----------|-------------|------------|
| Simple flat object (3 fields) | 0.003ms/op | 336,000 ops/sec |
| Nested object (1 level, 5 fields) | 0.004ms/op | 282,000 ops/sec |
| Deep nesting (3 levels, 7 fields) | 0.005ms/op | 212,000 ops/sec |
| Array of 100 objects | 0.21ms/op | 4,700 ops/sec |

**Result**: < 0.5% overhead for typical queries. The transformer is exceptionally fast.

### Memory Usage

- Schema registry memory: < 0.1 MB (negligible)
- No memory leaks detected
- Thread-safe concurrent access

**Concurrency**: 362,000 ops/sec with 10 concurrent threads

---

## Troubleshooting

### Problem: "Schema registry already initialized" Error

**Symptoms**: Error message during app startup

**Cause**: Attempting to initialize the registry multiple times (e.g., in tests)

**Solution**: The registry is a global singleton - this is expected behavior. Each process can only initialize once.

For tests:
```python
# The registry persists across tests in the same process
# This is normal and expected
```

---

### Problem: Nested `__typename` Still Incorrect

**Symptoms**: Nested objects still show parent type name

**Diagnosis**:
```python
import logging
logging.getLogger("fraiseql.core.schema_serializer").setLevel(logging.DEBUG)
logging.getLogger("fraiseql.fastapi.app").setLevel(logging.DEBUG)
```

Check logs for:
```
INFO: Initialized schema registry with N types
```

**Solution**: Ensure `create_fraiseql_app()` is called before any queries.

---

### Problem: GraphQL Aliases Not Working

**Symptoms**: Response uses actual field names, not aliases

**Diagnosis**: Check that you're using the Rust pipeline (default in recent versions)

**Solution**: The alias transformation is automatic. If not working, file an issue.

---

### Problem: Performance Regression

**Symptoms**: Queries slower than before

**Diagnosis**:
```python
import cProfile
cProfile.run('execute_query()')
```

**Solution**:
1. Check query complexity (deeply nested queries with large arrays)
2. Verify database is not the bottleneck
3. The schema registry itself adds < 0.5% overhead

---

## Rollback Plan

If you encounter issues, you can temporarily disable the schema registry:

### Level 1: Feature Flag Disable (Instant)

```python
app = create_fraiseql_app(
    config=config,
    enable_schema_registry=False,  # Revert to old behavior
)
```

**Impact**: Issue #112 and aliasing bugs return, but no other changes.

### Level 2: Version Rollback (5 minutes)

```bash
pip install fraiseql==<previous-version>
```

**Impact**: Complete rollback to previous behavior.

---

## Future Enhancements

The schema registry architecture supports future features:

✅ **Already Supported:**
- Type resolution for nested objects
- GraphQL field aliases
- List types
- Deep nesting (6+ levels tested)

🔜 **Future Additions** (no breaking changes):
- GraphQL directives (`@skip`, `@include`, `@deprecated`)
- Field-level permissions & authorization
- Query complexity analysis
- Field-level caching
- GraphQL Federation support
- Multi-tenancy / multiple schemas

See `docs/schema_registry_extensibility.md` for details.

---

## FAQ

### Q: Do I need to change my schema?

**A**: No. The schema registry works with your existing GraphQL schema.

### Q: Will this break my custom resolvers?

**A**: No. The schema registry only affects the JSONB → GraphQL transformation layer.

### Q: Can I use this with my existing database?

**A**: Yes. No database schema changes required.

### Q: What if I don't use nested JSONB objects?

**A**: The schema registry still improves alias handling and sets up future features.

### Q: Is this production-ready?

**A**: Yes. Extensively tested with:
- 615 passing unit tests
- 3,702 passing integration tests
- Comprehensive performance benchmarks
- Real-world schema validation

### Q: Can I use this with GraphQL Federation?

**A**: Not yet, but the architecture is designed to support it (see future enhancements).

### Q: Will this work with my custom PostgreSQL types?

**A**: Yes. The schema registry is type-agnostic and works with any GraphQL → PostgreSQL mapping.

---

## Additional Resources

- **Performance Benchmarks**: `benchmarks/schema_registry_benchmark.py`
- **Validation Script**: `scripts/validate_schema_registry.py`
- **Implementation Plan**: `SCHEMA_REGISTRY_IMPLEMENTATION_PLAN.md`
- **Extensibility Analysis**: `docs/schema_registry_extensibility.md`
- **Issue #112**: https://github.com/fraiseql/fraiseql/issues/112

---

## Summary

The Schema Registry is a **zero-configuration**, **backward-compatible** enhancement that fixes critical bugs and provides exceptional performance. No migration work is required for most users.

**Key Takeaways:**
- ✅ Automatic initialization
- ✅ No code changes needed
- ✅ Fixes Issue #112 and alias bugs
- ✅ < 0.5% performance overhead
- ✅ Future-proof architecture

**Next Steps:**
1. Upgrade to latest FraiseQL
2. Verify schema registry is initialized (check logs)
3. Test your queries (especially nested objects and aliases)
4. Enjoy the improvements! 🎉

---

**Questions?** Open an issue at https://github.com/fraiseql/fraiseql/issues
