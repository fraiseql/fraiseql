# Post-Release Implementation Plan: v0.11.6 Features

**Target Version**: v0.11.6
**Current Status**: v0.11.5 ready to publish (3,508 tests passing, 44 skipped, 0 failing)
**Timeline**: 2-4 weeks post-release
**Complexity**: Medium-High

---

## 🎯 Overview of Skipped Features

### Currently Skipped Tests (44 total):

1. **Nested Object Filtering** (3 tests) - HIGH PRIORITY
   - Feature: Filter on nested JSONB objects
   - Impact: Enables complex hybrid table queries
   - Estimated effort: 2-3 days

2. **Coordinate Datatype** (10 tests) - MEDIUM PRIORITY
   - Feature: Geographic coordinates with spatial operations
   - Impact: Enables location-based queries
   - Estimated effort: 3-5 days

3. **TypeName Integration** (3 tests) - LOW PRIORITY
   - Feature: __typename in mocked resolvers
   - Impact: Integration test completeness
   - Estimated effort: 2-3 days

4. **Example Templates** (10 tests) - MAINTENANCE
   - Feature: Blog example database templates
   - Impact: Example applications
   - Estimated effort: 1-2 days

5. **Other Tests** (18 tests) - VARIOUS
   - Mixed mode tests (obsolete)
   - JSON parsing validation
   - Dual mode repository (obsolete)

---

## 📋 Implementation Roadmap

### Phase 1: Nested Object Filtering (Week 1-2)
**Priority**: HIGH
**Complexity**: Medium-High
**Tests to Enable**: 3
**User Impact**: High (enables important use cases)

### Phase 2: Coordinate Datatype (Week 2-3)
**Priority**: MEDIUM
**Complexity**: Medium
**Tests to Enable**: 10
**User Impact**: Medium (niche but valuable)

### Phase 3: TypeName Integration (Week 3-4)
**Priority**: LOW
**Complexity**: Medium
**Tests to Enable**: 3
**User Impact**: Low (test coverage only)

### Phase 4: Example Templates (Week 4)
**Priority**: LOW
**Complexity**: Low
**Tests to Enable**: 10
**User Impact**: Low (developer experience)

---

## 🔥 Feature 1: Nested Object Filtering

### Problem Statement

**Current behavior**:
```python
# This query FAILS (filter ignored)
where = {
    "machine": {
        "name": {"eq": "Machine 1"}
    }
}
results = await repo.find("allocations", where=where)
# Returns ALL allocations (filter not applied)
```

**Expected behavior**:
```python
# Should filter where machine.name = 'Machine 1'
# SQL: WHERE data->'machine'->>'name' = 'Machine 1'
results = await repo.find("allocations", where=where)
# Returns only allocations for Machine 1
```

**Use case**: Filtering on nested objects in hybrid tables (SQL columns + JSONB data)

---

### Implementation Plan

#### Phase 1.1: Understand Current WHERE Clause Builder (2-4 hours)

**Goal**: Map out how WHERE clauses are currently built

**Files to study**:
```
src/fraiseql/sql/where/core/sql_builder.py    - Main WHERE builder
src/fraiseql/sql/where_generator.py            - WHERE clause generation
src/fraiseql/sql/graphql_where_generator.py    - GraphQL WHERE input
src/fraiseql/sql/operator_strategies.py         - Operator handling
```

**Key questions**:
1. How does the current builder handle nested dicts?
2. Where does it stop recursing into nested objects?
3. How are JSONB paths currently built?
4. How does it detect operators vs nested objects?

**Deliverable**: Architecture diagram showing current WHERE clause flow

---

#### Phase 1.2: Design Nested Object Filtering (4-6 hours)

**Goal**: Design recursive WHERE clause builder supporting nested objects

**Design decisions**:

**1. Operator Detection**
```python
def is_operator_dict(d: dict) -> bool:
    """Check if dict contains operators vs nested objects."""
    operators = {"eq", "neq", "gt", "gte", "lt", "lte",
                 "ilike", "like", "in", "notin",
                 "contains", "startswith", "endswith"}
    return any(k in operators for k in d.keys())

# Example:
is_operator_dict({"eq": "value"})  # True - has operator
is_operator_dict({"name": {"eq": "value"}})  # False - nested field
```

**2. JSONB Path Building**
```python
def build_jsonb_path(fields: list[str]) -> SQL:
    """Build JSONB navigation path.

    Examples:
        ["machine", "name"] → data->'machine'->>'name'
        ["location", "address", "city"] → data->'location'->'address'->>'city'
    """
    if len(fields) == 1:
        # Single field: data->>'field'
        return SQL("data->>'{}'").format(Literal(fields[0]))
    else:
        # Nested: data->'field1'->'field2'->>'field3'
        path = "data"
        for i, field in enumerate(fields):
            if i == len(fields) - 1:
                # Last field uses ->> (text extraction)
                path += f"->>{{{field}}}"
            else:
                # Intermediate fields use -> (JSONB navigation)
                path += f"->{{{field}}}"
        return SQL(path)
```

**3. Recursive WHERE Clause Builder**
```python
def build_where_clause(
    where_dict: dict,
    path: list[str] = None
) -> tuple[Composed, dict]:
    """Build WHERE clause with nested object support.

    Args:
        where_dict: WHERE clause dictionary
        path: Current path in JSONB tree (for recursion)

    Returns:
        (SQL WHERE clause, parameters dict)

    Examples:
        # Flat filter
        where = {"status": {"eq": "active"}}
        → WHERE data->>'status' = %(param_0)s

        # Nested filter
        where = {"machine": {"name": {"eq": "Machine 1"}}}
        → WHERE data->'machine'->>'name' = %(param_0)s
    """
    conditions = []
    params = {}
    path = path or []

    for field, value in where_dict.items():
        if isinstance(value, dict) and not is_operator_dict(value):
            # Nested object - recurse deeper
            nested_path = path + [field]
            nested_sql, nested_params = build_where_clause(value, nested_path)
            conditions.append(nested_sql)
            params.update(nested_params)
        else:
            # Leaf node with operators
            full_path = path + [field]
            jsonb_path = build_jsonb_path(full_path)

            # Handle operators on this field
            for operator, op_value in value.items():
                condition_sql = build_operator_condition(
                    jsonb_path,
                    operator,
                    op_value
                )
                conditions.append(condition_sql)
                params[f"param_{len(params)}"] = op_value

    # Combine with AND
    where_sql = SQL(" AND ").join(conditions)
    return where_sql, params
```

**4. Integration Points**

Update these files:
```
src/fraiseql/sql/where/core/sql_builder.py:
  - Add recursive_build_where_clause()
  - Detect nested objects vs operators
  - Build JSONB paths correctly

src/fraiseql/sql/where_generator.py:
  - Call recursive builder
  - Handle nested object WHERE clauses

src/fraiseql/sql/operator_strategies.py:
  - Ensure strategies work with nested JSONB paths
  - Handle data->'machine'->>'name' syntax
```

**Deliverable**: Design document with code examples and SQL output

---

#### Phase 1.3: Implement Recursive WHERE Builder (TDD) (8-12 hours)

**RED Phase: Write Failing Tests**

**File**: `tests/unit/sql/where/test_nested_object_where_builder.py`

```python
"""Test nested object WHERE clause building."""

import pytest
from fraiseql.sql.where.core.sql_builder import build_where_clause


class TestNestedObjectWhereBuilder:
    """Test WHERE clause builder with nested object support."""

    def test_flat_where_clause(self):
        """Test basic flat WHERE clause (existing functionality)."""
        where = {"status": {"eq": "active"}}
        sql, params = build_where_clause(where)

        assert "data->>'status'" in sql.as_string(None)
        assert " = " in sql.as_string(None)
        assert params == {"param_0": "active"}

    def test_single_level_nested_where(self):
        """Test one level of nesting."""
        where = {"machine": {"name": {"eq": "Machine 1"}}}
        sql, params = build_where_clause(where)

        # Should generate: data->'machine'->>'name' = %(param_0)s
        sql_str = sql.as_string(None)
        assert "data->'machine'->>'name'" in sql_str
        assert " = " in sql_str
        assert params == {"param_0": "Machine 1"}

    def test_two_level_nested_where(self):
        """Test two levels of nesting."""
        where = {
            "location": {
                "address": {
                    "city": {"eq": "Seattle"}
                }
            }
        }
        sql, params = build_where_clause(where)

        # Should generate: data->'location'->'address'->>'city' = %(param_0)s
        sql_str = sql.as_string(None)
        assert "data->'location'->'address'->>'city'" in sql_str
        assert params == {"param_0": "Seattle"}

    def test_multiple_nested_conditions(self):
        """Test multiple conditions at different nesting levels."""
        where = {
            "status": {"eq": "active"},
            "machine": {
                "name": {"eq": "Machine 1"},
                "type": {"eq": "Server"}
            }
        }
        sql, params = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data->>'status'" in sql_str
        assert "data->'machine'->>'name'" in sql_str
        assert "data->'machine'->>'type'" in sql_str
        assert " AND " in sql_str
        assert len(params) == 3

    def test_mixed_operators_nested(self):
        """Test different operators on nested objects."""
        where = {
            "machine": {
                "power": {"gte": 100},
                "status": {"neq": "offline"}
            }
        }
        sql, params = build_where_clause(where)

        sql_str = sql.as_string(None)
        assert "data->'machine'->>'power'" in sql_str
        assert ">=" in sql_str
        assert "data->'machine'->>'status'" in sql_str
        assert "!=" in sql_str
```

**Expected**: All tests FAIL (functionality not implemented)

---

**GREEN Phase: Implement Minimal Functionality**

**File**: `src/fraiseql/sql/where/core/sql_builder.py`

Add recursive WHERE clause builder:

```python
from psycopg.sql import SQL, Composed, Literal
from typing import Any


def is_operator_dict(d: dict) -> bool:
    """Check if dict contains operators vs nested objects."""
    operators = {
        "eq", "neq", "gt", "gte", "lt", "lte",
        "ilike", "like", "in", "notin",
        "contains", "startswith", "endswith",
        "is_null", "is_not_null"
    }
    return any(k in operators for k in d.keys())


def build_jsonb_path(fields: list[str]) -> Composed:
    """Build JSONB navigation path for nested objects.

    Args:
        fields: List of field names from root to leaf

    Returns:
        SQL composed object with JSONB path

    Examples:
        ["status"] → data->>'status'
        ["machine", "name"] → data->'machine'->>'name'
        ["location", "address", "city"] → data->'location'->'address'->>'city'
    """
    if not fields:
        raise ValueError("Fields list cannot be empty")

    if len(fields) == 1:
        # Single field: data->>'field'
        return Composed([
            SQL("data->>"),
            Literal(fields[0])
        ])

    # Multiple fields: data->'field1'->'field2'->>'field3'
    parts = [SQL("data")]

    for i, field in enumerate(fields):
        if i == len(fields) - 1:
            # Last field: ->> (text extraction)
            parts.append(SQL("->>"))
            parts.append(Literal(field))
        else:
            # Intermediate fields: -> (JSONB navigation)
            parts.append(SQL("->"))
            parts.append(Literal(field))

    return Composed(parts)


def build_where_clause_recursive(
    where_dict: dict,
    path: list[str] = None,
    param_counter: dict = None
) -> tuple[list[Composed], dict]:
    """Recursively build WHERE clause with nested object support.

    Args:
        where_dict: WHERE clause dictionary
        path: Current field path in JSONB tree
        param_counter: Counter for parameter naming

    Returns:
        (List of SQL conditions, parameters dict)
    """
    if path is None:
        path = []
    if param_counter is None:
        param_counter = {"count": 0}

    conditions = []
    params = {}

    for field, value in where_dict.items():
        if isinstance(value, dict) and not is_operator_dict(value):
            # Nested object - recurse deeper
            nested_path = path + [field]
            nested_conditions, nested_params = build_where_clause_recursive(
                value,
                nested_path,
                param_counter
            )
            conditions.extend(nested_conditions)
            params.update(nested_params)
        else:
            # Leaf node with operators
            full_path = path + [field]
            jsonb_path = build_jsonb_path(full_path)

            # Handle operators on this field
            if isinstance(value, dict):
                for operator, op_value in value.items():
                    param_name = f"param_{param_counter['count']}"
                    param_counter['count'] += 1

                    # Build operator condition
                    condition = build_operator_sql(
                        jsonb_path,
                        operator,
                        param_name,
                        op_value
                    )
                    conditions.append(condition)
                    params[param_name] = op_value

    return conditions, params


def build_operator_sql(
    jsonb_path: Composed,
    operator: str,
    param_name: str,
    value: Any
) -> Composed:
    """Build SQL for a specific operator.

    Args:
        jsonb_path: JSONB path to field
        operator: Operator name (eq, gte, ilike, etc.)
        param_name: Parameter placeholder name
        value: Value for comparison

    Returns:
        SQL condition
    """
    # Use operator strategies from existing system
    from fraiseql.sql.operator_strategies import get_operator_registry

    registry = get_operator_registry()

    # Build SQL using operator strategy
    return registry.build_sql(
        path_sql=jsonb_path,
        op=operator,
        val=value,
        field_type=None  # Will auto-detect from value
    )


def build_where_clause(where_dict: dict) -> tuple[Composed, dict]:
    """Main entry point for WHERE clause building.

    Args:
        where_dict: WHERE clause dictionary

    Returns:
        (WHERE SQL clause, parameters dict)
    """
    if not where_dict:
        return SQL("TRUE"), {}

    conditions, params = build_where_clause_recursive(where_dict)

    if not conditions:
        return SQL("TRUE"), {}

    # Combine all conditions with AND
    where_sql = Composed([
        conditions[0]
    ])

    for condition in conditions[1:]:
        where_sql = Composed([
            where_sql,
            SQL(" AND "),
            condition
        ])

    return where_sql, params
```

**Expected**: Tests pass with minimal implementation

---

**REFACTOR Phase: Improve Code Quality**

1. **Extract helper functions**
2. **Add comprehensive error handling**
3. **Support NULL operators correctly**
4. **Handle edge cases** (empty paths, None values)
5. **Add type hints**
6. **Improve SQL formatting**

---

**QA Phase: Integration Testing**

**File**: `tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py`

Remove `@pytest.mark.skip` decorators and run tests:

```bash
uv run pytest tests/integration/database/repository/test_hybrid_table_nested_object_filtering.py -v
```

**Expected**: All 3 tests pass ✅

---

#### Phase 1.4: Integration with Repository Layer (2-4 hours)

**Goal**: Integrate recursive WHERE builder with FraiseQLRepository

**Files to modify**:
```
src/fraiseql/db.py:
  - Update find() method to use new builder
  - Pass WHERE dict to recursive builder
  - Ensure backward compatibility

src/fraiseql/sql/where_generator.py:
  - Route to recursive builder when nested objects detected
  - Preserve existing flat WHERE functionality
```

**Test**:
```python
@pytest.mark.asyncio
async def test_nested_filtering_integration(db_pool):
    """End-to-end test of nested object filtering."""
    repo = FraiseQLRepository(db_pool)

    # Test nested filter
    result = await repo.find(
        "allocations",
        where={
            "machine": {
                "name": {"eq": "Machine 1"}
            }
        }
    )

    results = extract_graphql_data(result, "allocations")
    assert len(results) > 0
    assert all(r["machine"]["name"] == "Machine 1" for r in results)
```

---

#### Phase 1.5: Documentation & Examples (2-3 hours)

**Goal**: Document nested object filtering feature

**Files to create/update**:
```
docs/core/queries-and-mutations.md:
  - Add nested object filtering section
  - Show examples with hybrid tables

examples/nested_filtering_example.py:
  - Complete working example
  - Multiple nesting levels
  - Combined with regular filters

docs/migration-guides/v0.11.6.md:
  - New feature announcement
  - Migration notes
  - Breaking changes (if any)
```

**Example documentation**:

```markdown
## Nested Object Filtering

FraiseQL v0.11.6+ supports filtering on nested objects in JSONB fields:

### Basic Nested Filter

```python
where = {
    "machine": {
        "name": {"eq": "Server-01"}
    }
}
```

Generates SQL:
```sql
WHERE data->'machine'->>'name' = 'Server-01'
```

### Multiple Levels

```python
where = {
    "location": {
        "address": {
            "city": {"eq": "Seattle"}
        }
    }
}
```

### Combined Filters

```python
where = {
    "status": {"eq": "active"},
    "machine": {
        "type": {"eq": "Server"},
        "power": {"gte": 100}
    }
}
```
```

---

### Phase 1 Summary

**Timeline**: 2-3 days
**Tests Fixed**: 3
**Files Modified**: ~6
**Files Created**: ~3
**User Impact**: HIGH (important feature)

**Deliverables**:
- ✅ Recursive WHERE clause builder
- ✅ JSONB path navigation
- ✅ Integration with repository
- ✅ Unit tests passing
- ✅ Integration tests passing
- ✅ Documentation complete
- ✅ Examples working

---

## 🌍 Feature 2: Coordinate Datatype

### Problem Statement

**Current situation**: Coordinate type exists but operators not fully implemented

**Skipped tests**:
```python
@pytest.mark.skip(reason="Coordinate datatype feature is incomplete")
class TestCoordinateFilterOperations:
    def test_coordinate_eq_operation(self):
        # Test coordinate equality

    def test_coordinate_distance_within_operation(self):
        # Test distance-based filtering
```

**Goal**: Complete coordinate datatype with all operators and PostgreSQL POINT integration

---

### Implementation Plan

#### Phase 2.1: Complete Coordinate Operators (4-6 hours)

**File**: `src/fraiseql/sql/where/operators/coordinate.py`

**Already exists**: `src/fraiseql/types/scalars/coordinates.py` (coordinate parsing/validation)

**Need to add**: Operator strategies for coordinate filtering

```python
"""Coordinate operator strategies for spatial filtering."""

from typing import Any
from psycopg.sql import SQL, Composed, Literal

from fraiseql.sql.operator_strategies import BaseOperatorStrategy


class CoordinateEqualityOperatorStrategy(BaseOperatorStrategy):
    """Handle coordinate equality operations with PostgreSQL POINT type."""

    def __init__(self):
        super().__init__(["eq", "neq"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None
    ) -> Composed:
        """Build SQL for coordinate equality/inequality.

        Converts (lat, lng) to PostgreSQL POINT(lng, lat) format.
        Note: PostgreSQL POINT uses (x, y) = (lng, lat) order.

        Examples:
            (45.5, -122.6) → POINT(-122.6, 45.5)
        """
        if not isinstance(val, tuple) or len(val) != 2:
            raise ValueError(f"Coordinate must be (lat, lng) tuple, got {val}")

        lat, lng = val

        # Cast JSONB to POINT and compare
        # Format: data->>'coordinates'::point = POINT(lng, lat)
        if op == "eq":
            return Composed([
                SQL("("),
                path_sql,
                SQL(")::point = POINT("),
                Literal(lng),
                SQL(","),
                Literal(lat),
                SQL(")")
            ])
        else:  # neq
            return Composed([
                SQL("("),
                path_sql,
                SQL(")::point != POINT("),
                Literal(lng),
                SQL(","),
                Literal(lat),
                SQL(")")
            ])


class CoordinateListOperatorStrategy(BaseOperatorStrategy):
    """Handle coordinate IN/NOT IN operations."""

    def __init__(self):
        super().__init__(["in", "notin"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None
    ) -> Composed:
        """Build SQL for coordinate list membership.

        Examples:
            val = [(45.5, -122.6), (47.6, -122.3)]
            → data->>'coordinates'::point IN (POINT(-122.6, 45.5), POINT(-122.3, 47.6))
        """
        if not isinstance(val, list):
            raise ValueError(f"IN operator requires list, got {type(val)}")

        # Convert all coordinates to POINT literals
        points = []
        for coord in val:
            if not isinstance(coord, tuple) or len(coord) != 2:
                raise ValueError(f"Each coordinate must be (lat, lng) tuple, got {coord}")
            lat, lng = coord
            points.append(Composed([
                SQL("POINT("),
                Literal(lng),
                SQL(","),
                Literal(lat),
                SQL(")")
            ]))

        # Build IN/NOT IN clause
        if op == "in":
            operator = SQL(" IN (")
        else:  # notin
            operator = SQL(" NOT IN (")

        result = Composed([
            SQL("("),
            path_sql,
            SQL(")::point"),
            operator
        ])

        # Add all points
        for i, point in enumerate(points):
            if i > 0:
                result = Composed([result, SQL(", ")])
            result = Composed([result, point])

        result = Composed([result, SQL(")")])
        return result


class CoordinateDistanceOperatorStrategy(BaseOperatorStrategy):
    """Handle coordinate distance-based filtering with PostGIS."""

    def __init__(self):
        super().__init__(["distance_within"])

    def build_sql(
        self,
        path_sql: SQL,
        op: str,
        val: Any,
        field_type: type | None = None
    ) -> Composed:
        """Build SQL for distance-based filtering using PostGIS.

        Args:
            val: Tuple of (center_coordinate, distance_meters)

        Examples:
            val = ((45.5, -122.6), 5000)  # Within 5km of center
            → ST_DWithin(
                  ST_GeographyFromText(data->>'coordinates'::point),
                  ST_GeographyFromText(POINT(-122.6, 45.5)),
                  5000
              )
        """
        if not isinstance(val, tuple) or len(val) != 2:
            raise ValueError(
                f"distance_within requires (center, distance_meters), got {val}"
            )

        center, distance = val

        if not isinstance(center, tuple) or len(center) != 2:
            raise ValueError(f"Center must be (lat, lng) tuple, got {center}")

        if not isinstance(distance, (int, float)) or distance <= 0:
            raise ValueError(f"Distance must be positive number, got {distance}")

        lat, lng = center

        # Use PostGIS ST_DWithin for geographic distance
        # Requires PostGIS extension
        return Composed([
            SQL("ST_DWithin("),
            SQL("ST_GeographyFromText(("),
            path_sql,
            SQL(")::point::text),"),
            SQL("ST_GeographyFromText('POINT("),
            Literal(lng),
            SQL(" "),
            Literal(lat),
            SQL(")'),"),
            Literal(distance),
            SQL(")")
        ])


# Register coordinate operators
def register_coordinate_operators():
    """Register all coordinate operator strategies."""
    from fraiseql.sql.operator_strategies import get_operator_registry

    registry = get_operator_registry()

    registry.register(CoordinateEqualityOperatorStrategy())
    registry.register(CoordinateListOperatorStrategy())
    registry.register(CoordinateDistanceOperatorStrategy())
```

---

#### Phase 2.2: PostgreSQL POINT Type Integration (2-3 hours)

**Goal**: Ensure coordinates are stored as POINT type in PostgreSQL

**File**: `src/fraiseql/migrations/schema_generator.py` (or equivalent)

Add POINT type handling:

```python
def get_column_type_for_field(field_type):
    """Determine PostgreSQL column type for Python field type."""
    # ... existing type mappings ...

    # Coordinate type → POINT
    if field_type == CoordinateField:
        return "POINT"

    # ... other types ...
```

**Migration for existing databases**:

```sql
-- Convert existing coordinate columns to POINT type
-- In migration script
ALTER TABLE your_table
  ALTER COLUMN coordinates
  TYPE POINT
  USING (coordinates::point);

-- Add GiST index for spatial queries
CREATE INDEX idx_your_table_coordinates
  ON your_table
  USING GIST (coordinates);
```

---

#### Phase 2.3: Enable PostGIS Support (1-2 hours)

**Goal**: Document PostGIS requirement for distance operations

**File**: `docs/core/postgresql-extensions.md`

Add PostGIS section:

```markdown
## PostGIS Extension

PostGIS is **required** for coordinate distance-based filtering.

### Installation

```sql
-- Enable PostGIS extension
CREATE EXTENSION IF NOT EXISTS postgis;
```

### Verify Installation

```sql
SELECT PostGIS_Version();
```

### Features Enabled

- `distance_within`: Filter by geographic distance
- Spatial indexing with GiST
- Geographic calculations (meters, not degrees)

### Without PostGIS

If PostGIS is not available:
- ✅ Coordinate equality (`eq`, `neq`)
- ✅ Coordinate lists (`in`, `notin`)
- ❌ Distance-based filtering (`distance_within`)

Distance queries will raise an error with helpful message to install PostGIS.
```

---

#### Phase 2.4: Update Tests & Documentation (3-4 hours)

**Remove skip decorators**:

```python
# tests/integration/database/sql/test_coordinate_filter_operations.py
# Remove: @pytest.mark.skip(reason="...")

class TestCoordinateFilterOperations:
    def test_coordinate_eq_operation(self):
        # Now runs
        ...
```

**Add coordinate examples**:

```python
# examples/coordinate_filtering_example.py

@fraiseql.type
class Location:
    id: int
    name: str
    coordinates: Coordinate  # Geographic point
    address: str


@query
async def nearby_locations(
    center: Coordinate,
    radius_meters: int = 5000
) -> list[Location]:
    """Find locations within distance of center point."""
    repo = info.context["repo"]

    return await repo.find(
        "locations",
        where={
            "coordinates": {
                "distance_within": {
                    "center": center,
                    "radius": radius_meters
                }
            }
        }
    )
```

---

### Phase 2 Summary

**Timeline**: 3-5 days
**Tests Fixed**: 10
**Files Modified**: ~4
**Files Created**: ~2
**User Impact**: MEDIUM (valuable for location-based apps)

**Deliverables**:
- ✅ All coordinate operators implemented
- ✅ PostgreSQL POINT type integration
- ✅ PostGIS distance filtering
- ✅ Unit tests passing
- ✅ Integration tests passing
- ✅ Documentation complete
- ✅ Examples working

---

## 🏷️ Feature 3: TypeName Integration Tests

### Problem Statement

**Current situation**: TypeName injection tests use mocked resolvers instead of real database

**Skipped tests**:
```python
@pytest.mark.skip(reason="Requires full FastAPI + PostgreSQL + Rust pipeline setup")
def test_typename_injected_in_single_object_response(graphql_client):
    ...
```

**Goal**: Refactor tests to use real database + Rust pipeline

---

### Implementation Plan

#### Phase 3.1: Refactor Tests to Use Real Database (4-6 hours)

**File**: `tests/integration/graphql/test_typename_in_responses.py`

**Current approach** (mocked):
```python
# Uses mocked data
MOCK_USERS = {
    uuid.UUID("..."): {"id": "...", "name": "Alice"}
}

@query
async def user(id: uuid.UUID) -> Optional[User]:
    user_data = MOCK_USERS.get(id)  # ❌ Mock
    return User(**user_data)
```

**New approach** (real database):
```python
@pytest.fixture
async def setup_typename_test_data(db_pool):
    """Set up real database with JSONB for typename tests."""
    async with db_pool.connection() as conn:
        # Create table with JSONB
        await conn.execute("""
            CREATE TABLE IF NOT EXISTS v_user (
                id UUID PRIMARY KEY,
                data JSONB NOT NULL
            )
        """)

        # Insert test data
        await conn.execute("""
            INSERT INTO v_user (id, data) VALUES
            (
                '11111111-1111-1111-1111-111111111111',
                '{"id": "11111111-1111-1111-1111-111111111111", "name": "Alice", "email": "alice@example.com"}'
            ),
            (
                '22222222-2222-2222-2222-222222222222',
                '{"id": "22222222-2222-2222-2222-222222222222", "name": "Bob", "email": "bob@example.com"}'
            )
        """)
        await conn.commit()


@query
async def user(info, id: uuid.UUID) -> Optional[User]:
    """Get user from database (not mocked)."""
    repo = info.context["repo"]
    result = await repo.find_one_rust("v_user", "user", info, id=id)
    return result  # RustResponseBytes with __typename


def test_typename_injected_in_single_object_response(
    graphql_client,
    setup_typename_test_data
):
    """Test __typename injection with real Rust pipeline."""
    query = """
    query GetUser {
        user(id: "11111111-1111-1111-1111-111111111111") {
            __typename
            id
            name
            email
        }
    }
    """

    response = graphql_client.post("/graphql", json={"query": query})
    assert response.status_code == 200

    result = response.json()
    assert result["data"]["user"]["__typename"] == "User"
    assert result["data"]["user"]["name"] == "Alice"
```

---

#### Phase 3.2: Verify TypeName Injection (2-3 hours)

**Goal**: Ensure Rust pipeline correctly injects `__typename`

**Check**:
1. Single object queries
2. List queries
3. Mixed queries (multiple types)
4. Null responses

**If failing**, debug Rust pipeline typename injection:

```rust
// fraiseql_rs/src/graphql_response.rs
// Ensure __typename is injected when type_name is provided
```

---

### Phase 3 Summary

**Timeline**: 2-3 days
**Tests Fixed**: 3
**Files Modified**: 1
**User Impact**: LOW (test coverage only)

**Deliverables**:
- ✅ Tests use real database
- ✅ Tests use Rust pipeline
- ✅ TypeName injection verified
- ✅ All tests passing

---

## 🎨 Feature 4: Example Templates

### Problem Statement

**Current situation**: Blog example templates failing validation

**Skipped tests**: 10 example template tests

**Goal**: Fix example application database templates

---

### Implementation Plan (Simplified)

#### Phase 4.1: Fix Blog Example Templates (2-4 hours)

**Error**: "Template validation failed for blog_simple_template"

**Files to check**:
```
examples/blog_simple/db/
examples/blog_enterprise/db/
```

**Common issues**:
1. Missing JSONB columns
2. Template SQL syntax errors
3. Missing migrations
4. Incompatible with Rust pipeline

**Fix approach**:
1. Run example setup manually
2. Identify specific error
3. Fix template SQL
4. Verify example works

---

### Phase 4 Summary

**Timeline**: 1-2 days
**Tests Fixed**: 10
**User Impact**: LOW (developer experience)

---

## 📅 Overall Timeline

### Week 1: Nested Object Filtering
- Days 1-2: Design & implement recursive WHERE builder
- Day 3: Integration & testing
- Day 4: Documentation & examples
- **Deliverable**: 3 tests passing

### Week 2: Coordinate Datatype (Start)
- Days 5-7: Implement coordinate operators
- Day 8: PostgreSQL POINT integration
- **Deliverable**: Partial completion

### Week 3: Coordinate Datatype (Complete)
- Days 9-10: PostGIS support & distance filtering
- Day 11: Testing & debugging
- Day 12: Documentation & examples
- **Deliverable**: 10 tests passing

### Week 4: Polish & Cleanup
- Days 13-14: TypeName integration tests
- Day 15: Example templates
- Days 16-17: Final testing & documentation
- Day 18: Release v0.11.6

---

## 🎯 Success Criteria

### v0.11.6 Release Goals:

**Must Have**:
- ✅ Nested object filtering working
- ✅ All coordinate operators implemented
- ✅ Tests passing: 3,521+ (currently 3,508)
- ✅ Documentation complete

**Nice to Have**:
- ✅ TypeName integration tests fixed
- ✅ Example templates working
- ✅ PostGIS distance filtering
- ✅ Migration guides

**Metrics**:
- From: 3,508 passing, 44 skipped
- To: 3,521+ passing, 20-30 skipped
- Improvement: ~13 tests fixed

---

## 🚀 Quick Start for Agent

To implement nested object filtering (highest priority):

1. Read Phase 1 section of this document
2. Start with Phase 1.3 (TDD implementation)
3. Write failing tests first
4. Implement recursive WHERE builder
5. Test integration
6. Update documentation

**Estimated time**: 2-3 days for nested filtering alone

---

**This plan provides a clear roadmap to v0.11.6 with all major features completed!** 🎉
