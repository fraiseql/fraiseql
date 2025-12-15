# Phase 1: RED - Write Failing Tests

**Status**: Ready for Implementation
**Effort**: 2 hours
**Type**: TDD - Test First

---

## Objective

Write comprehensive tests that **define the expected behavior** of custom scalar WHERE filtering. These tests will FAIL, clearly showing what's missing.

---

## Context

Currently:
- ✅ Custom scalars work as field types
- ✅ Custom scalars work in database roundtrip
- ❌ Custom scalars don't work in WHERE clauses

**Error when trying**: `Variable '$filterValue' of type 'CIDR!' used in position expecting type 'String'`

**Root cause**: Filter generator creates `StringFilter` instead of `CIDRFilter`

---

## Tests to Write

### Test 1: Filter Type Generation (Unit Test)
**File**: `tests/unit/sql/test_custom_scalar_where_filters.py` (NEW)

**Purpose**: Verify that custom scalar filter types are generated correctly

```python
"""Unit tests for custom scalar WHERE filter generation."""
import pytest
from graphql import GraphQLScalarType
from fraiseql.types.scalars import CIDRScalar, EmailScalar, ColorScalar
from fraiseql.sql.graphql_where_generator import create_graphql_where_input
from fraiseql import fraise_type


def test_custom_scalar_filter_is_generated():
    """Filter generator should create ScalarNameFilter for custom scalars."""
    @fraise_type
    class TestType:
        id: int
        ip_network: CIDRScalar

    where_input = create_graphql_where_input(TestType)

    # Should generate TestTypeWhereInput
    assert where_input is not None
    assert where_input.name == "TestTypeWhereInput"

    # Should have ipNetwork field (camelCase)
    assert "ipNetwork" in where_input.fields

    # Field should be a CIDRFilter, not StringFilter
    ip_filter_type = where_input.fields["ipNetwork"].type
    assert ip_filter_type.name == "CIDRFilter"


def test_custom_scalar_filter_has_standard_operators():
    """Custom scalar filters should have eq, ne, in, notIn, etc."""
    @fraise_type
    class TestType:
        email: EmailScalar

    where_input = create_graphql_where_input(TestType)
    email_filter_type = where_input.fields["email"].type

    # Should have standard comparison operators
    assert "eq" in email_filter_type.fields
    assert "ne" in email_filter_type.fields
    assert "in" in email_filter_type.fields
    assert "notIn" in email_filter_type.fields

    # Should have string pattern operators
    assert "contains" in email_filter_type.fields
    assert "startsWith" in email_filter_type.fields
    assert "endsWith" in email_filter_type.fields


def test_custom_scalar_filter_uses_scalar_type():
    """Filter operators should use the scalar type, not String."""
    @fraise_type
    class TestType:
        color: ColorScalar

    where_input = create_graphql_where_input(TestType)
    color_filter_type = where_input.fields["color"].type

    # eq operator should accept ColorScalar, not String
    eq_field = color_filter_type.fields["eq"]
    assert isinstance(eq_field.type, GraphQLScalarType)
    assert eq_field.type.name == "Color"  # The scalar's GraphQL name


def test_filter_type_is_cached():
    """Same scalar type should reuse the same filter type instance."""
    @fraise_type
    class TypeA:
        email1: EmailScalar
        email2: EmailScalar

    @fraise_type
    class TypeB:
        email: EmailScalar

    where_a = create_graphql_where_input(TypeA)
    where_b = create_graphql_where_input(TypeB)

    # Both should use the SAME EmailFilter instance (cached)
    email_filter_a1 = where_a.fields["email1"].type
    email_filter_a2 = where_a.fields["email2"].type
    email_filter_b = where_b.fields["email"].type

    assert email_filter_a1 is email_filter_a2
    assert email_filter_a1 is email_filter_b


def test_nullable_custom_scalar_filter():
    """Nullable scalar fields should still get proper filters."""
    @fraise_type
    class TestType:
        optional_email: EmailScalar | None

    where_input = create_graphql_where_input(TestType)

    # Should still have the filter
    assert "optionalEmail" in where_input.fields

    # Filter type should still be EmailFilter
    filter_type = where_input.fields["optionalEmail"].type
    assert filter_type.name == "EmailFilter"
```

**Expected Result**: All tests FAIL with clear messages like:
- `AssertionError: expected 'CIDRFilter', got 'StringFilter'`
- `KeyError: 'ipNetwork' not in where_input.fields`

---

### Test 2: GraphQL Query Integration (Integration Test)
**File**: `tests/integration/meta/test_all_scalars.py` (MODIFY)

**Purpose**: Un-skip the 6 WHERE clause tests, fix them to work properly

**Current Status**: Tests are skipped, implementation is half-done from investigation

**Changes Needed**:

1. **Remove skip decorator**:
```python
# REMOVE THIS:
@pytest.mark.skip(
    reason="WHERE filter generation does not support custom scalar types..."
)

# Keep this:
@pytest.mark.parametrize(...)
async def test_scalar_in_where_clause(...):
```

2. **Simplify test implementation** (current version is overly complex):

```python
async def test_scalar_in_where_clause(scalar_name, scalar_class, meta_test_pool):
    """Every scalar should work in WHERE clauses with database roundtrip."""
    from graphql import graphql
    from fraiseql import fraise_type, query
    from fraiseql.gql.builders import SchemaRegistry
    from fraiseql.sql.graphql_where_generator import create_graphql_where_input

    # Create test table
    table_name = f"test_{scalar_name.lower()}_table"
    column_name = f"{scalar_name.lower()}_col"

    async with meta_test_pool.connection() as conn:
        await conn.execute(
            sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
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
                sql.SQL(get_postgres_type_for_scalar(scalar_class)),
            )
        )

        # Insert test data
        test_value = get_test_value_for_scalar(scalar_class)
        if isinstance(test_value, dict):
            from psycopg.types.json import Jsonb
            adapted_value = Jsonb(test_value)
        else:
            adapted_value = test_value

        await conn.execute(
            sql.SQL("INSERT INTO {} ({}) VALUES (%s)").format(
                sql.Identifier(table_name),
                sql.Identifier(column_name)
            ),
            [adapted_value],
        )
        await conn.commit()

    try:
        # Create schema
        registry = SchemaRegistry.get_instance()
        registry.clear()

        # Create type with scalar field
        @fraise_type(sql_source=table_name)
        class TestType:
            id: int

        # Add scalar field annotation dynamically
        TestType.__annotations__["test_field"] = scalar_class

        # Create WhereInput
        TestTypeWhereInput = create_graphql_where_input(TestType)

        # Register type
        registry.register_type(TestType)

        # Create query with WHERE parameter
        @query
        async def get_test_data(
            info,
            where: TestTypeWhereInput | None = None
        ) -> list[TestType]:
            """Query with WHERE support."""
            from fraiseql.db import FraiseQLRepository
            db = info.context.get("db") or info.context.get("pool")
            repo = FraiseQLRepository(db)
            result = await repo.find(table_name, where=where)
            return result.get(table_name, [])

        registry.register_query(get_test_data)

        # Build schema
        schema = registry.build_schema()

        # Verify WhereInput was created correctly
        where_input_type = schema.get_type("TestTypeWhereInput")
        assert where_input_type is not None

        # Verify testField filter exists
        assert "testField" in where_input_type.fields

        # Execute GraphQL query with WHERE filter
        graphql_scalar_name = scalar_class.name
        test_value = get_test_value_for_scalar(scalar_class)

        query_str = f"""
        query GetTestData($filterValue: {graphql_scalar_name}!) {{
            getTestData(where: {{testField: {{eq: $filterValue}}}}) {{
                id
                testField
            }}
        }}
        """

        context = {"db": meta_test_pool}
        variables = {"filterValue": test_value}

        result = await graphql(
            schema,
            query_str,
            variable_values=variables,
            context_value=context
        )

        # Should work without errors
        assert not result.errors, (
            f"Scalar {scalar_name} failed in WHERE clause: {result.errors}"
        )

        # Should return the inserted row
        assert result.data is not None
        assert "getTestData" in result.data
        results = result.data["getTestData"]
        assert len(results) == 1
        assert results[0]["id"] == 1

    finally:
        # Cleanup
        async with meta_test_pool.connection() as conn:
            await conn.execute(
                sql.SQL("DROP TABLE IF EXISTS {}").format(sql.Identifier(table_name))
            )
            await conn.commit()
```

**Expected Result**: All 6 tests FAIL with:
```
AssertionError: Scalar CIDRScalar failed in WHERE clause: [GraphQLError("Variable '$filterValue' of type 'CIDR!' used in position expecting type 'String'.")]
```

---

### Test 3: Edge Cases (Unit Tests)
**File**: `tests/unit/sql/test_custom_scalar_where_filters.py`

**Purpose**: Test edge cases and error handling

```python
def test_list_of_custom_scalars():
    """List fields with custom scalars should work."""
    @fraise_type
    class TestType:
        tags: list[ColorScalar]

    where_input = create_graphql_where_input(TestType)

    # Should have tags filter
    assert "tags" in where_input.fields

    # Filter should handle list operations
    # (Exact behavior TBD - document current behavior)


def test_mixed_field_types():
    """Type with both custom scalars and regular fields."""
    @fraise_type
    class TestType:
        name: str                    # Regular string
        email: EmailScalar           # Custom scalar
        count: int                   # Regular int
        ip_address: CIDRScalar       # Another custom scalar

    where_input = create_graphql_where_input(TestType)

    # All fields should have appropriate filters
    assert where_input.fields["name"].type.name == "StringFilter"
    assert where_input.fields["email"].type.name == "EmailFilter"
    assert where_input.fields["count"].type.name == "IntFilter"
    assert where_input.fields["ipAddress"].type.name == "CIDRFilter"


def test_built_in_scalar_types_unchanged():
    """Built-in scalars (UUID, DateTime) should still work."""
    from datetime import datetime
    import uuid

    @fraise_type
    class TestType:
        id: uuid.UUID
        created_at: datetime
        name: str

    where_input = create_graphql_where_input(TestType)

    # Should use existing filter types (not break existing behavior)
    assert where_input.fields["id"].type.name in ["UUIDFilter", "IDFilter"]
    assert where_input.fields["createdAt"].type.name in ["DateTimeFilter"]
    assert where_input.fields["name"].type.name == "StringFilter"
```

**Expected Result**: Most tests FAIL, showing what behavior we need to implement

---

## Implementation Steps

### Step 1: Create Unit Test File
**Action**: Create `tests/unit/sql/test_custom_scalar_where_filters.py`

**Content**: All unit tests from above

**Verification**:
```bash
uv run pytest tests/unit/sql/test_custom_scalar_where_filters.py -v
```

**Expected**: All tests FAIL with clear error messages

---

### Step 2: Un-skip Integration Tests
**Action**: Modify `tests/integration/meta/test_all_scalars.py`

**Changes**:
1. Remove `@pytest.mark.skip(...)` decorator from `test_scalar_in_where_clause`
2. Simplify test implementation (use code above)
3. Ensure test creates database table, inserts data, queries with WHERE

**Verification**:
```bash
uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -v
```

**Expected**: All 6 tests FAIL with:
```
Variable '$filterValue' of type 'CIDR!' used in position expecting type 'String'
```

---

### Step 3: Document Current Behavior
**Action**: Add docstring to test file explaining what we're testing

```python
"""
Custom Scalar WHERE Filter Tests

These tests verify that custom scalar types can be used in WHERE clause
filtering. The filter generator should create scalar-specific filter types
(e.g., CIDRFilter, EmailFilter) with standard operators (eq, ne, in, etc.)
that accept the scalar type instead of defaulting to String.

Expected behavior:
1. Filter type generation: CIDRFilter for CIDRScalar fields
2. Operator support: eq, ne, in, notIn, contains, startsWith, endsWith
3. Type safety: Operators accept scalar type, not String
4. Caching: Same filter type reused across fields
5. GraphQL query: WHERE clause works with custom scalar variables
"""
```

---

## Acceptance Criteria

- [ ] Unit test file created with 8+ tests
- [ ] All unit tests FAIL with clear, actionable error messages
- [ ] Integration tests un-skipped (6 tests)
- [ ] All integration tests FAIL with expected error message
- [ ] Test failures clearly point to `create_graphql_where_input()` as the fix location
- [ ] No tests pass that shouldn't (no false positives)
- [ ] Tests document the expected behavior clearly

---

## Expected Test Output

```bash
$ uv run pytest tests/unit/sql/test_custom_scalar_where_filters.py -v

FAILED test_custom_scalar_filter_is_generated - AssertionError: expected 'CIDRFilter', got 'StringFilter'
FAILED test_custom_scalar_filter_has_standard_operators - KeyError: 'CIDRFilter'
FAILED test_custom_scalar_filter_uses_scalar_type - AssertionError: expected GraphQLScalarType 'CIDR', got String
FAILED test_filter_type_is_cached - AssertionError: filter instances are different
FAILED test_nullable_custom_scalar_filter - AssertionError: expected 'EmailFilter', got 'StringFilter'
FAILED test_list_of_custom_scalars - KeyError: 'tags' filter not found
FAILED test_mixed_field_types - AssertionError: expected 'EmailFilter', got 'StringFilter'
PASSED test_built_in_scalar_types_unchanged

8 tests: 1 passed, 7 failed
```

```bash
$ uv run pytest tests/integration/meta/test_all_scalars.py::test_scalar_in_where_clause -v

FAILED test_scalar_in_where_clause[CIDRScalar] - Variable of type 'CIDR!' used in position expecting 'String'
FAILED test_scalar_in_where_clause[CUSIPScalar] - Variable of type 'CUSIP!' used in position expecting 'String'
FAILED test_scalar_in_where_clause[DateScalar] - Variable of type 'Date!' used in position expecting 'String'
FAILED test_scalar_in_where_clause[IpAddressScalar] - Variable of type 'IPAddress!' used in position expecting 'String'
FAILED test_scalar_in_where_clause[JSONScalar] - Variable of type 'JSON!' used in position expecting 'String'
FAILED test_scalar_in_where_clause[UUIDScalar] - Variable of type 'UUID!' used in position expecting 'String'

6 tests: 0 passed, 6 failed
```

---

## Commit Message

```
test(where): add tests for custom scalar WHERE filters [RED]

Add comprehensive test coverage for custom scalar WHERE filtering:
- Unit tests for filter type generation
- Integration tests for GraphQL queries with WHERE
- Edge case tests for nullable and list fields

All tests currently FAIL, demonstrating the gap in filter generation.
The filter generator creates StringFilter instead of scalar-specific
filters (CIDRFilter, EmailFilter, etc.).

Expected errors:
- "Variable of type 'CIDR!' used in position expecting type 'String'"
- Filter type assertions fail (got StringFilter, expected CIDRFilter)

Related: #XXX (create issue for this feature)
```

---

## DO NOT

- ❌ Write any implementation code yet
- ❌ Make tests pass artificially
- ❌ Skip difficult edge cases
- ❌ Write tests that are unclear about expected behavior

## DO

- ✅ Write tests that clearly specify expected behavior
- ✅ Ensure all tests FAIL for the right reasons
- ✅ Document why each test exists
- ✅ Make error messages actionable
- ✅ Think through edge cases

---

**Next Phase**: Phase 2 - GREEN (Make all these tests pass)
