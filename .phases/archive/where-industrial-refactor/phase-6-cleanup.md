# Phase 6: Remove Old Code Paths [REFACTOR]

## Objective

Remove redundant WHERE processing code now that normalization layer handles everything.

## Context

After Phases 1-5, we have:
- ✅ Canonical WhereClause representation
- ✅ Normalization for dict and WhereInput
- ✅ Single SQL generation path
- ✅ Explicit FK metadata

Old code that can be removed:
- `_where_obj_to_dict()` - replaced by `_normalize_where()`
- Dual code paths in `_build_where_clause()` - now single path
- Old `_convert_dict_where_to_sql()` - logic moved to WhereClause.to_sql()

## Files to Modify

- `src/fraiseql/db.py` - Remove old methods

## Implementation Steps

### Step 1: Remove _where_obj_to_dict()

```python
# DELETE this entire method (lines ~2871-2920)
def _where_obj_to_dict(self, where_obj: Any, table_columns: set[str]) -> dict[str, Any] | None:
    # ... DELETE ...
```

### Step 2: Simplify _convert_dict_where_to_sql()

The old `_convert_dict_where_to_sql()` had complex FK detection logic. This is now in `normalize_dict_where()`.

**Option A:** Keep method as thin wrapper (backward compatibility)
```python
def _convert_dict_where_to_sql(
    self,
    where_dict: dict,
    view_name: str,
    table_columns: set[str] | None = None,
    jsonb_column: str | None = None,
) -> Composed | None:
    """Convert dict WHERE to SQL (legacy method).

    DEPRECATED: Use _normalize_where() + WhereClause.to_sql() instead.
    Kept for backward compatibility.
    """
    warnings.warn(
        "_convert_dict_where_to_sql() is deprecated. Use _normalize_where() instead.",
        DeprecationWarning,
        stacklevel=2
    )

    clause = self._normalize_where(where_dict, view_name, table_columns)
    sql, params = clause.to_sql()

    # Store params
    if not hasattr(self, "_where_params"):
        self._where_params = []
    self._where_params.extend(params)

    return sql
```

**Option B:** Remove entirely (breaking change)
```python
# DELETE _convert_dict_where_to_sql() completely
```

**Recommendation:** Option A for now, remove in next major version.

### Step 3: Remove Old _build_where_clause() Branches

```python
# DELETE old branching logic:
# - Lines checking hasattr(where_obj, "to_sql")
# - Lines calling _where_obj_to_dict()
# - Fallback paths to old SQL generation

# Keep only:
def _build_where_clause(self, view_name: str, **kwargs: Any) -> list[Any]:
    """Build WHERE clause (refactored)."""
    where_parts = []
    where_obj = kwargs.pop("where", None)

    if where_obj:
        table_columns = self._get_table_columns(view_name)
        where_clause = self._normalize_where(where_obj, view_name, table_columns)
        sql, params = where_clause.to_sql()

        if sql:
            self._where_params = getattr(self, "_where_params", [])
            self._where_params.extend(params)
            where_parts.append(sql)

    # Simple equality filters from kwargs
    for key, value in kwargs.items():
        where_parts.append(Composed([Identifier(key), SQL(" = "), Literal(value)]))

    return where_parts
```

### Step 4: Remove Old _is_nested_object_filter() from db.py

The old `_is_nested_object_filter()` in `db.py` is now replaced by the one in `where_normalization.py`.

```python
# DELETE old _is_nested_object_filter() from db.py
# Keep the one in where_normalization.py
```

### Step 5: Add Golden File Regression Tests

**CRITICAL BACKWARD COMPATIBILITY TEST**

Create `tests/regression/test_where_golden.py`:

```python
"""Golden file tests for WHERE clause SQL generation.

These tests verify that the refactor doesn't change SQL output for common queries.
Each test records the expected SQL and parameters, then verifies they remain identical.
"""

import uuid
import pytest
from fraiseql.db import FraiseQLRepository


# Golden queries: real production patterns with expected SQL output
GOLDEN_QUERIES = [
    {
        "name": "simple_equality",
        "where": {"status": "active"},
        "expected_sql_contains": ['"status"', '=', '%s'],
        "expected_params": ["active"],
    },
    {
        "name": "simple_eq_operator",
        "where": {"status": {"eq": "active"}},
        "expected_sql_contains": ['"status"', '=', '%s'],
        "expected_params": ["active"],
    },
    {
        "name": "in_operator",
        "where": {"status": {"in": ["active", "pending", "completed"]}},
        "expected_sql_contains": ['"status"', 'IN', '%s'],
        "expected_params": [("active", "pending", "completed")],
    },
    {
        "name": "fk_nested_filter",
        "where": {"machine": {"id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")}}},
        "expected_sql_contains": ['"machine_id"', '=', '%s'],
        "expected_params": [uuid.UUID("12345678-1234-5678-1234-567812345678")],
        "table_columns": {"machine_id", "data"},
    },
    {
        "name": "jsonb_nested_filter",
        "where": {"device": {"name": {"eq": "Printer"}}},
        "expected_sql_contains": ['data', '->', "'device'", '->>', "'name'", '=', '%s'],
        "expected_params": ["Printer"],
        "table_columns": {"id", "data"},  # No device_id column
    },
    {
        "name": "multiple_conditions_and",
        "where": {
            "status": {"eq": "active"},
            "machine": {"id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")}}
        },
        "expected_sql_contains": ['"status"', '=', 'AND', '"machine_id"', '='],
        "expected_param_count": 2,
        "table_columns": {"status", "machine_id", "data"},
    },
    {
        "name": "or_operator",
        "where": {
            "OR": [
                {"status": {"eq": "active"}},
                {"status": {"eq": "pending"}}
            ]
        },
        "expected_sql_contains": ['"status"', '=', 'OR', '"status"', '='],
        "expected_param_count": 2,
    },
    {
        "name": "contains_string_operator",
        "where": {"name": {"contains": "test"}},
        "expected_sql_contains": ['"name"', 'LIKE', '%s'],
        "expected_params": ["%test%"],
    },
    {
        "name": "isnull_operator",
        "where": {"machine_id": {"isnull": True}},
        "expected_sql_contains": ['"machine_id"', 'IS NULL'],
        "expected_param_count": 0,
    },
    {
        "name": "not_isnull_operator",
        "where": {"machine_id": {"isnull": False}},
        "expected_sql_contains": ['"machine_id"', 'IS NOT NULL'],
        "expected_param_count": 0,
    },
    {
        "name": "gte_operator",
        "where": {"created_at": {"gte": "2024-01-01"}},
        "expected_sql_contains": ['"created_at"', '>=', '%s'],
        "expected_params": ["2024-01-01"],
    },
    {
        "name": "mixed_fk_and_jsonb",
        "where": {
            "machine": {
                "id": {"eq": uuid.UUID("12345678-1234-5678-1234-567812345678")},
                "name": {"contains": "Printer"}
            }
        },
        "expected_sql_contains": ['"machine_id"', '=', 'data', "->", "'machine'", "->>", "'name'", 'LIKE'],
        "expected_param_count": 2,
        "table_columns": {"machine_id", "data"},
    },
]


class TestGoldenFileRegression:
    """Test SQL output unchanged for common WHERE patterns."""

    @pytest.mark.parametrize("golden", GOLDEN_QUERIES, ids=lambda g: g["name"])
    def test_where_sql_unchanged(self, golden):
        """Verify WHERE clause generates expected SQL."""
        repo = FraiseQLRepository(None)

        table_columns = golden.get("table_columns", {"status", "machine_id", "data"})
        where = golden["where"]

        # Normalize WHERE clause
        clause = repo._normalize_where(where, "tv_allocation", table_columns)
        sql, params = clause.to_sql()

        # Verify SQL contains expected fragments
        sql_str = sql.as_string(None)
        for expected_fragment in golden.get("expected_sql_contains", []):
            assert expected_fragment in sql_str, (
                f"Golden test '{golden['name']}' failed: "
                f"Expected fragment '{expected_fragment}' not found in SQL: {sql_str}"
            )

        # Verify parameter count or exact params
        if "expected_params" in golden:
            assert params == golden["expected_params"], (
                f"Golden test '{golden['name']}' failed: "
                f"Expected params {golden['expected_params']}, got {params}"
            )
        elif "expected_param_count" in golden:
            assert len(params) == golden["expected_param_count"], (
                f"Golden test '{golden['name']}' failed: "
                f"Expected {golden['expected_param_count']} params, got {len(params)}"
            )

    def test_golden_queries_comprehensive_coverage(self):
        """Verify golden tests cover all major WHERE patterns."""
        golden_names = {g["name"] for g in GOLDEN_QUERIES}

        required_patterns = {
            "simple_equality",
            "simple_eq_operator",
            "in_operator",
            "fk_nested_filter",
            "jsonb_nested_filter",
            "or_operator",
            "contains_string_operator",
            "isnull_operator",
        }

        assert required_patterns.issubset(golden_names), (
            f"Missing golden tests for: {required_patterns - golden_names}"
        )
```

### Step 6: Code Metrics

Before cleanup:
```bash
# Count lines in WHERE processing
grep -n "def _.*where" src/fraiseql/db.py | wc -l
```

After cleanup:
```bash
# Should be significantly less
```

Expected reduction: **500-800 lines** of code removed.

## Verification Commands

```bash
# Run golden file tests FIRST (ensure no regressions)
uv run pytest tests/regression/test_where_golden.py -v

# Run all tests (must pass)
uv run pytest tests/ -v -x

# Verify no deprecation warnings in normal usage
uv run pytest tests/ -v -W error::DeprecationWarning

# Check code coverage
uv run pytest tests/ --cov=fraiseql --cov-report=term-missing

# Verify performance unchanged
# Compare before/after metrics if available
```

## Acceptance Criteria

- [ ] **Golden file regression tests pass (SQL output unchanged)**
- [ ] `_where_obj_to_dict()` removed
- [ ] Old `_build_where_clause()` branching removed
- [ ] Duplicate `_is_nested_object_filter()` removed
- [ ] Old `_convert_dict_where_to_sql()` simplified or deprecated
- [ ] 500+ lines of code removed
- [ ] All tests pass
- [ ] No regressions (verified by golden tests)
- [ ] Code coverage maintained or improved

## DO NOT

- ❌ Remove public APIs
- ❌ Break backward compatibility without deprecation warnings
- ❌ Delete tests

## Notes

This phase should feel **liberating** - removing complexity, reducing duplication, making the codebase cleaner.

Document removed code in CHANGELOG:
```markdown
### Internal Refactoring
- Removed old WHERE clause processing paths
- Simplified WHERE logic with canonical representation
- 50% reduction in WHERE-related code
```

## Next Phase

**Phase 7:** Performance optimization and caching.
