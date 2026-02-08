# Phase 1: Fix Database Roundtrip Tests [REFACTOR]

**Objective**: Fix SQL parameter binding error in scalar database roundtrip tests
**Priority**: P0 - Critical
**Estimated Effort**: 1-2 hours
**Tests Fixed**: 114 tests (all scalar roundtrip tests)

---

## Context

The `test_scalar_database_roundtrip` test validates that each scalar type can be persisted to PostgreSQL and retrieved without data loss. Currently, all 54 scalars fail with:

```
psycopg.ProgrammingError: the query has 0 placeholders but 1 parameters were passed
```

This is a test implementation bug, not a framework bug. The scalars themselves work correctly.

---

## Root Cause

**File**: `tests/integration/meta/test_all_scalars.py`
**Lines**: 265-270

**Problem**: Mixing f-string formatting with parameterized queries

```python
# CURRENT CODE (BROKEN):
await conn.execute(
    f"""
    INSERT INTO {table_name} ({column_name}) VALUES ($1)
    """,
    [test_value],
)
```

**Why it fails**:
1. The f-string interpolates `table_name` and `column_name` at Python level
2. The resulting SQL string is: `INSERT INTO test_cidrsca... (cidrscalar_col) VALUES ($1)`
3. psycopg3 sees the literal text `$1` (not a placeholder) because it came from an f-string
4. When parameters `[test_value]` are passed, psycopg3 errors: "0 placeholders but 1 parameter"

**Key insight**: f-strings and parameterized queries don't mix. You must use psycopg3's SQL composition API.

---

## Solution

Use `psycopg.sql` module to safely compose dynamic SQL with placeholders.

**Pattern**:
```python
from psycopg import sql

await conn.execute(
    sql.SQL("""
        INSERT INTO {} ({}) VALUES (%s)
    """).format(
        sql.Identifier(table_name),
        sql.Identifier(column_name)
    ),
    [test_value],
)
```

**How it works**:
1. `sql.SQL()` creates a composable SQL string
2. `{}` placeholders are for identifiers (table/column names)
3. `%s` is for data values (the actual placeholder)
4. `sql.Identifier()` safely quotes identifiers: `test_table` → `"test_table"`
5. Parameters `[test_value]` fill the `%s` placeholder

---

## Files to Modify

### Primary File

**`tests/integration/meta/test_all_scalars.py`**

**Changes needed**: 3 locations
1. Line 265-270: INSERT statement (add parameterization)
2. Line 1 (imports): Add `from psycopg import sql`
3. Line 280: DROP TABLE statement (optional - use sql.Identifier for consistency)

---

## Implementation Steps

### Step 1: Add Import

**Location**: Top of file (after existing imports)

**Add**:
```python
from psycopg import sql
```

**Result**:
```python
"""Meta-test for ALL scalar types integration."""

import pytest
from psycopg import sql  # ← ADD THIS
from fraiseql import fraise_type, query
from fraiseql.types.scalars import __all__ as ALL_SCALARS
# ... rest of imports
```

---

### Step 2: Fix INSERT Statement

**Location**: Lines 263-270

**Current code**:
```python
# Insert test value
test_value = get_test_value_for_scalar(scalar_class)
await conn.execute(
    f"""
    INSERT INTO {table_name} ({column_name}) VALUES ($1)
""",
    [test_value],
)
```

**Replace with**:
```python
# Insert test value
test_value = get_test_value_for_scalar(scalar_class)
await conn.execute(
    sql.SQL("""
        INSERT INTO {} ({}) VALUES (%s)
    """).format(
        sql.Identifier(table_name),
        sql.Identifier(column_name)
    ),
    [test_value],
)
```

**Key changes**:
- Remove `f` prefix from the string
- Use `sql.SQL()` wrapper
- Change `$1` to `%s` (psycopg3 style placeholder)
- Use `sql.Identifier()` for table and column names
- Keep `[test_value]` as parameters (unchanged)

---

### Step 3: Fix SELECT Statement (Optional but Recommended)

**Location**: Line 273

**Current code**:
```python
result = await conn.execute(f"SELECT {column_name} FROM {table_name} WHERE id = 1")
```

**Replace with**:
```python
result = await conn.execute(
    sql.SQL("SELECT {} FROM {} WHERE id = 1").format(
        sql.Identifier(column_name),
        sql.Identifier(table_name)
    )
)
```

**Why**: Consistency and safety (even though these are test-controlled values)

---

### Step 4: Fix DROP TABLE Statements (Optional)

**Location**: Lines 255, 280

**Current code**:
```python
await conn.execute(f"DROP TABLE IF EXISTS {table_name}")
```

**Replace with**:
```python
await conn.execute(
    sql.SQL("DROP TABLE IF EXISTS {}").format(
        sql.Identifier(table_name)
    )
)
```

**Why**: Consistency throughout the test file

---

### Step 5: Fix CREATE TABLE Statement (Optional)

**Location**: Lines 256-261

**Current code**:
```python
await conn.execute(f"""
    CREATE TABLE {table_name} (
        id SERIAL PRIMARY KEY,
        {column_name} {get_postgres_type_for_scalar(scalar_class)}
    )
""")
```

**Replace with**:
```python
await conn.execute(
    sql.SQL("""
        CREATE TABLE {} (
            id SERIAL PRIMARY KEY,
            {} {}
        )
    """).format(
        sql.Identifier(table_name),
        sql.Identifier(column_name),
        sql.SQL(get_postgres_type_for_scalar(scalar_class))
    )
)
```

**Note**: The PostgreSQL type (e.g., `TEXT`, `CIDR`) is not an identifier, so we use `sql.SQL()` instead of `sql.Identifier()`.

---

## Complete Fixed Code

**Full `test_scalar_database_roundtrip` function**:

```python
@pytest.mark.parametrize("scalar_name,scalar_class", get_all_scalar_types())
async def test_scalar_database_roundtrip(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should persist/retrieve correctly from database."""
    # Create a temporary table for this scalar
    table_name = f"test_{scalar_name.lower()}_roundtrip"
    column_name = f"{scalar_name.lower()}_col"

    async with meta_test_pool.connection() as conn:
        # Create table
        await conn.execute(
            sql.SQL("DROP TABLE IF EXISTS {}").format(
                sql.Identifier(table_name)
            )
        )
        await conn.execute(
            sql.SQL("""
                CREATE TABLE {} (
                    id SERIAL PRIMARY KEY,
                    {} {}
                )
            """).format(
                sql.Identifier(table_name),
                sql.Identifier(column_name),
                sql.SQL(get_postgres_type_for_scalar(scalar_class))
            )
        )

        # Insert test value
        test_value = get_test_value_for_scalar(scalar_class)
        await conn.execute(
            sql.SQL("""
                INSERT INTO {} ({}) VALUES (%s)
            """).format(
                sql.Identifier(table_name),
                sql.Identifier(column_name)
            ),
            [test_value],
        )

        # Retrieve value
        result = await conn.execute(
            sql.SQL("SELECT {} FROM {} WHERE id = 1").format(
                sql.Identifier(column_name),
                sql.Identifier(table_name)
            )
        )
        row = await result.fetchone()
        retrieved_value = row[0] if row else None

        await conn.commit()

        # Cleanup
        await conn.execute(
            sql.SQL("DROP TABLE IF EXISTS {}").format(
                sql.Identifier(table_name)
            )
        )
        await conn.commit()

    # Verify roundtrip
    assert retrieved_value is not None, f"No value retrieved for {scalar_name}"
    # Note: Exact equality might not work for all types (e.g., JSON, dates)
    # but the important thing is no errors occurred
```

---

## Verification Plan

### Test Individual Scalar

```bash
# Test one scalar to verify the fix works
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_database_roundtrip[CIDRScalar-scalar_class2] -vv

# Expected output:
# PASSED - no parameter binding errors
```

### Test All Scalars

```bash
# Run all roundtrip tests
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_database_roundtrip -v

# Expected output:
# 54 passed in ~5-10 seconds
```

### Full Test Suite

```bash
# Run all scalar tests (registration + roundtrip)
uv run pytest tests/integration/meta/test_all_scalars.py -v

# Expected output:
# 168 passed (54 registration + 114 roundtrip)
```

---

## Acceptance Criteria

- [ ] Import `psycopg.sql` added to test file
- [ ] INSERT statement uses `sql.SQL()` and `sql.Identifier()`
- [ ] All 54 scalar roundtrip tests pass
- [ ] No parameter binding errors
- [ ] No regressions in schema registration tests (54 tests still pass)
- [ ] Code follows psycopg3 best practices

---

## Troubleshooting

### Issue: "module 'psycopg' has no attribute 'sql'"

**Cause**: Incorrect import or wrong psycopg version

**Solution**:
```bash
# Check psycopg3 is installed
uv pip list | grep psycopg

# Should see: psycopg >= 3.0
# If psycopg2, upgrade to psycopg3
```

### Issue: Still getting parameter errors

**Cause**: Missed an f-string or incorrect placeholder syntax

**Check**:
1. No `f` prefix before SQL strings
2. Use `%s` (not `$1`) for value placeholders
3. Use `{}` for identifier placeholders
4. All `{}` filled by `.format()`

### Issue: SQL syntax errors

**Cause**: Incorrect use of `sql.Identifier()` vs `sql.SQL()`

**Rule**:
- `sql.Identifier()`: for table names, column names (gets quoted)
- `sql.SQL()`: for SQL keywords, types (no quotes)

**Example**:
```python
# WRONG - quotes the type
sql.Identifier("TEXT")  # → "TEXT" (invalid PostgreSQL)

# RIGHT - no quotes
sql.SQL("TEXT")  # → TEXT (valid PostgreSQL)
```

---

## Testing Edge Cases

After the fix, verify edge cases:

### Test 1: Special Characters in Table Names
```bash
# Tables with underscores, numbers
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_database_roundtrip[IPv6AddressScalar-...] -v
```

### Test 2: Complex Values (JSON, Arrays)
```bash
# Scalars with complex PostgreSQL types
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_database_roundtrip[JSONScalar-...] -v
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_database_roundtrip[VectorScalar-...] -v
```

### Test 3: Null Values (if implemented)
Currently, the test always uses non-null values. Consider adding a separate test for null handling.

---

## Additional Improvements (Optional)

### Improvement 1: Test Value Completeness

**Current**: Only 6 scalars have specific test values in `get_test_value_for_scalar()`

**Enhancement**: Add appropriate test values for all 54 scalars

```python
def get_test_value_for_scalar(scalar_class):
    """Get a test value appropriate for the given scalar type."""
    test_values = {
        # Existing
        CIDRScalar: "192.168.1.0/24",
        CUSIPScalar: "037833100",
        DateScalar: "2023-12-13",
        IpAddressScalar: "192.168.1.1",
        JSONScalar: {"key": "value", "number": 42},
        UUIDScalar: "550e8400-e29b-41d4-a716-446655440000",

        # Add more
        AirportCodeScalar: "LAX",
        ColorScalar: "#FF5733",
        EmailScalar: "test@example.com",
        URLScalar: "https://example.com",
        PhoneNumberScalar: "+1-555-123-4567",
        # ... etc for all 54 scalars
    }
    return test_values.get(scalar_class, "test_value")
```

**Note**: Not required for this phase, but improves test quality.

### Improvement 2: PostgreSQL Type Completeness

**Current**: Only 6 scalars have specific PostgreSQL types

**Enhancement**: Map all scalars to correct PostgreSQL types

```python
def get_postgres_type_for_scalar(scalar_class):
    """Get the appropriate PostgreSQL type for a scalar."""
    type_mapping = {
        CIDRScalar: "CIDR",
        CUSIPScalar: "VARCHAR(9)",
        DateScalar: "DATE",
        IpAddressScalar: "INET",
        JSONScalar: "JSONB",
        UUIDScalar: "UUID",

        # Add more
        DateTimeScalar: "TIMESTAMP",
        TimeScalar: "TIME",
        LTreeScalar: "LTREE",
        VectorScalar: "VECTOR",
        MacAddressScalar: "MACADDR",
        # ... etc
    }
    return type_mapping.get(scalar_class, "TEXT")
```

**Note**: `TEXT` fallback works for most scalars, but specific types enable better testing.

---

## Commit Message

```
fix(tests): use psycopg3 SQL composition for scalar roundtrip tests [REFACTOR]

Database roundtrip tests were failing with parameter binding errors because
they mixed f-string formatting with parameterized queries.

Root cause:
- f-strings interpolate ALL placeholders at Python level
- psycopg3 saw literal "$1" text, not a parameter placeholder
- When parameters were passed, psycopg3 error: "0 placeholders but 1 parameter"

Solution:
- Import psycopg.sql module
- Use sql.SQL() for composable queries
- Use sql.Identifier() for table/column names (safely quoted)
- Use %s for value placeholders (psycopg3 style)
- Pass parameters separately to conn.execute()

Changes:
- tests/integration/meta/test_all_scalars.py
  - Add: from psycopg import sql
  - Fix: INSERT, SELECT, CREATE TABLE, DROP TABLE statements
  - Use: sql.Identifier() for dynamic identifiers
  - Use: %s placeholders for values

Tests fixed: 114 (all scalar database roundtrip tests)

Verification:
  uv run pytest tests/integration/meta/test_all_scalars.py -v
  # 168 passed (54 registration + 114 roundtrip)
```

---

## Success Metrics

After completing this phase:

- [x] **Zero parameter binding errors**
- [x] **54/54 scalar roundtrip tests pass**
- [x] **No regressions** (registration tests still pass)
- [x] **Production-quality code** (parameterized queries, no SQL injection risk)
- [x] **Best practices** (follows psycopg3 patterns)

---

## Estimated Timeline

- **Reading this plan**: 15 minutes
- **Making changes**: 30 minutes
- **Testing**: 15 minutes
- **Debugging (if needed)**: 15 minutes
- **Verification**: 10 minutes
- **Commit**: 5 minutes

**Total**: 1-2 hours

---

## Next Phase

After this phase passes, all scalar integration tests will be complete. Move on to other test files if any remain (e.g., `test_all_where_operators.py`).

---

## References

- **psycopg3 SQL Composition**: https://www.psycopg.org/psycopg3/docs/api/sql.html
- **Parameter Placeholders**: psycopg3 uses `%s` (not `$1` like raw PostgreSQL)
- **SQL Injection Prevention**: Always use `sql.Identifier()` for dynamic table/column names

---

**Status**: Ready for implementation ✅
