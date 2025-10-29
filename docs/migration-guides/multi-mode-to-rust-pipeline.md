# Migration Complete: Exclusive Rust Pipeline

# Migration Guide: Multi-Mode to Exclusive Rust Pipeline

**Status**: ✅ Migration completed - FraiseQL now exclusively uses Rust pipeline for all queries.

## Overview

This guide helps you migrate from FraiseQL's legacy multi-mode execution system (NORMAL, PASSTHROUGH, TURBO modes) to the current exclusive Rust pipeline architecture.

### What Changed

**Before (v0.11.4 and earlier):**
```
Query → Mode Selection → Python Processing → Response
                                      ↓
                             (NORMAL/PASSTHROUGH/TURBO)
```

**After (v1.0.0+):**
```
Query → Rust Pipeline → Response
```

### Key Benefits
- **7-10x faster** JSON transformation
- **Zero-copy** HTTP responses
- **Consistent behavior** across all queries
- **Simplified configuration** (no mode selection)

## Migration Steps

### Step 1: Update Dependencies

Ensure you're using FraiseQL v0.11.5 or later:

```bash
pip install --upgrade fraiseql>=0.11.5
```

The Rust pipeline requires `fraiseql-rs` which is included as a dependency.

### Step 2: Remove Mode-Specific Configuration

**Remove these from your code:**

```python
# ❌ OLD: Mode-specific configuration
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL"),
    execution_mode=ExecutionMode.TURBO,  # Remove this
    enable_turbo_mode=True,              # Remove this
    passthrough_mode=False,              # Remove this
)

# ❌ OLD: Mode selection in context
repo = FraiseQLRepository(pool, context={"mode": "turbo"})
```

**Replace with:**

```python
# Simplified configuration
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL")
    # Rust pipeline always active, no additional config needed
)
```

### Step 3: Update Repository Usage

**GraphQL Queries (Most Common):**
No changes needed! GraphQL queries work exactly the same:

```python
# ✅ Works unchanged
result = await repo.find("v_user", where={"name": {"eq": "John"}})
# Returns RustResponseBytes - FastAPI handles this automatically
```

**Direct Repository Access (Tests/Custom Code):**
Update code that accesses repository results directly:

```python
# ❌ OLD: Expected Python objects
result = await repo.find("v_user")
assert isinstance(result, list)  # Fails - now RustResponseBytes
assert result[0].name == "John"  # Fails - no longer Product instances

# Handle RustResponseBytes
import json
from fraiseql.core.rust_pipeline import RustResponseBytes

result = await repo.find("v_user")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
    users = data["data"]["v_user"]  # Note: field name matches query
else:
    users = result  # Fallback for compatibility

# For assertions:
assert isinstance(users, list)
assert users[0]["firstName"] == "John"  # Note: camelCase field names
```

### Step 4: Update Test Assertions

**Field Name Changes:**
- Database fields: `first_name` → GraphQL fields: `firstName`
- Boolean fields: `is_active` → `isActive`

```python
# ❌ OLD: Python field names
assert user.first_name == "John"
assert user.is_active is True

# GraphQL camelCase field names
assert user["firstName"] == "John"
assert user["isActive"] is True
```

**Return Type Changes:**
```python
# ❌ OLD: Direct list/dict returns
result = await repo.find("users")
assert isinstance(result, list)

# RustResponseBytes wrapper
result = await repo.find("users")
assert isinstance(result, RustResponseBytes)

# Extract data for testing:
from tests.unit.utils.test_response_utils import extract_graphql_data
users = extract_graphql_data(result, "users")
assert isinstance(users, list)
```

### Step 5: Update WHERE Clause Handling

WHERE clauses work the same, but results are now always in GraphQL format:

```python
# ✅ Works unchanged
where = ProductWhere(price={"gt": 50})
result = await repo.find("products", where=where)

# Extract for testing:
products = extract_graphql_data(result, "products")
expensive_products = [p for p in products if p["price"] > 50]
```

## Troubleshooting

### "RustResponseBytes has no attribute X"

**Problem:** Code expects Python objects but gets `RustResponseBytes`.

**Solution:**
```python
from fraiseql.core.rust_pipeline import RustResponseBytes
import json

result = await repo.find("users")
if isinstance(result, RustResponseBytes):
    data = json.loads(bytes(result.bytes))
    users = data["data"]["users"]
else:
    users = result
```

### "KeyError: 'firstName'" or "AttributeError: 'first_name'"

**Problem:** Using database field names instead of GraphQL field names.

**Solution:** Use camelCase GraphQL field names:
```python
# Database: first_name, is_active
# GraphQL: firstName, isActive

user = users[0]
assert user["firstName"] == "John"    # ✅ Correct
assert user["first_name"] == "John"  # ❌ Wrong
```

### "TypeError: object of type 'RustResponseBytes' has no len()"

**Problem:** Calling `len()` directly on repository results.

**Solution:** Extract data first:
```python
result = await repo.find("users")
users = extract_graphql_data(result, "users")
assert len(users) > 0  # ✅ Now works
```

### Performance Issues

**Problem:** Queries seem slower after migration.

**Solution:** The Rust pipeline should be faster. Check:
1. You're using FraiseQL v1.0.0+
2. No Python post-processing of results
3. Using `RustResponseBytes` directly with FastAPI

### Import Errors

**Problem:** `ImportError: fraiseql_rs not found`

**Solution:** Install with Rust dependencies:
```bash
pip install fraiseql  # Includes fraiseql-rs
# OR
pip install fraiseql-rs  # Direct install
```

## Code Examples

### Complete Migration Example

**Before:**
```python
from fraiseql import FraiseQLConfig, ExecutionMode
from fraiseql.db import FraiseQLRepository

config = FraiseQLConfig(
    database_url="postgresql://...",
    execution_mode=ExecutionMode.TURBO
)

repo = FraiseQLRepository(pool, context={"mode": "turbo"})
result = await repo.find("users", where={"status": {"eq": "active"}})

# Result was Python list
for user in result:
    print(f"{user.first_name} - {user.email}")
```

**After:**
```python
from fraiseql import FraiseQLConfig
from fraiseql.db import FraiseQLRepository
from tests.unit.utils.test_response_utils import extract_graphql_data

config = FraiseQLConfig(database_url="postgresql://...")

repo = FraiseQLRepository(pool)
result = await repo.find("users", where={"status": {"eq": "active"}})

# Result is RustResponseBytes - extract for processing
users = extract_graphql_data(result, "users")

# Use GraphQL field names
for user in users:
    print(f"{user['firstName']} - {user['email']}")
```

### Test Migration Example

**Before:**
```python
async def test_user_creation(repo):
    result = await repo.find("users")
    assert len(result) == 1
    assert result[0].first_name == "Test User"
```

**After:**
```python
async def test_user_creation(repo):
    from tests.unit.utils.test_response_utils import extract_graphql_data

    result = await repo.find("users")
    users = extract_graphql_data(result, "users")

    assert len(users) == 1
    assert users[0]["firstName"] == "Test User"
```

## Validation Checklist

- [ ] Updated to FraiseQL v1.0.0+
- [ ] Removed all `ExecutionMode` references
- [ ] Removed mode-specific configuration
- [ ] Updated test assertions to use `extract_graphql_data`
- [ ] Changed field names to camelCase
- [ ] Verified GraphQL queries work unchanged
- [ ] Checked performance is improved (should be 7-10x faster)

## Need Help?

If you encounter issues not covered here:

1. Check the [troubleshooting section](#troubleshooting) above
2. Review the [Rust Pipeline Integration Guide](../core/rust-pipeline-integration.md)
3. Search existing GitHub issues
4. Create a new issue with your migration problem

The Rust pipeline provides significant performance improvements - this migration is worth the effort!

## Migration Status: Complete ✅

All migration steps have been completed. FraiseQL now exclusively uses the Rust pipeline:

- ✅ Configuration updated - no execution mode options
- ✅ Tests updated - handle RustResponseBytes consistently
- ✅ Legacy code removed - no mode-specific logic needed
- ✅ Field names unified - always camelCase from Rust pipeline

## Performance Benefits

FraiseQL's exclusive Rust pipeline delivers consistent high performance:

- **All queries**: 0.5-5ms response times
- **7-10x faster** than legacy Python-based execution
- **Consistent performance** - no mode switching overhead

## Historical Issues (Resolved)

If you encounter any legacy issues, they indicate incomplete migration. The current FraiseQL version handles all these automatically through the exclusive Rust pipeline.

## Current Architecture

- See [Rust Pipeline Overview](../rust/RUST_FIRST_PIPELINE.md)
- All queries now use exclusive Rust pipeline execution
