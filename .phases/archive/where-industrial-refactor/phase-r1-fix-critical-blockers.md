# Phase R1: Fix Critical Blockers [RED]

**Status**: NOT STARTED
**Priority**: 🔥 CRITICAL
**Duration**: 2 days
**Risk**: HIGH

---

## Objective

Fix all critical blockers preventing tests from passing. Get test pass rate from 98.5% (with 73 regressions) to 99.5%+ by completing SQL generation integration and removing broken old code.

---

## Context

**Current State**:
- 73 test failures introduced by incomplete Phase 4 migration
- Old code paths deleted but new code not fully working
- SQL generation returns None in some cases
- Missing test fixtures

**Root Causes**:
1. JSONB path SQL generation incomplete/incorrect
2. Old methods removed but still referenced by tests
3. `_normalize_where()` returns None in some edge cases
4. Missing `setup_hybrid_table` fixture for parameter binding tests

**Target State**:
- All SQL generation working correctly
- Old code fully removed or replaced
- Test pass rate >99.5%

---

## Implementation Steps

### Step 1: Fix JSONB Path SQL Generation (4 hours)

**Problem**: `FieldCondition.to_sql()` returns None for JSONB paths in some cases

**Location**: `src/fraiseql/where_clause.py:175-209`

**Fix**:
```python
# Current code has bug at line 184 (builds SQL object but sometimes incomplete)

def to_sql(self) -> tuple[Composed, list[Any]]:
    """Generate SQL for this condition."""
    params = []

    # ... existing FK column logic (lines 158-174) ...

    elif self.lookup_strategy == "jsonb_path":
        # JSONB path lookup: data->'device'->>'name' = %s
        sql_op = ALL_OPERATORS[self.operator]

        # Build JSONB path: data->'device'->>'name'
        if not self.jsonb_path:
            raise ValueError("jsonb_path required for jsonb_path lookup")

        # FIX: Ensure we always build complete Composed object
        # Build the JSONB path as a Composed expression with proper escaping
        path_parts = [Identifier(self.target_column)]  # FIX: Use Identifier, not SQL

        # Add intermediate keys with ->
        for key in self.jsonb_path[:-1]:
            path_parts.extend([SQL(" -> "), Literal(str(key))])

        # Add final key with ->> (text extraction)
        path_parts.extend([SQL(" ->> "), Literal(str(self.jsonb_path[-1]))])

        jsonb_expr = Composed(path_parts)

        # FIX: Ensure all branches return Composed object
        if self.operator in CONTAINMENT_OPERATORS:
            sql = Composed([jsonb_expr, SQL(f" {sql_op} "), SQL("%s")])
            params.append(tuple(self.value) if isinstance(self.value, list) else self.value)
        elif self.operator == "isnull":
            null_op = "IS NULL" if self.value else "IS NOT NULL"
            sql = Composed([jsonb_expr, SQL(f" {null_op}")])
        elif self.operator in STRING_OPERATORS:
            # LIKE/ILIKE with pattern
            pattern = self._build_like_pattern()
            sql = Composed([jsonb_expr, SQL(f" {sql_op} "), SQL("%s")])
            params.append(pattern)
        else:
            # FIX: Always create Composed object, even for basic operators
            sql = Composed([jsonb_expr, SQL(f" {sql_op} "), SQL("%s")])
            params.append(str(self.value))  # JSONB text comparison

        return sql, params

    # ... rest of method ...
```

**Verification**:
```bash
# Test JSONB path generation
uv run pytest tests/unit/test_where_clause.py::TestFieldCondition::test_jsonb_condition_to_sql -v

# Test golden JSONB tests
uv run pytest tests/regression/test_where_golden.py -k "jsonb" -v
```

---

### Step 2: Fix `_normalize_where()` Edge Cases (2 hours)

**Problem**: `_normalize_where()` returns None when it should raise error or return empty clause

**Location**: `src/fraiseql/db.py:1446-1490`

**Fix**:
```python
def _normalize_where(
    self,
    where: dict | Any,
    view_name: str,
    table_columns: set[str] | None = None,
) -> WhereClause:
    """Normalize WHERE clause to canonical WhereClause representation."""

    # Already normalized
    if isinstance(where, WhereClause):
        return where

    # Dict-based WHERE
    if isinstance(where, dict):
        # FIX: Handle empty dict
        if not where:
            raise ValueError("WHERE clause cannot be empty dict")

        jsonb_column = "data"
        if view_name in _table_metadata:
            metadata = _table_metadata[view_name]
            if metadata.get("has_jsonb_data", False):
                jsonb_column = metadata.get("jsonb_column", "data")

        return normalize_dict_where(where, view_name, table_columns, jsonb_column)

    # WhereInput-based WHERE
    if hasattr(where, "_to_whereinput_dict"):
        jsonb_column = "data"
        if view_name in _table_metadata:
            metadata = _table_metadata[view_name]
            if metadata.get("has_jsonb_data", False):
                jsonb_column = metadata.get("jsonb_column", "data")

        return normalize_whereinput(where, view_name, table_columns, jsonb_column)

    # FIX: Always raise error for unsupported types, never return None
    raise TypeError(
        f"WHERE clause must be dict, WhereClause, or WhereInput object. "
        f"Got: {type(where).__name__}"
    )
```

**Verification**:
```bash
# Test edge cases
uv run pytest tests/unit/test_where_normalization.py -v
```

---

### Step 3: Update `_build_where_clause()` Integration (2 hours)

**Problem**: `_build_where_clause()` not fully integrated with new normalization

**Location**: `src/fraiseql/db.py:1492+`

**Current Code Issues**:
- Returns `list[Any]` but should return `tuple[list[Composed], list[Any]]`
- Doesn't handle all kwargs consistently

**Fix**:
```python
def _build_where_clause(self, view_name: str, **kwargs: Any) -> tuple[list[Any], list[Any]]:
    """Build WHERE clause parts from kwargs.

    Returns:
        Tuple of (where_parts: list[Composed], params: list[Any])
    """
    from psycopg.sql import SQL, Composed, Identifier, Literal

    where_parts = []
    all_params = []

    # Handle WHERE clause
    where_obj = kwargs.get("where")
    if where_obj is not None:
        # Get table columns for this view
        table_columns = None
        if view_name in _table_metadata:
            metadata = _table_metadata[view_name]
            table_columns = metadata.get("columns")

        # Normalize to WhereClause
        try:
            where_clause = self._normalize_where(where_obj, view_name, table_columns)

            # Generate SQL from WhereClause
            sql, params = where_clause.to_sql()

            if sql:
                where_parts.append(sql)
                all_params.extend(params)
        except Exception as e:
            logger.error(f"WHERE clause normalization failed: {e}")
            raise

    # Handle tenant_id (if in kwargs and not in WHERE)
    tenant_id = kwargs.get("tenant_id") or self.context.get("tenant_id")
    if tenant_id and "tenant_id" not in str(where_obj):
        tenant_sql = Composed([
            Identifier("tenant_id"),
            SQL(" = "),
            SQL("%s")
        ])
        where_parts.append(tenant_sql)
        all_params.append(tenant_id)

    return where_parts, all_params
```

**Verification**:
```bash
# Test WHERE clause building
uv run pytest tests/integration/database/repository/ -k "where" -v
```

---

### Step 4: Remove All Old Code References (3 hours)

**Problem**: Tests reference deleted methods

**Strategy**: Update tests to use new code path, not old methods

**Files to Fix**:

#### 4a. `tests/unit/db/test_nested_jsonb_path_builder.py`
**Issue**: Tests `_build_nested_jsonb_path()` which is now internal to WhereClause

**Fix**: Either delete these tests OR update to test via public API
```python
# OLD (tests deleted method):
def test_build_nested_jsonb_path_basic_functionality(self):
    result = self.repo._build_nested_jsonb_path("device", "name")
    # ...

# NEW (test via normalization):
def test_nested_jsonb_path_via_normalization(self):
    where = {"device": {"name": {"eq": "Printer"}}}
    clause = self.repo._normalize_where(where, "test_view", {"id", "data"})
    sql, params = clause.to_sql()

    # Verify JSONB path in SQL
    sql_str = sql.as_string(None)
    assert "data" in sql_str
    assert "'device'" in sql_str
    assert "'name'" in sql_str
```

**Recommendation**: Delete file, functionality tested via integration tests

#### 4b. `tests/unit/db/test_nested_object_filter_detection.py`
**Issue**: Tests `_is_nested_object_filter()` and `_convert_dict_where_to_sql()`

**Fix**: Delete file, functionality tested via normalization tests
- Logic moved to `where_normalization.py:_is_nested_object_filter()`
- Tested in `tests/unit/test_where_normalization.py`

#### 4c. `tests/unit/repository/test_field_name_mapping.py`
**Issue**: Tests `_convert_dict_where_to_sql()` directly

**Fix**: Update to test via `_normalize_where()` and `to_sql()`
```python
# OLD:
sql_parts = repo._convert_dict_where_to_sql(where, table_columns)

# NEW:
clause = repo._normalize_where(where, "test_view", table_columns)
sql, params = clause.to_sql()
sql_str = sql.as_string(None)
```

#### 4d. `tests/integration/repository/test_field_name_mapping_integration.py`
**Issue**: Same as 4c

**Fix**: Same approach

#### 4e. `tests/integration/repository/test_repository_find_where_processing.py`
**Issue**: Tests operator strategy system (old implementation)

**Fix**: Update to verify WhereClause operators
```python
def test_repository_find_should_use_new_where_system(self):
    """Verify repository uses WhereClause normalization."""
    where = {"status": {"eq": "active"}}

    clause = repo._normalize_where(where, "test_view", {"status"})

    assert isinstance(clause, WhereClause)
    assert len(clause.conditions) == 1
    assert clause.conditions[0].operator == "eq"
```

---

### Step 5: Create Missing Test Fixture (1 hour)

**Problem**: `setup_hybrid_table` fixture missing

**Location**: `tests/conftest.py` or new `tests/integration/conftest.py`

**Implementation**:
```python
import pytest
import uuid
from fraiseql.db import FraiseQLRepository, register_type_for_view

@pytest.fixture
async def setup_hybrid_table(class_db_pool):
    """Set up hybrid table (machine + tv_allocation) for testing.

    Creates:
    - machine table (FK target)
    - tv_allocation table (hybrid: machine_id FK + data JSONB)
    - Sample data for testing

    Returns:
        dict with test data IDs
    """
    async with class_db_pool.connection() as conn, conn.cursor() as cursor:
        # Create machine table
        await cursor.execute("""
            CREATE TABLE IF NOT EXISTS machine (
                id UUID PRIMARY KEY,
                name TEXT
            )
        """)

        # Create tv_allocation hybrid table
        await cursor.execute("""
            CREATE TABLE IF NOT EXISTS tv_allocation (
                id UUID PRIMARY KEY,
                machine_id UUID REFERENCES machine(id),
                status TEXT,
                name TEXT,
                data JSONB,
                created_at TIMESTAMP DEFAULT NOW()
            )
        """)

        # Insert test machines
        machine1_id = uuid.uuid4()
        machine2_id = uuid.uuid4()

        await cursor.execute(
            "INSERT INTO machine (id, name) VALUES (%s, %s), (%s, %s)",
            (machine1_id, "Machine 1", machine2_id, "Machine 2")
        )

        # Insert test allocations
        alloc1_id = uuid.uuid4()
        alloc2_id = uuid.uuid4()

        await cursor.execute("""
            INSERT INTO tv_allocation (id, machine_id, status, name, data)
            VALUES
                (%s, %s, 'active', 'Test Allocation 1', '{"device": {"name": "Device1"}}'::jsonb),
                (%s, %s, 'pending', 'Test Allocation 2', '{"device": {"name": "Device2"}}'::jsonb)
        """, (alloc1_id, machine1_id, alloc2_id, machine2_id))

        await conn.commit()

        # Register type metadata
        register_type_for_view(
            "tv_allocation",
            object,  # Dummy type for testing
            table_columns={"id", "machine_id", "status", "name", "data", "created_at"},
            fk_relationships={"machine": "machine_id"},
            has_jsonb_data=True,
            jsonb_column="data"
        )

        yield {
            "machine1_id": machine1_id,
            "machine2_id": machine2_id,
            "alloc1_id": alloc1_id,
            "alloc2_id": alloc2_id,
        }

        # Cleanup
        await cursor.execute("DROP TABLE IF EXISTS tv_allocation CASCADE")
        await cursor.execute("DROP TABLE IF EXISTS machine CASCADE")
        await conn.commit()
```

**Verification**:
```bash
uv run pytest tests/integration/test_parameter_binding.py -v
```

---

### Step 6: Fix Remaining SQL Generation Issues (3 hours)

**Problem**: "SQL values must be strings, got None instead" errors

**Root Cause**: Composed SQL objects contain None values

**Debug Strategy**:
```python
# Add debug logging to FieldCondition.to_sql()
def to_sql(self) -> tuple[Composed, list[Any]]:
    """Generate SQL for this condition."""
    # ... existing code ...

    # ADD VALIDATION before return
    if sql is None:
        raise ValueError(
            f"SQL generation returned None for condition: {self!r}"
        )

    # Validate Composed object doesn't contain None
    if isinstance(sql, Composed):
        for part in sql._obj:  # Internal access for debugging
            if part is None:
                raise ValueError(
                    f"Composed SQL contains None part: {self!r}"
                )

    return sql, params
```

**Fix Likely Issues**:
1. `Identifier(None)` or `SQL(None)` somewhere
2. `target_column` is None in some cases
3. `jsonb_path` list contains None

**Systematic Fix**:
```bash
# Run one failing test with verbose output
uv run pytest tests/regression/test_nested_filter_id_field.py::TestNestedFilterIdField::test_nested_filter_on_related_field_jsonb_scenario -vv -s

# Look for traceback showing where None is introduced
# Fix that specific case
# Repeat for next failure
```

---

### Step 7: Remove Dead Code (30 minutes)

**Location**: `src/fraiseql/where_normalization.py:340-359`

**Action**: Delete lines 340-359 (unreachable duplicate code)

**Verification**: Ensure tests still pass

---

## Verification Commands

### Quick Verification (After Each Step)
```bash
# Step 1 verification
uv run pytest tests/unit/test_where_clause.py -v

# Step 2 verification
uv run pytest tests/unit/test_where_normalization.py -v

# Step 3 verification
uv run pytest tests/integration/database/repository/ -k "where" -v --maxfail=5

# Step 4 verification
uv run pytest tests/unit/db/ -v
uv run pytest tests/unit/repository/ -v

# Step 5 verification
uv run pytest tests/integration/test_parameter_binding.py -v

# Step 6 verification
uv run pytest tests/regression/test_where_golden.py -v
uv run pytest tests/regression/test_nested_filter_id_field.py -v
```

### Full Verification (End of Phase)
```bash
# Run all tests
uv run pytest tests/ -v --tb=short

# Check pass rate
uv run pytest tests/ -v | grep -E "(passed|failed)"

# Target: >99.5% pass rate (expect <20 failures out of 4,900)
```

---

## Acceptance Criteria

### Must Have ✅
- [ ] All JSONB path SQL generation working (no None returns)
- [ ] `_normalize_where()` handles all edge cases (no None returns)
- [ ] `_build_where_clause()` fully integrated with new system
- [ ] All tests referencing old methods updated or deleted
- [ ] `setup_hybrid_table` fixture created
- [ ] Dead code removed

### Test Results ✅
- [ ] `tests/unit/test_where_clause.py`: 29/29 passing
- [ ] `tests/unit/test_where_clause_security.py`: 5/5 passing
- [ ] `tests/unit/test_where_normalization.py`: 14/14 passing
- [ ] `tests/integration/test_parameter_binding.py`: 6/6 passing
- [ ] `tests/regression/test_where_golden.py`: 13/13 passing
- [ ] Overall test pass rate: >99.5% (expect <20 failures)

### Quality ✅
- [ ] No code smells (ruff passes)
- [ ] No dead code
- [ ] All edge cases handled with clear errors
- [ ] Logging comprehensive

---

## DO NOT

❌ **DO NOT** add backward compatibility code
❌ **DO NOT** keep old code paths
❌ **DO NOT** add feature flags (single code path only)
❌ **DO NOT** skip test verification
❌ **DO NOT** move to Phase R2 until ALL acceptance criteria met

---

## Rollback Plan

**If Phase R1 Cannot Be Completed**:
1. Document specific blocker in this file
2. Revert all changes: `git checkout main -- src/fraiseql/`
3. Re-assess strategy
4. **NOT EXPECTED** - issues are well-understood and fixable

---

## Time Estimates

| Step | Optimistic | Realistic | Pessimistic |
|------|-----------|-----------|-------------|
| 1. JSONB SQL fix | 2h | 4h | 6h |
| 2. Normalize edge cases | 1h | 2h | 3h |
| 3. Build WHERE integration | 1h | 2h | 4h |
| 4. Update test references | 2h | 3h | 5h |
| 5. Create fixture | 0.5h | 1h | 2h |
| 6. Fix SQL None errors | 2h | 3h | 5h |
| 7. Remove dead code | 0.5h | 0.5h | 1h |
| **TOTAL** | **9h** | **15.5h** | **26h** |

**Realistic Timeline**: 2 days (8h/day = 16h)

---

## Progress Tracking

### Day 1
- [ ] Steps 1-3 complete
- [ ] Unit tests passing
- [ ] Integration tests 50% passing

### Day 2
- [ ] Steps 4-7 complete
- [ ] All tests passing
- [ ] Phase R1 acceptance criteria met

---

## Notes / Issues

**Date**: [Add date when starting]
**Developer**: [Add name]

### Blockers
- None yet

### Discoveries
- [Add any discoveries during implementation]

### Decisions
- [Add any key decisions made]

---

**Phase Status**: NOT STARTED
**Next Phase**: [phase-r2-implement-missing-operators.md](phase-r2-implement-missing-operators.md)
