# Phase R3: WhereInput Integration [GREEN]

**Status**: BLOCKED (waiting for R1, R2)
**Priority**: 🟢 HIGH
**Duration**: 1 day
**Risk**: MEDIUM

---

## Objective

Complete WhereInput integration by generating `_to_whereinput_dict()` method for all GraphQL WhereInput types. Enable full GraphQL query support with type-safe filters.

---

## Context

**Current State**:
- `normalize_whereinput()` function exists and expects `_to_whereinput_dict()` method
- WhereInput classes generated but missing this method
- GraphQL queries using WhereInput likely failing

**Problem**:
```python
# Generated WhereInput class (current):
@strawberry.input
class UserWhereInput:
    id: UUIDFilter | None = None
    name: StringFilter | None = None
    age: IntFilter | None = None
    OR: list["UserWhereInput"] | None = None
    AND: list["UserWhereInput"] | None = None
    NOT: "UserWhereInput" | None = None

# Missing method:
def _to_whereinput_dict(self) -> dict[str, Any]:
    """Convert to dict for normalization."""
    # NOT GENERATED
```

**Goal**: Generate this method for all WhereInput classes

---

## Implementation Steps

### Step 1: Analyze Current WhereInput Generation (1 hour)

**Location**: `src/fraiseql/sql/graphql_where_generator.py`

**Tasks**:
1. Understand current WhereInput generation logic
2. Find where to inject `_to_whereinput_dict()` method
3. Understand Filter class structure (UUIDFilter, StringFilter, etc.)

**Key Functions to Study**:
- `create_graphql_where_input()` - Main entry point
- `_create_where_input_class()` - Class creation
- Filter class definitions

**Expected Discovery**:
```python
def create_graphql_where_input(type_cls: type) -> type:
    """Generate WhereInput class for a type."""
    # ... field extraction ...
    # ... Filter class assignment ...

    # WHERE WE NEED TO ADD: Method injection
    @strawberry.input
    class WhereInput:
        # ... fields ...

        # NEED TO ADD THIS:
        def _to_whereinput_dict(self) -> dict[str, Any]:
            """Convert WhereInput to dict for WHERE normalization."""
            # ... implementation ...

    return WhereInput
```

---

### Step 2: Implement `_to_whereinput_dict()` Generation (3 hours)

**Location**: `src/fraiseql/sql/graphql_where_generator.py`

**Implementation Strategy**:

#### Option A: Add Method to Generated Class (RECOMMENDED)
```python
def create_graphql_where_input(type_cls: type) -> type:
    """Generate WhereInput class for a type."""

    # ... existing field generation logic ...

    # Create method dynamically
    def _to_whereinput_dict_impl(self) -> dict[str, Any]:
        """Convert WhereInput to dict format for WHERE normalization.

        Converts:
        - Filter objects (UUIDFilter, StringFilter, etc.) → operator dicts
        - Nested WhereInput objects → nested dicts
        - Logical operators (OR, AND, NOT) → list/dict structures
        """
        result = {}

        # Iterate through all fields
        for field_name, field_value in self.__dict__.items():
            if field_value is None:
                continue

            # Skip Strawberry internal fields
            if field_name.startswith("_"):
                continue

            # Handle logical operators
            if field_name == "OR":
                result["OR"] = [
                    item._to_whereinput_dict() if hasattr(item, "_to_whereinput_dict") else item
                    for item in field_value
                ]
                continue

            if field_name == "AND":
                result["AND"] = [
                    item._to_whereinput_dict() if hasattr(item, "_to_whereinput_dict") else item
                    for item in field_value
                ]
                continue

            if field_name == "NOT":
                result["NOT"] = (
                    field_value._to_whereinput_dict()
                    if hasattr(field_value, "_to_whereinput_dict")
                    else field_value
                )
                continue

            # Handle Filter objects (UUIDFilter, StringFilter, etc.)
            if hasattr(field_value, "__dict__"):
                # This is a Filter object, extract operators
                filter_dict = {}
                for op_name, op_value in field_value.__dict__.items():
                    if op_value is not None and not op_name.startswith("_"):
                        filter_dict[op_name] = op_value

                if filter_dict:
                    result[field_name] = filter_dict

            # Handle nested WhereInput objects
            elif hasattr(field_value, "_to_whereinput_dict"):
                result[field_name] = field_value._to_whereinput_dict()

            # Handle direct values (should not happen with type-safe WhereInput)
            else:
                result[field_name] = field_value

        return result

    # Generate WhereInput class
    @strawberry.input
    class WhereInput:
        # ... generated fields ...

        # Attach method to class
        _to_whereinput_dict = _to_whereinput_dict_impl

    # Set class name
    WhereInput.__name__ = f"{type_cls.__name__}WhereInput"

    return WhereInput
```

#### Option B: Add to Base Class (Alternative)
Create a base WhereInput class with the method:
```python
class BaseWhereInput:
    """Base class for all generated WhereInput classes."""

    def _to_whereinput_dict(self) -> dict[str, Any]:
        """Convert WhereInput to dict format."""
        # ... implementation ...

# Then in generation:
@strawberry.input
class WhereInput(BaseWhereInput):  # Inherit from base
    # ... fields ...
```

**Recommendation**: Option A (direct injection) - simpler, no inheritance complexity

---

### Step 3: Test Filter Object Conversion (2 hours)

**Create Test**: `tests/unit/sql/test_whereinput_to_dict.py`

```python
"""Tests for WhereInput._to_whereinput_dict() conversion."""

import uuid
import pytest
from fraiseql.sql import create_graphql_where_input, UUIDFilter, StringFilter, IntFilter


class User:
    """Test user class."""
    id: uuid.UUID
    name: str
    age: int


class TestWhereInputToDict:
    """Test WhereInput to dict conversion."""

    def test_simple_filter_conversion(self):
        """Test simple filter converts to operator dict."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(name=StringFilter(eq="John"))

        result = where_input._to_whereinput_dict()

        assert result == {"name": {"eq": "John"}}

    def test_multiple_operators_on_same_field(self):
        """Test multiple operators on same field."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(age=IntFilter(gte=18, lte=65))

        result = where_input._to_whereinput_dict()

        assert result == {"age": {"gte": 18, "lte": 65}}

    def test_multiple_fields(self):
        """Test multiple fields with filters."""
        UserWhereInput = create_graphql_where_input(User)

        user_id = uuid.uuid4()
        where_input = UserWhereInput(
            id=UUIDFilter(eq=user_id),
            name=StringFilter(icontains="john")
        )

        result = where_input._to_whereinput_dict()

        assert result == {
            "id": {"eq": user_id},
            "name": {"icontains": "john"}
        }

    def test_or_operator_conversion(self):
        """Test OR operator converts to list."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            OR=[
                UserWhereInput(name=StringFilter(eq="John")),
                UserWhereInput(name=StringFilter(eq="Jane"))
            ]
        )

        result = where_input._to_whereinput_dict()

        assert result == {
            "OR": [
                {"name": {"eq": "John"}},
                {"name": {"eq": "Jane"}}
            ]
        }

    def test_not_operator_conversion(self):
        """Test NOT operator converts correctly."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            name=StringFilter(eq="Active"),
            NOT=UserWhereInput(age=IntFilter(lt=18))
        )

        result = where_input._to_whereinput_dict()

        assert result == {
            "name": {"eq": "Active"},
            "NOT": {"age": {"lt": 18}}
        }

    def test_nested_whereinput_conversion(self):
        """Test nested WhereInput objects."""
        # This requires related objects, tested in integration

    def test_none_values_ignored(self):
        """Test None values are not included in result."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(
            name=StringFilter(eq="John"),
            age=None  # Should be ignored
        )

        result = where_input._to_whereinput_dict()

        assert result == {"name": {"eq": "John"}}
        assert "age" not in result

    def test_empty_filter_ignored(self):
        """Test filter with no operators set is ignored."""
        UserWhereInput = create_graphql_where_input(User)

        where_input = UserWhereInput(name=StringFilter())  # No operators

        result = where_input._to_whereinput_dict()

        # Empty filter should not appear in result
        assert result == {} or "name" not in result
```

**Verification**:
```bash
uv run pytest tests/unit/sql/test_whereinput_to_dict.py -v
```

---

### Step 4: Integration with Normalization (1 hour)

**Test**: Verify full pipeline works

**Location**: `tests/unit/test_where_normalization.py`

**Add Tests**:
```python
def test_normalize_whereinput_with_generated_method(self):
    """Test normalizing WhereInput with _to_whereinput_dict()."""
    from fraiseql.sql import create_graphql_where_input, StringFilter
    from tests.regression.test_nested_filter_id_field import Allocation

    AllocationWhereInput = create_graphql_where_input(Allocation)

    where_input = AllocationWhereInput(status=StringFilter(eq="active"))

    repo = FraiseQLRepository(None)
    clause = repo._normalize_where(
        where_input,
        "tv_allocation",
        {"status", "machine_id", "data"}
    )

    assert isinstance(clause, WhereClause)
    assert len(clause.conditions) == 1
    assert clause.conditions[0].field_path == ["status"]
    assert clause.conditions[0].operator == "eq"
    assert clause.conditions[0].value == "active"

def test_whereinput_equals_dict_after_normalization(self):
    """Test WhereInput and dict produce identical WhereClause."""
    from fraiseql.sql import create_graphql_where_input, StringFilter
    from tests.regression.test_nested_filter_id_field import Allocation

    AllocationWhereInput = create_graphql_where_input(Allocation)

    # WhereInput version
    where_input = AllocationWhereInput(status=StringFilter(eq="active"))

    # Dict version
    where_dict = {"status": {"eq": "active"}}

    repo = FraiseQLRepository(None)

    clause_from_input = repo._normalize_where(
        where_input, "tv_allocation", {"status"}
    )
    clause_from_dict = repo._normalize_where(
        where_dict, "tv_allocation", {"status"}
    )

    # Should be structurally identical
    assert len(clause_from_input.conditions) == len(clause_from_dict.conditions)
    assert clause_from_input.conditions[0].operator == clause_from_dict.conditions[0].operator
    assert clause_from_input.conditions[0].value == clause_from_dict.conditions[0].value

    # Should generate identical SQL
    sql_input, params_input = clause_from_input.to_sql()
    sql_dict, params_dict = clause_from_dict.to_sql()

    assert sql_input.as_string(None) == sql_dict.as_string(None)
    assert params_input == params_dict
```

---

### Step 5: GraphQL Query Integration Test (2 hours)

**Test**: End-to-end GraphQL query with WhereInput

**Location**: `tests/integration/graphql/test_whereinput_queries.py` (new file)

```python
"""Integration tests for GraphQL queries with WhereInput."""

import pytest
import strawberry
from fraiseql.sql import create_graphql_where_input, StringFilter, IntFilter


@pytest.mark.asyncio
async def test_graphql_query_with_whereinput(class_db_pool):
    """Test GraphQL query using WhereInput filter."""
    from fraiseql.db import FraiseQLRepository, register_type_for_view

    # Set up test data
    async with class_db_pool.connection() as conn, conn.cursor() as cursor:
        await cursor.execute("""
            CREATE TABLE IF NOT EXISTS test_users (
                id UUID PRIMARY KEY,
                name TEXT,
                age INT
            )
        """)

        await cursor.execute("""
            INSERT INTO test_users (id, name, age)
            VALUES
                (gen_random_uuid(), 'Alice', 25),
                (gen_random_uuid(), 'Bob', 30),
                (gen_random_uuid(), 'Charlie', 35)
        """)
        await conn.commit()

    # Define GraphQL type
    @strawberry.type
    class User:
        id: str
        name: str
        age: int

    UserWhereInput = create_graphql_where_input(User)

    # Define query
    @strawberry.type
    class Query:
        @strawberry.field
        async def users(self, where: UserWhereInput | None = None) -> list[User]:
            repo = FraiseQLRepository(class_db_pool)
            results = await repo.find("test_users", where=where)
            return [User(**r) for r in results]

    schema = strawberry.Schema(query=Query)

    # Execute query with WhereInput
    query = """
        query {
            users(where: {age: {gte: 30}}) {
                name
                age
            }
        }
    """

    result = await schema.execute(query)

    assert result.errors is None
    assert len(result.data["users"]) == 2
    assert all(u["age"] >= 30 for u in result.data["users"])

    # Cleanup
    async with class_db_pool.connection() as conn, conn.cursor() as cursor:
        await cursor.execute("DROP TABLE test_users")
        await conn.commit()
```

**Verification**:
```bash
uv run pytest tests/integration/graphql/test_whereinput_queries.py -v
```

---

## Verification Commands

### After Each Step
```bash
# Step 2: Method generation
uv run pytest tests/unit/sql/test_whereinput_to_dict.py -v

# Step 4: Normalization integration
uv run pytest tests/unit/test_where_normalization.py -k "whereinput" -v

# Step 5: GraphQL integration
uv run pytest tests/integration/graphql/test_whereinput_queries.py -v
```

### Full Verification
```bash
# All WhereInput tests
uv run pytest tests/ -k "whereinput" -v

# GraphQL tests (should now pass)
uv run pytest tests/integration/graphql/test_graphql_query_execution_complete.py -v

# Full suite
uv run pytest tests/ -v
```

---

## Acceptance Criteria

### Implementation ✅
- [ ] `_to_whereinput_dict()` method generated for all WhereInput classes
- [ ] Method handles Filter objects correctly
- [ ] Method handles OR, AND, NOT operators
- [ ] Method handles nested WhereInput objects
- [ ] Method ignores None values

### Tests ✅
- [ ] Unit tests for `_to_whereinput_dict()` passing (10+ tests)
- [ ] Integration tests with normalization passing
- [ ] GraphQL end-to-end tests passing
- [ ] Equivalence tests (dict == WhereInput) passing

### Integration ✅
- [ ] `normalize_whereinput()` works with generated method
- [ ] GraphQL queries with WhereInput work
- [ ] Nested filters work (FK and JSONB)
- [ ] All logical operators work

---

## DO NOT

❌ **DO NOT** modify Filter classes (UUIDFilter, StringFilter, etc.)
❌ **DO NOT** change WhereInput field structure
❌ **DO NOT** break existing GraphQL schema
❌ **DO NOT** skip conversion tests

---

## Rollback Plan

**If generation too complex**:
- Implement manual `_to_whereinput_dict()` in base class
- Document requirement for users to call method
- Add helper function for conversion

---

## Time Estimates

| Step | Optimistic | Realistic | Pessimistic |
|------|-----------|-----------|-------------|
| 1. Analyze generation | 0.5h | 1h | 2h |
| 2. Implement method | 2h | 3h | 5h |
| 3. Test conversion | 1h | 2h | 3h |
| 4. Integration test | 0.5h | 1h | 2h |
| 5. GraphQL test | 1h | 2h | 3h |
| **TOTAL** | **5h** | **9h** | **15h** |

**Realistic Timeline**: 1 day (8h = 9h with breaks)

---

## Progress Tracking

- [ ] Step 1: Analysis complete
- [ ] Step 2: Method generation working
- [ ] Step 3: Conversion tests passing
- [ ] Step 4: Normalization integration passing
- [ ] Step 5: GraphQL queries working
- [ ] All acceptance criteria met

---

**Phase Status**: BLOCKED (waiting for R1, R2)
**Previous Phase**: [phase-r2-implement-missing-operators.md](phase-r2-implement-missing-operators.md)
**Next Phase**: [phase-r4-optimization-cleanup.md](phase-r4-optimization-cleanup.md)
