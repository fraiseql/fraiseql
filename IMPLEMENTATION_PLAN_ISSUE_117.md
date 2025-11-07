# Implementation Plan: Issue #117 - FraiseQLRepository Utility Methods

**Issue**: https://github.com/fraiseql/fraiseql/issues/117
**Pattern**: Test-Driven Development (TDD) with RED-GREEN-REFACTOR-QA cycles
**Precedent**: `count()` method implementation in v1.3.2 (Issue #116)

---

## 📋 Overview

This plan implements 9 utility methods for `FraiseQLRepository` in 3 phases:
- **Phase 1**: `exists()`, `sum()`, `avg()`, `min()`, `max()` (High priority, simple)
- **Phase 2**: `distinct()`, `pluck()` (Medium priority, simple)
- **Phase 3**: `aggregate()`, `batch_exists()` (Lower priority, more complex)

Each phase follows strict TDD cycles: **RED → GREEN → REFACTOR → QA**

### ⚠️ Important Implementation Notes

**SQL Injection Protection**:
- All table names use `psycopg.sql.Identifier` (protects against SQL injection)
- All field names use `psycopg.sql.Identifier` (protects against SQL injection)
- Schema-qualified tables (`schema.table`) are handled correctly
- Follows same pattern as `count()` method from v1.3.2

**Datatype Handling**:
- `sum()` and `avg()`: Convert PostgreSQL `Decimal` to Python `float` for consistency
- `min()` and `max()`: Return value **as-is** to preserve original type (Decimal, datetime, etc.)
- `distinct()` and `pluck()`: Return values as-is (preserves all datatypes)
- `batch_exists()`: Returns boolean dict (type-agnostic for IDs)

**Why Different Type Handling?**
- **Sum/Avg**: Always numeric operations → float is expected Python type
- **Min/Max**: Can operate on ANY comparable type (dates, numbers, strings) → preserve original type
- **Distinct/Pluck**: Extract field values → preserve whatever type the field has

---

## 🎯 Phase 1: Existence Checks and Simple Aggregations

**Objective**: Implement `exists()`, `sum()`, `avg()`, `min()`, `max()`
**Estimated Time**: 1-2 days
**Files**:
- Implementation: `src/fraiseql/db.py`
- Tests: `tests/unit/db/test_db_utility_methods.py`

---

### Phase 1.1: exists() Method

#### TDD Cycle 1.1

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Create new test file** with the following tests:

```python
import pytest
from unittest.mock import AsyncMock, MagicMock, patch
from fraiseql.db import FraiseQLRepository


class TestExists:
    """Test suite for exists() method."""

    def test_exists_method_exists(self):
        """Test that exists() method exists on FraiseQLRepository."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "exists")
        assert callable(db.exists)

    @pytest.mark.asyncio
    async def test_exists_returns_bool(self):
        """Test that exists() returns a boolean."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        # Mock database connection
        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(True,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.exists("v_users")
        assert isinstance(result, bool)

    @pytest.mark.asyncio
    async def test_exists_true_when_records_exist(self):
        """Test exists() returns True when records exist."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(True,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.exists("v_users")
        assert result is True

    @pytest.mark.asyncio
    async def test_exists_false_when_no_records(self):
        """Test exists() returns False when no records exist."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(False,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.exists("v_users")
        assert result is False

    @pytest.mark.asyncio
    async def test_exists_with_where_clause(self):
        """Test exists() with WHERE filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(True,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.exists("v_users", where={"email": {"eq": "test@example.com"}})

        # Verify WHERE clause was included in SQL
        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result is True

    @pytest.mark.asyncio
    async def test_exists_with_tenant_id(self):
        """Test exists() with tenant_id filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(True,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        import uuid
        tenant_id = uuid.uuid4()
        result = await db.exists("v_users", tenant_id=tenant_id)

        # Verify tenant_id was included in SQL
        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "tenant_id" in sql_query
        assert result is True

    @pytest.mark.asyncio
    async def test_exists_generates_correct_sql(self):
        """Test exists() generates SELECT EXISTS() SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(True,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.exists("v_users")

        # Verify SQL structure
        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT EXISTS" in sql_query
        assert "FROM v_users" in sql_query

    @pytest.mark.asyncio
    async def test_exists_with_multiple_filters(self):
        """Test exists() with multiple WHERE filters."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(False,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.exists(
            "v_users",
            where={"email": {"eq": "test@example.com"}, "status": {"eq": "active"}}
        )

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result is False
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestExists -v
```

**Expected**: All tests fail because `exists()` method doesn't exist yet.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after the `count()` method** (around line 910):

```python
async def exists(
    self,
    view_name: str,
    **kwargs: Any,
) -> bool:
    """Check if any records exist matching the filter.

    More efficient than count() > 0 for existence checks.
    Uses EXISTS() SQL query for optimal performance.

    Args:
        view_name: Database table/view name (e.g., "v_users", "v_orders")
        **kwargs: Query parameters:
            - where: dict - WHERE clause filters (e.g., {"email": {"eq": "test@example.com"}})
            - tenant_id: UUID - Filter by tenant_id
            - Any other parameters supported by _build_where_clause()

    Returns:
        True if at least one record exists, False otherwise

    Example:
        # Check if email exists
        exists = await db.exists("v_users", where={"email": {"eq": "test@example.com"}})

        # Check if tenant has orders
        has_orders = await db.exists("v_orders", tenant_id=tenant_id)

        # Check with multiple filters
        exists = await db.exists(
            "v_users",
            where={"email": {"eq": "test@example.com"}, "status": {"eq": "active"}}
        )
    """
    # Build WHERE clause using existing helper
    where_parts = self._build_where_clause(view_name, **kwargs)

    # Build SQL query with EXISTS
    where_clause = " AND ".join(where_parts) if where_parts else "1=1"
    query = f"SELECT EXISTS(SELECT 1 FROM {view_name} WHERE {where_clause})"

    # Execute query
    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            return bool(result[0]) if result else False
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestExists -v
```

**Expected**: All 9 tests pass.

---

##### 🔧 REFACTOR Phase: Optimize and Clean

**Review**:
1. ✅ Reuses `_build_where_clause()` helper (DRY principle)
2. ✅ Uses `SELECT EXISTS()` for optimal performance
3. ✅ Comprehensive docstring
4. ✅ Follows pattern from `count()` method

**No refactoring needed** - implementation is already clean.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestExists -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase: Verify Quality

**Run full db test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass (including existing count() tests).

**Check test coverage**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestExists --cov=src/fraiseql/db --cov-report=term-missing
```

**Expected**: `exists()` method has 100% coverage.

---

### Phase 1.2: sum() Method

#### TDD Cycle 1.2

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add to existing file** after `TestExists` class:

```python
class TestSum:
    """Test suite for sum() method."""

    def test_sum_method_exists(self):
        """Test that sum() method exists on FraiseQLRepository."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "sum")
        assert callable(db.sum)

    @pytest.mark.asyncio
    async def test_sum_returns_float(self):
        """Test that sum() returns a float."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(125.50,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.sum("v_orders", "amount")
        assert isinstance(result, float)

    @pytest.mark.asyncio
    async def test_sum_with_values(self):
        """Test sum() returns correct sum."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(250.75,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.sum("v_orders", "amount")
        assert result == 250.75

    @pytest.mark.asyncio
    async def test_sum_returns_zero_when_no_records(self):
        """Test sum() returns 0.0 when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(None,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.sum("v_orders", "amount")
        assert result == 0.0

    @pytest.mark.asyncio
    async def test_sum_with_where_clause(self):
        """Test sum() with WHERE filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(500.0,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.sum("v_orders", "amount", where={"status": {"eq": "completed"}})

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result == 500.0

    @pytest.mark.asyncio
    async def test_sum_with_tenant_id(self):
        """Test sum() with tenant_id filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(750.0,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        import uuid
        tenant_id = uuid.uuid4()
        result = await db.sum("v_orders", "amount", tenant_id=tenant_id)

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "tenant_id" in sql_query
        assert result == 750.0

    @pytest.mark.asyncio
    async def test_sum_generates_correct_sql(self):
        """Test sum() generates SELECT SUM() SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(100.0,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.sum("v_orders", "amount")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT SUM(amount)" in sql_query or "SELECT COALESCE(SUM(amount)" in sql_query
        assert "FROM v_orders" in sql_query

    @pytest.mark.asyncio
    async def test_sum_requires_field_parameter(self):
        """Test sum() requires field parameter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        # Should raise TypeError if field is missing
        with pytest.raises(TypeError):
            await db.sum("v_orders")  # Missing field parameter
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestSum -v
```

**Expected**: All tests fail because `sum()` method doesn't exist yet.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after `exists()`**:

```python
async def sum(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> float:
    """Sum a numeric field.

    Args:
        view_name: Database table/view name (e.g., "v_orders")
        field: Field name to sum (e.g., "amount", "quantity")
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        Sum as float (returns 0.0 if no records)

    Example:
        # Total revenue
        total = await db.sum("v_orders", "amount")

        # Total for completed orders
        total = await db.sum(
            "v_orders",
            "amount",
            where={"status": {"eq": "completed"}}
        )

        # Total for tenant
        total = await db.sum("v_orders", "amount", tenant_id=tenant_id)
    """
    from psycopg.sql import SQL, Composed, Identifier
    from decimal import Decimal

    # Build WHERE clause using existing helper
    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    # Build field identifier (protect against SQL injection)
    field_identifier = Identifier(field)

    # Build SUM query with COALESCE to return 0 instead of NULL
    query_parts = [
        SQL("SELECT COALESCE(SUM("),
        field_identifier,
        SQL("), 0) FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    query = Composed(query_parts)

    # Execute query
    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            if result and result[0] is not None:
                # Convert Decimal to float if needed
                value = result[0]
                return float(value) if isinstance(value, Decimal) else float(value)
            return 0.0
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestSum -v
```

**Expected**: All 9 tests pass.

---

##### 🔧 REFACTOR Phase: Optimize and Clean

**Review**:
1. ✅ Reuses `_build_where_clause()` helper
2. ✅ Uses `COALESCE()` to return 0 instead of NULL
3. ✅ Comprehensive docstring
4. ✅ Follows same pattern as `exists()`

**No refactoring needed**.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestSum -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase: Verify Quality

**Run full db test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

---

### Phase 1.3: avg(), min(), max() Methods

#### TDD Cycle 1.3

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add three test classes** after `TestSum`:

```python
class TestAvg:
    """Test suite for avg() method."""

    def test_avg_method_exists(self):
        """Test that avg() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "avg")
        assert callable(db.avg)

    @pytest.mark.asyncio
    async def test_avg_returns_float(self):
        """Test that avg() returns a float."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(125.50,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.avg("v_orders", "amount")
        assert isinstance(result, float)

    @pytest.mark.asyncio
    async def test_avg_returns_zero_when_no_records(self):
        """Test avg() returns 0.0 when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(None,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.avg("v_orders", "amount")
        assert result == 0.0

    @pytest.mark.asyncio
    async def test_avg_generates_correct_sql(self):
        """Test avg() generates SELECT AVG() SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(100.0,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.avg("v_orders", "amount")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT COALESCE(AVG(amount)" in sql_query or "SELECT AVG(amount)" in sql_query
        assert "FROM v_orders" in sql_query


class TestMin:
    """Test suite for min() method."""

    def test_min_method_exists(self):
        """Test that min() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "min")
        assert callable(db.min)

    @pytest.mark.asyncio
    async def test_min_returns_value(self):
        """Test that min() returns the minimum value."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(9.99,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.min("v_products", "price")
        assert result == 9.99

    @pytest.mark.asyncio
    async def test_min_returns_none_when_no_records(self):
        """Test min() returns None when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(None,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.min("v_products", "price")
        assert result is None

    @pytest.mark.asyncio
    async def test_min_generates_correct_sql(self):
        """Test min() generates SELECT MIN() SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(9.99,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.min("v_products", "price")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT MIN(price)" in sql_query
        assert "FROM v_products" in sql_query


class TestMax:
    """Test suite for max() method."""

    def test_max_method_exists(self):
        """Test that max() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "max")
        assert callable(db.max)

    @pytest.mark.asyncio
    async def test_max_returns_value(self):
        """Test that max() returns the maximum value."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(999.99,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.max("v_products", "price")
        assert result == 999.99

    @pytest.mark.asyncio
    async def test_max_returns_none_when_no_records(self):
        """Test max() returns None when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(None,))
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.max("v_products", "price")
        assert result is None

    @pytest.mark.asyncio
    async def test_max_generates_correct_sql(self):
        """Test max() generates SELECT MAX() SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(999.99,))
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.max("v_products", "price")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT MAX(price)" in sql_query
        assert "FROM v_products" in sql_query
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestAvg -v
uv run pytest tests/unit/db/test_db_utility_methods.py::TestMin -v
uv run pytest tests/unit/db/test_db_utility_methods.py::TestMax -v
```

**Expected**: All tests fail because methods don't exist yet.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add three methods after `sum()`**:

```python
async def avg(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> float:
    """Average of a numeric field.

    Args:
        view_name: Database table/view name
        field: Field name to average
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        Average as float (returns 0.0 if no records)

    Example:
        # Average order value
        avg_order = await db.avg("v_orders", "amount")

        # Average for completed orders
        avg_order = await db.avg(
            "v_orders",
            "amount",
            where={"status": {"eq": "completed"}}
        )
    """
    from psycopg.sql import SQL, Composed, Identifier
    from decimal import Decimal

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(field)

    query_parts = [
        SQL("SELECT COALESCE(AVG("),
        field_identifier,
        SQL("), 0) FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            if result and result[0] is not None:
                value = result[0]
                return float(value) if isinstance(value, Decimal) else float(value)
            return 0.0

async def min(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> Any:
    """Minimum value of a field.

    Args:
        view_name: Database table/view name
        field: Field name to get minimum value
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        Minimum value (type depends on field), or None if no records

    Example:
        # Lowest product price
        min_price = await db.min("v_products", "price")

        # Earliest order date
        first_order = await db.min("v_orders", "created_at")
    """
    from psycopg.sql import SQL, Composed, Identifier

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(field)

    query_parts = [
        SQL("SELECT MIN("),
        field_identifier,
        SQL(") FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            # Return value as-is (preserves original type: Decimal, datetime, etc.)
            return result[0] if result else None

async def max(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> Any:
    """Maximum value of a field.

    Args:
        view_name: Database table/view name
        field: Field name to get maximum value
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        Maximum value (type depends on field), or None if no records

    Example:
        # Highest product price
        max_price = await db.max("v_products", "price")

        # Latest order date
        last_order = await db.max("v_orders", "created_at")
    """
    from psycopg.sql import SQL, Composed, Identifier

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(field)

    query_parts = [
        SQL("SELECT MAX("),
        field_identifier,
        SQL(") FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            # Return value as-is (preserves original type: Decimal, datetime, etc.)
            return result[0] if result else None
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestAvg -v
uv run pytest tests/unit/db/test_db_utility_methods.py::TestMin -v
uv run pytest tests/unit/db/test_db_utility_methods.py::TestMax -v
```

**Expected**: All tests pass.

---

##### 🔧 REFACTOR Phase: Extract Common Pattern

**Observation**: All aggregation methods (`sum`, `avg`, `min`, `max`) follow the same pattern.

**Consider extracting helper** (optional optimization):

```python
async def _aggregate_field(
    self,
    view_name: str,
    field: str,
    agg_function: str,
    default_value: Any = None,
    **kwargs: Any,
) -> Any:
    """Helper for field aggregation operations.

    Internal method - use sum(), avg(), min(), max() instead.
    """
    where_parts = self._build_where_clause(view_name, **kwargs)
    where_clause = " AND ".join(where_parts) if where_parts else "1=1"

    if default_value is not None:
        query = f"SELECT COALESCE({agg_function}({field}), {default_value}) FROM {view_name} WHERE {where_clause}"
    else:
        query = f"SELECT {agg_function}({field}) FROM {view_name} WHERE {where_clause}"

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()
            if default_value is not None:
                return type(default_value)(result[0]) if result else default_value
            return result[0] if result else None
```

**Then simplify methods**:

```python
async def sum(self, view_name: str, field: str, **kwargs: Any) -> float:
    """Sum a numeric field."""
    return await self._aggregate_field(view_name, field, "SUM", 0.0, **kwargs)

async def avg(self, view_name: str, field: str, **kwargs: Any) -> float:
    """Average of a numeric field."""
    return await self._aggregate_field(view_name, field, "AVG", 0.0, **kwargs)

async def min(self, view_name: str, field: str, **kwargs: Any) -> Any:
    """Minimum value of a field."""
    return await self._aggregate_field(view_name, field, "MIN", None, **kwargs)

async def max(self, view_name: str, field: str, **kwargs: Any) -> Any:
    """Maximum value of a field."""
    return await self._aggregate_field(view_name, field, "MAX", None, **kwargs)
```

**Decision**: Keep methods explicit for now (easier to understand). Refactor to helper can be done later if needed.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase: Phase 1 Complete

**Run full test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

**Summary of Phase 1**:
- ✅ `exists()` - 9 tests passing
- ✅ `sum()` - 9 tests passing
- ✅ `avg()` - 4 tests passing
- ✅ `min()` - 4 tests passing
- ✅ `max()` - 4 tests passing
- **Total**: 30 tests passing

---

## 🎯 Phase 2: Value Extraction Methods

**Objective**: Implement `distinct()`, `pluck()`
**Estimated Time**: 1 day
**Files**:
- Implementation: `src/fraiseql/db.py`
- Tests: `tests/unit/db/test_db_utility_methods.py`

---

### Phase 2.1: distinct() Method

#### TDD Cycle 2.1

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add test class**:

```python
class TestDistinct:
    """Test suite for distinct() method."""

    def test_distinct_method_exists(self):
        """Test that distinct() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "distinct")
        assert callable(db.distinct)

    @pytest.mark.asyncio
    async def test_distinct_returns_list(self):
        """Test that distinct() returns a list."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("books",), ("electronics",), ("clothing",)])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.distinct("v_products", "category")
        assert isinstance(result, list)

    @pytest.mark.asyncio
    async def test_distinct_returns_unique_values(self):
        """Test distinct() returns unique values."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("books",), ("electronics",), ("clothing",)])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.distinct("v_products", "category")
        assert result == ["books", "electronics", "clothing"]

    @pytest.mark.asyncio
    async def test_distinct_returns_empty_list_when_no_records(self):
        """Test distinct() returns empty list when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.distinct("v_products", "category")
        assert result == []

    @pytest.mark.asyncio
    async def test_distinct_with_where_clause(self):
        """Test distinct() with WHERE filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("active",), ("pending",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.distinct(
            "v_orders",
            "status",
            where={"created_at": {"gte": "2024-01-01"}}
        )

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result == ["active", "pending"]

    @pytest.mark.asyncio
    async def test_distinct_generates_correct_sql(self):
        """Test distinct() generates SELECT DISTINCT SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("books",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.distinct("v_products", "category")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT DISTINCT category" in sql_query
        assert "FROM v_products" in sql_query
        assert "ORDER BY category" in sql_query

    @pytest.mark.asyncio
    async def test_distinct_with_tenant_id(self):
        """Test distinct() with tenant_id filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("US",), ("FR",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        import uuid
        tenant_id = uuid.uuid4()
        result = await db.distinct("v_users", "country", tenant_id=tenant_id)

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "tenant_id" in sql_query
        assert result == ["US", "FR"]
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestDistinct -v
```

**Expected**: All tests fail.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after aggregation methods**:

```python
async def distinct(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> list[Any]:
    """Get distinct values for a field.

    Args:
        view_name: Database table/view name
        field: Field name to get distinct values
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        List of unique values (sorted)

    Example:
        # Get all categories
        categories = await db.distinct("v_products", "category")
        # Returns: ["books", "clothing", "electronics"]

        # Get statuses for tenant
        statuses = await db.distinct("v_orders", "status", tenant_id=tenant_id)
        # Returns: ["cancelled", "completed", "pending"]
    """
    from psycopg.sql import SQL, Composed, Identifier

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(field)

    query_parts = [
        SQL("SELECT DISTINCT "),
        field_identifier,
        SQL(" FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    # Order by the field for consistent results
    query_parts.extend([SQL(" ORDER BY "), field_identifier])

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            results = await cursor.fetchall()
            return [row[0] for row in results] if results else []
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestDistinct -v
```

**Expected**: All 7 tests pass.

---

##### 🔧 REFACTOR Phase

**Review**: Implementation is clean and simple. No refactoring needed.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestDistinct -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase

**Run full db test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

---

### Phase 2.2: pluck() Method

#### TDD Cycle 2.2

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add test class**:

```python
class TestPluck:
    """Test suite for pluck() method."""

    def test_pluck_method_exists(self):
        """Test that pluck() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "pluck")
        assert callable(db.pluck)

    @pytest.mark.asyncio
    async def test_pluck_returns_list(self):
        """Test that pluck() returns a list."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[
            (uuid.uuid4(),),
            (uuid.uuid4(),),
            (uuid.uuid4(),),
        ])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.pluck("v_users", "id")
        assert isinstance(result, list)

    @pytest.mark.asyncio
    async def test_pluck_returns_field_values(self):
        """Test pluck() returns field values."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[
            ("user1@example.com",),
            ("user2@example.com",),
            ("user3@example.com",),
        ])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.pluck("v_users", "email")
        assert result == ["user1@example.com", "user2@example.com", "user3@example.com"]

    @pytest.mark.asyncio
    async def test_pluck_returns_empty_list_when_no_records(self):
        """Test pluck() returns empty list when no records."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.pluck("v_users", "email")
        assert result == []

    @pytest.mark.asyncio
    async def test_pluck_with_where_clause(self):
        """Test pluck() with WHERE filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("active@example.com",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.pluck(
            "v_users",
            "email",
            where={"status": {"eq": "active"}}
        )

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result == ["active@example.com"]

    @pytest.mark.asyncio
    async def test_pluck_generates_correct_sql(self):
        """Test pluck() generates SELECT field SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("test@example.com",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.pluck("v_users", "email")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT email" in sql_query
        assert "FROM v_users" in sql_query

    @pytest.mark.asyncio
    async def test_pluck_with_limit(self):
        """Test pluck() with limit parameter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("email1@example.com",), ("email2@example.com",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.pluck("v_users", "email", limit=2)

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "LIMIT" in sql_query
        assert len(result) <= 2
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestPluck -v
```

**Expected**: All tests fail.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after `distinct()`**:

```python
async def pluck(
    self,
    view_name: str,
    field: str,
    **kwargs: Any,
) -> list[Any]:
    """Extract a single field from matching records.

    More efficient than find() when you only need one field.

    Args:
        view_name: Database table/view name
        field: Field name to extract
        **kwargs: Query parameters (where, limit, offset, order_by, etc.)

    Returns:
        List of field values (not full objects)

    Example:
        # Get all user IDs
        user_ids = await db.pluck("v_users", "id")
        # Returns: [uuid1, uuid2, uuid3, ...]

        # Get emails for active users
        emails = await db.pluck(
            "v_users",
            "email",
            where={"status": {"eq": "active"}}
        )
        # Returns: ["user1@example.com", "user2@example.com", ...]

        # Get product names with limit
        names = await db.pluck("v_products", "name", limit=10)
    """
    from psycopg.sql import SQL, Composed, Identifier, Literal

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(field)

    query_parts = [
        SQL("SELECT "),
        field_identifier,
        SQL(" FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    # Add LIMIT if provided (using Literal for safe parameterization)
    if "limit" in kwargs:
        query_parts.extend([SQL(" LIMIT "), Literal(kwargs["limit"])])

    # Add OFFSET if provided
    if "offset" in kwargs:
        query_parts.extend([SQL(" OFFSET "), Literal(kwargs["offset"])])

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            results = await cursor.fetchall()
            return [row[0] for row in results] if results else []
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestPluck -v
```

**Expected**: All 7 tests pass.

---

##### 🔧 REFACTOR Phase

**Review**: Implementation is clean. No refactoring needed.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestPluck -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase: Phase 2 Complete

**Run full test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

**Summary of Phase 2**:
- ✅ `distinct()` - 7 tests passing
- ✅ `pluck()` - 7 tests passing
- **Phase 2 Total**: 14 tests passing
- **Overall Total**: 44 tests passing

---

## 🎯 Phase 3: Advanced Methods

**Objective**: Implement `aggregate()`, `batch_exists()`
**Estimated Time**: 1-2 days
**Files**:
- Implementation: `src/fraiseql/db.py`
- Tests: `tests/unit/db/test_db_utility_methods.py`

---

### Phase 3.1: aggregate() Method

#### TDD Cycle 3.1

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add test class**:

```python
class TestAggregate:
    """Test suite for aggregate() method."""

    def test_aggregate_method_exists(self):
        """Test that aggregate() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "aggregate")
        assert callable(db.aggregate)

    @pytest.mark.asyncio
    async def test_aggregate_returns_dict(self):
        """Test that aggregate() returns a dict."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(1000.0, 250.0, 4))
        mock_cursor.description = [
            ("total", None),
            ("avg", None),
            ("count", None),
        ]
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.aggregate(
            "v_orders",
            aggregations={"total": "SUM(amount)", "avg": "AVG(amount)", "count": "COUNT(*)"}
        )
        assert isinstance(result, dict)

    @pytest.mark.asyncio
    async def test_aggregate_multiple_aggregations(self):
        """Test aggregate() with multiple aggregations."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(1000.0, 250.0, 500.0, 10.0, 4))
        mock_cursor.description = [
            ("total_revenue", None),
            ("avg_order", None),
            ("max_order", None),
            ("min_order", None),
            ("order_count", None),
        ]
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.aggregate(
            "v_orders",
            aggregations={
                "total_revenue": "SUM(amount)",
                "avg_order": "AVG(amount)",
                "max_order": "MAX(amount)",
                "min_order": "MIN(amount)",
                "order_count": "COUNT(*)",
            }
        )

        assert result == {
            "total_revenue": 1000.0,
            "avg_order": 250.0,
            "max_order": 500.0,
            "min_order": 10.0,
            "order_count": 4,
        }

    @pytest.mark.asyncio
    async def test_aggregate_with_where_clause(self):
        """Test aggregate() with WHERE filter."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(750.0, 2))
        mock_cursor.description = [("total", None), ("count", None)]
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.aggregate(
            "v_orders",
            aggregations={"total": "SUM(amount)", "count": "COUNT(*)"},
            where={"status": {"eq": "completed"}}
        )

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "WHERE" in sql_query
        assert result == {"total": 750.0, "count": 2}

    @pytest.mark.asyncio
    async def test_aggregate_generates_correct_sql(self):
        """Test aggregate() generates correct multi-aggregation SQL."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchone = AsyncMock(return_value=(1000.0, 250.0))
        mock_cursor.description = [("total", None), ("avg", None)]
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.aggregate(
            "v_orders",
            aggregations={"total": "SUM(amount)", "avg": "AVG(amount)"}
        )

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT" in sql_query
        assert "SUM(amount) AS total" in sql_query
        assert "AVG(amount) AS avg" in sql_query
        assert "FROM v_orders" in sql_query

    @pytest.mark.asyncio
    async def test_aggregate_returns_empty_dict_when_no_aggregations(self):
        """Test aggregate() returns empty dict when no aggregations provided."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        result = await db.aggregate("v_orders", aggregations={})
        assert result == {}
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestAggregate -v
```

**Expected**: All tests fail.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after `pluck()`**:

```python
async def aggregate(
    self,
    view_name: str,
    aggregations: dict[str, str],
    **kwargs: Any,
) -> dict[str, Any]:
    """Perform multiple aggregations in a single query.

    Args:
        view_name: Database table/view name
        aggregations: Dict mapping result names to SQL aggregation expressions
            WARNING: Aggregation expressions are NOT sanitized - only use trusted values
        **kwargs: Query parameters (where, tenant_id, etc.)

    Returns:
        Dict with aggregation results as Python values

    Example:
        # Multiple aggregations in one query
        stats = await db.aggregate(
            "v_orders",
            aggregations={
                "total_revenue": "SUM(amount)",
                "avg_order": "AVG(amount)",
                "max_order": "MAX(amount)",
                "order_count": "COUNT(*)",
            },
            where={"status": {"eq": "completed"}}
        )
        # Returns: {
        #     "total_revenue": 125000.50,
        #     "avg_order": 250.00,
        #     "max_order": 1500.00,
        #     "order_count": 500
        # }
    """
    from psycopg.sql import SQL, Composed, Identifier

    if not aggregations:
        return {}

    where_parts = self._build_where_clause(view_name, **kwargs)

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    # Build SELECT clause with all aggregations
    # NOTE: Aggregation expressions are raw SQL - only use trusted values!
    agg_parts = []
    for name, expr in aggregations.items():
        # Name is sanitized as Identifier, but expr is raw SQL
        agg_parts.append(SQL("{expr} AS {name}").format(
            expr=SQL(expr),  # Raw SQL expression
            name=Identifier(name)
        ))

    query_parts = [
        SQL("SELECT "),
        SQL(", ").join(agg_parts),
        SQL(" FROM "),
        table_identifier,
    ]

    if where_parts:
        query_parts.append(SQL(" WHERE "))
        query_parts.append(SQL(" AND ").join(where_parts))

    query = Composed(query_parts)

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            await cursor.execute(query)
            result = await cursor.fetchone()

            if not result:
                return {name: None for name in aggregations.keys()}

            # Map column names to values
            column_names = [desc[0] for desc in cursor.description]
            return dict(zip(column_names, result))
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestAggregate -v
```

**Expected**: All 6 tests pass.

---

##### 🔧 REFACTOR Phase

**Review**: Implementation is clean and efficient. No refactoring needed.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestAggregate -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase

**Run full db test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

---

### Phase 3.2: batch_exists() Method

#### TDD Cycle 3.2

##### 🔴 RED Phase: Write Failing Tests

**File**: `tests/unit/db/test_db_utility_methods.py`

**Add test class**:

```python
class TestBatchExists:
    """Test suite for batch_exists() method."""

    def test_batch_exists_method_exists(self):
        """Test that batch_exists() method exists."""
        db = FraiseQLRepository(pool=MagicMock())
        assert hasattr(db, "batch_exists")
        assert callable(db.batch_exists)

    @pytest.mark.asyncio
    async def test_batch_exists_returns_dict(self):
        """Test that batch_exists() returns a dict."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        id1, id2 = uuid.uuid4(), uuid.uuid4()

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[(id1,), (id2,)])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.batch_exists("v_users", [id1, id2])
        assert isinstance(result, dict)

    @pytest.mark.asyncio
    async def test_batch_exists_all_exist(self):
        """Test batch_exists() when all IDs exist."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        id1, id2, id3 = uuid.uuid4(), uuid.uuid4(), uuid.uuid4()

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[(id1,), (id2,), (id3,)])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.batch_exists("v_users", [id1, id2, id3])

        assert result == {id1: True, id2: True, id3: True}

    @pytest.mark.asyncio
    async def test_batch_exists_some_missing(self):
        """Test batch_exists() when some IDs are missing."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        id1, id2, id3 = uuid.uuid4(), uuid.uuid4(), uuid.uuid4()

        # Only id1 and id3 exist
        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[(id1,), (id3,)])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.batch_exists("v_users", [id1, id2, id3])

        assert result == {id1: True, id2: False, id3: True}

    @pytest.mark.asyncio
    async def test_batch_exists_none_exist(self):
        """Test batch_exists() when no IDs exist."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        id1, id2 = uuid.uuid4(), uuid.uuid4()

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[])
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        result = await db.batch_exists("v_users", [id1, id2])

        assert result == {id1: False, id2: False}

    @pytest.mark.asyncio
    async def test_batch_exists_empty_list(self):
        """Test batch_exists() with empty ID list."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        result = await db.batch_exists("v_users", [])
        assert result == {}

    @pytest.mark.asyncio
    async def test_batch_exists_generates_correct_sql(self):
        """Test batch_exists() generates SELECT with IN clause."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        import uuid
        id1, id2 = uuid.uuid4(), uuid.uuid4()

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[(id1,)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.batch_exists("v_users", [id1, id2])

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT id" in sql_query
        assert "FROM v_users" in sql_query
        assert "WHERE id" in sql_query
        assert "IN" in sql_query or "= ANY" in sql_query

    @pytest.mark.asyncio
    async def test_batch_exists_custom_id_field(self):
        """Test batch_exists() with custom ID field."""
        pool = MagicMock()
        db = FraiseQLRepository(pool=pool)

        mock_cursor = AsyncMock()
        mock_cursor.fetchall = AsyncMock(return_value=[("user123",)])
        mock_cursor.execute = AsyncMock()
        mock_conn = AsyncMock()
        mock_conn.cursor = MagicMock(return_value=mock_cursor)
        mock_conn.__aenter__ = AsyncMock(return_value=mock_conn)
        mock_conn.__aexit__ = AsyncMock(return_value=None)
        mock_cursor.__aenter__ = AsyncMock(return_value=mock_cursor)
        mock_cursor.__aexit__ = AsyncMock(return_value=None)
        pool.connection = MagicMock(return_value=mock_conn)

        await db.batch_exists("v_users", ["user123", "user456"], id_field="username")

        assert mock_cursor.execute.called
        sql_query = mock_cursor.execute.call_args[0][0]
        assert "SELECT username" in sql_query
        assert "WHERE username" in sql_query
```

**Run tests** (should fail):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestBatchExists -v
```

**Expected**: All tests fail.

---

##### 🟢 GREEN Phase: Minimal Implementation

**File**: `src/fraiseql/db.py`

**Add method after `aggregate()`**:

```python
async def batch_exists(
    self,
    view_name: str,
    ids: list[Any],
    id_field: str = "id",
) -> dict[Any, bool]:
    """Check which IDs exist in a single query.

    More efficient than calling exists() for each ID individually.

    Args:
        view_name: Database table/view name
        ids: List of IDs to check
        id_field: Name of ID field (default: "id")

    Returns:
        Dict mapping each ID to boolean (exists or not)

    Example:
        # Check multiple user IDs
        import uuid
        user_ids = [uuid1, uuid2, uuid3]
        existence = await db.batch_exists("v_users", user_ids)
        # Returns: {uuid1: True, uuid2: False, uuid3: True}

        # Find missing IDs
        missing = [id for id, exists in existence.items() if not exists]
        if missing:
            raise ValueError(f"Users not found: {missing}")

        # Check with custom ID field
        existence = await db.batch_exists(
            "v_users",
            ["user123", "user456"],
            id_field="username"
        )
    """
    from psycopg.sql import SQL, Composed, Identifier, Placeholder

    if not ids:
        return {}

    # Handle schema-qualified table names
    if "." in view_name:
        schema_name, table_name = view_name.split(".", 1)
        table_identifier = Identifier(schema_name, table_name)
    else:
        table_identifier = Identifier(view_name)

    field_identifier = Identifier(id_field)

    # Build query with = ANY(%s) for PostgreSQL array comparison
    # This is more efficient than multiple IN values
    query = Composed([
        SQL("SELECT "),
        field_identifier,
        SQL(" FROM "),
        table_identifier,
        SQL(" WHERE "),
        field_identifier,
        SQL(" = ANY(%s)"),
    ])

    async with self._pool.connection() as conn:
        async with conn.cursor() as cursor:
            # Pass IDs as a list (psycopg converts to PostgreSQL array)
            await cursor.execute(query, (ids,))
            results = await cursor.fetchall()

            # Get set of existing IDs
            existing_ids = {row[0] for row in results}

            # Map all requested IDs to boolean
            return {id_val: (id_val in existing_ids) for id_val in ids}
```

**Run tests** (should pass):
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestBatchExists -v
```

**Expected**: All 8 tests pass.

---

##### 🔧 REFACTOR Phase

**Review**: Implementation is clean and efficient. No refactoring needed.

**Run tests again**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py::TestBatchExists -v
```

**Expected**: All tests still pass.

---

##### ✅ QA Phase: Phase 3 Complete

**Run full test suite**:
```bash
uv run pytest tests/unit/db/ -v
```

**Expected**: All tests pass.

**Summary of Phase 3**:
- ✅ `aggregate()` - 6 tests passing
- ✅ `batch_exists()` - 8 tests passing
- **Phase 3 Total**: 14 tests passing
- **Overall Total**: 58 tests passing

---

## 🎉 Final QA and Release

### Final QA Checklist

**Run all tests**:
```bash
uv run pytest tests/unit/db/ -v --tb=short
```

**Expected**: All 58+ tests passing (including existing db tests).

**Test coverage**:
```bash
uv run pytest tests/unit/db/test_db_utility_methods.py --cov=src/fraiseql/db --cov-report=term-missing
```

**Expected**: All new methods have >95% coverage.

**Type checking**:
```bash
uv run mypy src/fraiseql/db.py
```

**Expected**: No type errors.

**Linting**:
```bash
uv run ruff check src/fraiseql/db.py
```

**Expected**: No linting errors (or auto-fixed by pre-commit).

---

### Documentation Updates

**Update files**:

1. **`docs/reference/database.md`** - Add examples for each new method
2. **`CHANGELOG.md`** - Add v1.3.3 entry with all new methods
3. **`README.md`** - Update version if needed

---

### Create Release

**Version bump**:
- `pyproject.toml`: version = "1.3.3"
- `fraiseql_rs/Cargo.toml`: version = "1.3.3"
- `src/fraiseql/__init__.py`: __version__ = "1.3.3"

**Rebuild Rust**:
```bash
cd fraiseql_rs && cargo check
```

**Create release notes**:
- File: `RELEASE_1.3.3.md`
- Include all 9 methods with examples
- Reference issue #117

**Commit**:
```bash
git add -A
git commit -m "feat: add utility methods to FraiseQLRepository (issue #117)

- exists(): Check if records exist (9 tests)
- sum(): Sum numeric field (9 tests)
- avg(): Average numeric field (4 tests)
- min(): Minimum field value (4 tests)
- max(): Maximum field value (4 tests)
- distinct(): Get unique values (7 tests)
- pluck(): Extract single field (7 tests)
- aggregate(): Multiple aggregations (6 tests)
- batch_exists(): Batch ID checking (8 tests)

All methods follow 'everything in DB' pattern with SQL doing the work.
Total: 58 new tests passing.

Resolves #117

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"
```

---

## 📊 Summary

### Total Implementation

**Methods Implemented**: 9
- `exists()` - Existence check
- `sum()` - Sum aggregation
- `avg()` - Average aggregation
- `min()` - Minimum value
- `max()` - Maximum value
- `distinct()` - Unique values
- `pluck()` - Field extraction
- `aggregate()` - Multiple aggregations
- `batch_exists()` - Batch existence check

**Tests Created**: 58
- Phase 1: 30 tests
- Phase 2: 14 tests
- Phase 3: 14 tests

**Estimated Time**: 3-4 days total
- Phase 1: 1-2 days
- Phase 2: 1 day
- Phase 3: 1-2 days

**Files Modified**:
- `src/fraiseql/db.py` (+400 lines)
- `tests/unit/db/test_db_utility_methods.py` (new, +800 lines)
- `docs/reference/database.md` (+200 lines)
- `CHANGELOG.md` (+100 lines)
- `RELEASE_1.3.3.md` (new, +400 lines)

### Success Metrics

✅ All methods follow TDD (RED-GREEN-REFACTOR-QA)
✅ All methods reuse `_build_where_clause()` helper
✅ Consistent API across all methods
✅ Comprehensive test coverage (>95%)
✅ Database does all computation (pattern aligned)
✅ Type-safe with proper hints
✅ Well-documented with examples
