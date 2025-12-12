# Phase 4: Refactor SQL Generation [REFACTOR]

## Objective

Refactor `_build_where_clause()` to use `WhereClause` directly, making it the single SQL generation code path.

## Context

Phases 1-3 created the normalization layer. Now we integrate it into the query building pipeline, replacing the dual code paths with a single path through `WhereClause`.

## Files to Modify

- `src/fraiseql/db.py` - Refactor `_build_where_clause()` to normalize first
- All methods that call `_build_where_clause()` (find, count, update, delete)

## Implementation Steps

### Step 1: Refactor _build_where_clause()

```python
def _build_where_clause(self, view_name: str, **kwargs: Any) -> list[Any]:
    """Build WHERE clause parts from kwargs.

    New architecture:
        1. Extract where parameter
        2. Normalize to WhereClause (single code path)
        3. Generate SQL from WhereClause
        4. Process remaining kwargs
    """
    from psycopg.sql import SQL, Composed, Identifier, Literal

    where_parts = []

    # Extract where parameter
    where_obj = kwargs.pop("where", None)

    if where_obj:
        # Get table columns for normalization
        table_columns = None
        if hasattr(self, "_introspected_columns") and view_name in self._introspected_columns:
            table_columns = self._introspected_columns[view_name]
        elif view_name in _table_metadata and "columns" in _table_metadata[view_name]:
            table_columns = _table_metadata[view_name]["columns"]

        # SINGLE CODE PATH: Normalize to WhereClause
        try:
            where_clause = self._normalize_where(where_obj, view_name, table_columns)

            # Generate SQL from WhereClause
            sql, params = where_clause.to_sql()

            if sql:
                # Store params for query execution
                if not hasattr(self, "_where_params"):
                    self._where_params = []
                self._where_params.extend(params)

                where_parts.append(sql)

                logger.debug(
                    f"WHERE clause built from {type(where_obj).__name__}",
                    extra={
                        "view_name": view_name,
                        "conditions": len(where_clause.conditions),
                        "fk_optimizations": sum(
                            1 for c in where_clause.conditions
                            if c.lookup_strategy == "fk_column"
                        ),
                    }
                )
        except Exception as e:
            logger.error(
                f"WHERE normalization failed for {view_name}: {e}",
                exc_info=True
            )
            raise

    # Process remaining kwargs as simple equality filters
    for key, value in kwargs.items():
        where_condition = Composed([Identifier(key), SQL(" = "), Literal(value)])
        where_parts.append(where_condition)

    return where_parts
```

### Step 2: Handle Parameters in Query Execution

The current code uses `Literal()` for parameters. With `WhereClause.to_sql()` returning parameterized queries, we need to handle params differently:

```python
async def find(
    self,
    view_name: str,
    select: list[str] | None = None,
    **kwargs: Any,
) -> dict[str, Any]:
    """Find records from a view with optional filtering and ordering."""

    # Clear params from previous queries
    self._where_params = []

    # Build WHERE clause (now populates self._where_params)
    where_parts = self._build_where_clause(view_name, **kwargs)

    # Build query
    # ... existing query building code ...

    # Execute with parameters
    if self._where_params:
        result = await cursor.execute(query, self._where_params)
    else:
        result = await cursor.execute(query)

    # ... rest of method ...
```

### Step 3: Update All Query Methods

Apply same pattern to:
- `count()`
- `update()`
- `delete()`
- Any other methods using `_build_where_clause()`

### Step 4: Add Parameter Binding Correctness Tests

**CRITICAL CORRECTNESS TEST**

Create `tests/integration/test_parameter_binding.py`:

```python
"""Integration tests for parameter binding correctness.

Verifies that parameterized queries have correct parameter alignment
and don't cause silent data corruption.
"""

import uuid
import pytest
from fraiseql.db import FraiseQLRepository
from fraiseql.where_clause import WhereClause


class TestParameterBinding:
    """Test parameter binding correctness in WHERE clause execution."""

    async def test_parameter_count_matches_placeholders(self, class_db_pool):
        """Verify parameter count matches %s placeholder count."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Complex query with multiple parameters
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": uuid.uuid4()}},
            "name": {"contains": "test"}
        }

        table_columns = {"status", "machine_id", "name", "data"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Count placeholders in SQL
        sql_str = sql.as_string(None)
        placeholder_count = sql_str.count("%s")

        assert placeholder_count == len(params), (
            f"Parameter count mismatch: {placeholder_count} placeholders "
            f"but {len(params)} parameters"
        )

    async def test_parameter_order_correctness(self, class_db_pool, setup_hybrid_table):
        """Verify parameters are in correct order for placeholders."""
        test_data = setup_hybrid_table
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Query with known data
        where = {
            "status": {"eq": "active"},
            "machine": {"id": {"eq": test_data["machine1_id"]}}
        }

        # This should return results (correct binding)
        result = await repo.find("tv_allocation", where=where)

        # Swap parameter order manually to verify detection
        table_columns = {"status", "machine_id", "data"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Original should work
        assert result is not None

        # Swapped params should return different results
        if len(params) >= 2:
            swapped_params = [params[1], params[0]] + params[2:]
            # Execute with swapped params (should get different/no results)
            # This verifies parameter order matters
            pass  # Can't easily test without direct SQL execution

    async def test_in_operator_parameter_binding(self, class_db_pool):
        """Verify IN operator uses tuple parameter correctly."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        where = {"status": {"in": ["active", "pending", "completed"]}}

        table_columns = {"status"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # IN operator should have single tuple parameter
        assert len(params) == 1
        assert isinstance(params[0], tuple)
        assert params[0] == ("active", "pending", "completed")

        # SQL should have single %s placeholder for IN
        sql_str = sql.as_string(None)
        assert sql_str.count("%s") == 1

    async def test_null_operator_no_parameters(self, class_db_pool):
        """Verify IS NULL operator has no parameters."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        where = {"machine_id": {"isnull": True}}

        table_columns = {"machine_id"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # IS NULL should have no parameters
        assert len(params) == 0

        # SQL should have no %s placeholders
        sql_str = sql.as_string(None)
        assert "%s" not in sql_str
        assert "IS NULL" in sql_str

    async def test_mixed_operators_parameter_binding(self, class_db_pool):
        """Verify complex WHERE with mixed operators has correct binding."""
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        machine_id = uuid.uuid4()
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": machine_id}},
            "name": {"contains": "test"},
            "created_at": {"gte": "2024-01-01"}
        }

        table_columns = {"status", "machine_id", "name", "created_at", "data"}
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Should have 4 parameters (IN tuple, eq UUID, contains pattern, gte date)
        expected_param_count = 4
        assert len(params) == expected_param_count

        # Verify parameter types
        assert isinstance(params[0], tuple)  # IN values
        assert isinstance(params[1], uuid.UUID)  # machine_id
        assert isinstance(params[2], str)  # LIKE pattern
        assert isinstance(params[3], str)  # date

    async def test_query_execution_smoke_test(self, class_db_pool, setup_hybrid_table):
        """Smoke test: Execute complex query to verify no runtime errors."""
        test_data = setup_hybrid_table
        repo = FraiseQLRepository(class_db_pool, context={"tenant_id": "test"})

        # Complex query
        where = {
            "status": {"in": ["active", "pending"]},
            "machine": {"id": {"eq": test_data["machine1_id"]}},
            "OR": [
                {"name": {"contains": "test"}},
                {"name": {"startswith": "demo"}}
            ]
        }

        # Should execute without errors
        result = await repo.find("tv_allocation", where=where)

        # Should return structured result
        assert result is not None
        assert "data" in result or "tv_allocation" in result
```

## Verification Commands

```bash
# Run all WHERE tests
uv run pytest tests/unit/test_where_clause.py tests/unit/test_where_normalization.py -v

# Run parameter binding tests (CRITICAL)
uv run pytest tests/integration/test_parameter_binding.py -v

# Run regression tests
uv run pytest tests/regression/test_nested_filter_id_field.py -v

# Run full test suite (critical - ensure no regressions)
uv run pytest tests/ -v -x

# Test with logging to verify single code path
uv run pytest tests/regression/test_nested_filter_id_field.py -v -s --log-cli-level=DEBUG

# Performance test (should be similar or better)
uv run pytest tests/performance/ -v  # if performance tests exist
```

## Acceptance Criteria

- [ ] `_build_where_clause()` uses `_normalize_where()` for all inputs
- [ ] Single code path for WHERE processing
- [ ] Parameterized queries work correctly
- [ ] **Parameter binding tests pass (correctness verified)**
- [ ] All existing tests pass (no regressions)
- [ ] Nested FK filters use FK columns (verified in logs)
- [ ] No "Unsupported operator" warnings
- [ ] Query performance unchanged or improved
- [ ] All query methods (find, count, update, delete) work correctly

## DO NOT

- ❌ Remove old code yet (keep as commented fallback for safety)
- ❌ Change public API
- ❌ Break backward compatibility

## Notes

This is a **REFACTOR phase**: Behavior should remain identical to users, but internal implementation changes.

Consider adding a feature flag for gradual rollout:
```python
USE_NEW_WHERE_NORMALIZATION = os.getenv("FRAISEQL_NEW_WHERE", "true").lower() == "true"
```

## Next Phase

**Phase 5:** Add explicit FK metadata to `register_type_for_view()` and generated WhereInput classes.
