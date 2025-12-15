# FraiseQL Integration Test Coverage Gaps - Implementation Plan

## Executive Summary

**Objective**: Address critical integration test gaps identified in `/tmp/fraiseql-integration-test-gaps-analysis.md` where 27 out of 57 test areas (47%) have unit tests but no integration tests.

**Risk**: Components work in isolation but fail when integrated, creating production bugs (similar to recent network operators issue where operators were implemented but not registered in `ALL_OPERATORS`).

**Approach**: Build meta-integration tests first to catch "works in isolation, fails in production" bugs, then systematically fill coverage gaps using TDD workflow.

**Success Metrics**:
- All 27 gap areas have integration tests
- Meta-tests prevent future registration bugs
- Test suite completes in <10 minutes
- <5% test failure rate in CI

---

## Phase 0: Discovery & Test Infrastructure Setup

### Objective
Understand FraiseQL's existing test infrastructure and verify what utilities are available before writing integration tests.

### Context
Before implementing integration tests, we need to:
1. Document existing test utilities (`GraphQLTestClient`, fixtures)
2. Understand FraiseQL's operator/scalar registration patterns
3. Identify gaps in test infrastructure
4. Map out available pytest fixtures

This prevents writing tests based on assumptions about APIs that don't exist.

### Files to Check
- ✅ `tests/utils/graphql_test_client.py` - GraphQL test client (EXISTS)
- ✅ `tests/conftest.py` - Pytest fixtures (EXISTS)
- ✅ `src/fraiseql/where_clause.py` - Operator registry (`ALL_OPERATORS` EXISTS)
- ✅ `src/fraiseql/types/scalars/` - Custom scalar implementations (80+ files)

### Implementation Steps

#### Step 1: Document Existing Test Utilities [RED]

**Task**: Read and document `GraphQLTestClient` API.

**Expected findings**:
```python
# tests/utils/graphql_test_client.py provides:

class GraphQLTestClient:
    def __init__(self, schema: GraphQLSchema)

    async def query(
        self,
        query: str,
        result_type: Type[T],
        variables: dict[str, Any] | None = None,
        operation_name: str | None = None,
        context: dict[str, Any] | None = None,
    ) -> TypedGraphQLResponse[T]

# Returns TypedGraphQLResponse with:
# - data: T | None
# - errors: list[dict[str, Any]] | None
# - ok: bool (property)
```

**Action**: Create `docs/testing/existing-test-infrastructure.md` documenting what's available.

#### Step 2: Document Available Pytest Fixtures [GREEN]

**Task**: Extract fixture list from `tests/conftest.py`.

**Expected findings**:
```python
# Database fixtures (from tests/fixtures/database/database_conftest.py):
# - postgres_container: Docker PostgreSQL instance
# - postgres_url: Connection string
# - db_connection: Database connection
# - class_db_pool: Class-scoped connection pool
# - test_schema: GraphQL schema for testing

# Example fixtures (from tests/fixtures/examples/conftest_examples.py):
# - blog_simple_app: Simple blog app
# - blog_simple_client: GraphQL client
# - sample_user_data, sample_post_data, etc.
```

**Action**: Document fixtures in `docs/testing/existing-test-infrastructure.md`.

#### Step 3: Map Operator Registry Pattern [REFACTOR]

**Task**: Understand how operators are registered in `ALL_OPERATORS`.

**Expected findings**:
```python
# src/fraiseql/where_clause.py:

# Individual operator dictionaries
COMPARISON_OPERATORS = {"eq": "=", "neq": "!=", ...}
CONTAINMENT_OPERATORS = {"in": "IN", "nin": "NOT IN"}
STRING_OPERATORS = {"contains": "LIKE", "icontains": "ILIKE", ...}
NULL_OPERATORS = {"isnull": "IS NULL"}
VECTOR_OPERATORS = {"cosine_distance": "<=>", ...}
ARRAY_OPERATORS = {"array_contains": "@>", ...}
NETWORK_OPERATORS = {"isIPv4": "family({}) = 4", ...}
MACADDR_OPERATORS = {"notin": "NOT IN"}
DATERANGE_OPERATORS = {"contains_date": "@>", ...}
LTREE_OPERATORS = {"ancestor_of": "@>", ...}
COORDINATE_OPERATORS = {"within_radius": "distance", ...}

# Master registry (line 203-217)
ALL_OPERATORS = {
    **COMPARISON_OPERATORS,
    **CONTAINMENT_OPERATORS,
    **STRING_OPERATORS,
    **NULL_OPERATORS,
    **VECTOR_OPERATORS,
    **_ARRAY_OPERATORS_FOR_ALL,
    **FULLTEXT_OPERATORS,
    **NETWORK_OPERATORS,
    **MACADDR_OPERATORS,
    **DATERANGE_OPERATORS,
    **LTREE_OPERATORS,
    **COORDINATE_OPERATORS,
}
```

**Key insight**: Operators must be added to category dict AND included in `ALL_OPERATORS` spread. Missing from either causes bugs.

**Action**: Document in `docs/testing/existing-test-infrastructure.md`.

#### Step 4: Map Scalar Implementation Pattern [QA]

**Task**: Understand how custom scalars are implemented.

**Expected findings**:
```python
# src/fraiseql/types/scalars/ contains 80+ scalar implementations
# Examples:
# - email_address.py
# - uuid.py
# - datetime.py
# - mac_address.py
# - coordinates.py
# etc.

# Pattern: Each scalar is a separate file with class definition
# No central registry like ALL_OPERATORS (yet)
```

**Action**: Document scalar pattern. Note that we may need to CREATE a `get_all_custom_scalars()` function for meta-testing.

### Verification Commands

```bash
# Verify GraphQLTestClient exists
cat tests/utils/graphql_test_client.py | grep "class GraphQLTestClient"

# List all fixtures
grep -r "@pytest.fixture" tests/conftest.py tests/fixtures/

# Verify ALL_OPERATORS exists
grep "ALL_OPERATORS = {" src/fraiseql/where_clause.py -A 15

# Count custom scalars
find src/fraiseql/types/scalars/ -name "*.py" | wc -l
```

**Expected output**:
```
✓ GraphQLTestClient found
✓ 20+ fixtures available
✓ ALL_OPERATORS found with 12 categories
✓ 80+ custom scalar files
```

### Acceptance Criteria

- [ ] `docs/testing/existing-test-infrastructure.md` created with GraphQLTestClient API
- [ ] All available pytest fixtures documented
- [ ] Operator registration pattern documented with line numbers
- [ ] Scalar implementation pattern documented
- [ ] Identified what test utilities need to be created (e.g., `get_all_custom_scalars()`)

### DO NOT

- ❌ Assume APIs exist without verifying
- ❌ Write integration tests before understanding fixtures
- ❌ Skip documentation (junior engineer needs this)

---

## Phase 1: Meta-Integration Tests (Prevent Future Regressions)

### Objective
Create meta-integration tests that automatically verify ALL components work in complete pipelines, preventing the "works in isolation, fails in production" pattern.

### Context

**Current State**:
- Unit tests verify individual operators work in isolation
- No tests verify operators are registered in `ALL_OPERATORS`
- Recent bug: network operators implemented but not registered → failed in production

**Problem**:
- Developers add operators to category dict but forget `ALL_OPERATORS`
- No automated check catches missing registrations
- Bugs only found when users try to use operators in WHERE clauses

**Solution**:
- Meta-test that iterates through ALL_OPERATORS and tests each in real GraphQL query
- Similar to `test_operator_registration.py` that caught the network bug
- If operator missing from `ALL_OPERATORS`, test fails immediately

### Files to Create

- `tests/integration/test_all_operators_registration.py`
- `tests/integration/test_all_scalars_integration.py` (requires helper first)
- `src/fraiseql/testing/scalar_registry.py` (helper to enumerate scalars)

### Implementation Steps

#### Step 1.1: Create Operator Registration Meta-Test [RED]

**Task**: Write test that verifies every operator in `ALL_OPERATORS` works in real GraphQL queries.

**File**: `tests/integration/test_all_operators_registration.py`

```python
"""Meta-integration test for operator registration.

This test prevents the "implemented but not registered" bug pattern.
If an operator exists in a category dict but is missing from ALL_OPERATORS,
this test will fail.
"""

import pytest
from fraiseql.where_clause import ALL_OPERATORS


class TestOperatorRegistration:
    """Test ALL operators are registered and work in GraphQL queries."""

    def test_all_operators_are_registered(self):
        """Verify ALL_OPERATORS contains all operator categories.

        This is a sanity check that prevents the network operators bug
        where operators were implemented but not added to ALL_OPERATORS.
        """
        # Expected operator categories
        expected_categories = [
            "eq", "neq", "gt", "gte", "lt", "lte",  # COMPARISON
            "in", "nin",  # CONTAINMENT
            "contains", "icontains", "startswith", "istartswith",  # STRING (subset)
            "isnull",  # NULL
            "cosine_distance", "l2_distance",  # VECTOR (subset)
            "array_contains", "array_eq",  # ARRAY (subset)
            "isIPv4", "isIPv6", "inSubnet",  # NETWORK (subset)
        ]

        for op in expected_categories:
            assert op in ALL_OPERATORS, (
                f"Operator '{op}' missing from ALL_OPERATORS. "
                f"Did you add it to category dict but forget ALL_OPERATORS?"
            )

    @pytest.mark.asyncio
    async def test_all_operators_work_in_where_clauses(
        self,
        db_connection,
        test_schema
    ):
        """Meta-test: Every operator in ALL_OPERATORS works in GraphQL WHERE clause.

        This test:
        1. Creates a test table with various column types
        2. For each operator in ALL_OPERATORS:
           - Constructs a GraphQL query with that operator
           - Executes the query
           - Verifies no errors occurred

        If an operator is in ALL_OPERATORS but doesn't work, this test catches it.
        """
        from tests.utils.graphql_test_client import GraphQLTestClient

        client = GraphQLTestClient(test_schema)

        # Create test table with various types
        await db_connection.execute("""
            CREATE TABLE IF NOT EXISTS operator_test (
                id SERIAL PRIMARY KEY,
                text_field TEXT,
                int_field INTEGER,
                bool_field BOOLEAN,
                array_field TEXT[],
                jsonb_field JSONB
            )
        """)

        # Insert test data
        await db_connection.execute("""
            INSERT INTO operator_test (text_field, int_field, bool_field, array_field, jsonb_field)
            VALUES ('test', 42, true, ARRAY['a', 'b'], '{"key": "value"}')
        """)

        # Test subset of operators (comprehensive test would be too slow)
        # Focus on operators that commonly have registration bugs
        critical_operators = {
            "eq": ("int_field", 42),
            "neq": ("int_field", 0),
            "gt": ("int_field", 0),
            "in": ("int_field", [42, 43]),
            "contains": ("text_field", "test"),  # String LIKE
            "isnull": ("text_field", False),
        }

        for operator, (field, value) in critical_operators.items():
            query = f"""
            query TestOperator($value: Any!) {{
                operatorTests(where: {{{field}: {{{operator}: $value}}}}) {{
                    id
                    textField
                }}
            }}
            """

            response = await client.query(
                query=query,
                result_type=list[dict],
                variables={"value": value}
            )

            assert response.ok, (
                f"Operator '{operator}' failed in WHERE clause: {response.errors}"
            )
```

**Verification**:
```bash
pytest tests/integration/test_all_operators_registration.py -v

# Expected: FAILED - table doesn't exist yet, or schema not configured
# This is correct for RED phase
```

#### Step 1.2: Make Test Pass [GREEN]

**Task**: Set up test schema and make operator registration test pass.

**Action**: Ensure `test_schema` fixture includes `operator_test` table in schema definition.

**Verification**:
```bash
pytest tests/integration/test_all_operators_registration.py -v

# Expected: PASSED - all critical operators work
```

#### Step 1.3: Refactor Test for Maintainability [REFACTOR]

**Task**: Extract test data setup into fixture, improve error messages.

**Changes**:
- Move table creation to conftest fixture
- Add better error messages showing which operator failed
- Group operators by category for clearer output

**Verification**:
```bash
pytest tests/integration/test_all_operators_registration.py -v

# Expected: PASSED (same behavior, cleaner code)
```

#### Step 1.4: Add Comprehensive Operator Coverage [QA]

**Task**: Expand test to cover ALL operators in ALL_OPERATORS, not just critical subset.

**Changes**:
- Test all comparison operators (eq, neq, gt, gte, lt, lte)
- Test all string operators (contains, icontains, startswith, etc.)
- Test array operators
- Test network operators
- Add edge cases (null values, empty arrays, etc.)

**Verification**:
```bash
pytest tests/integration/test_all_operators_registration.py -v --tb=short

# Expected: PASSED with output showing all operators tested:
# ✓ Tested 60+ operators
# ✓ All operators in ALL_OPERATORS work in WHERE clauses
```

### Verification Commands

```bash
# Run meta-integration test
pytest tests/integration/test_all_operators_registration.py -v

# Run with coverage to see which operators are tested
pytest tests/integration/test_all_operators_registration.py --cov=fraiseql.where_clause --cov-report=term-missing

# Test that removing an operator from ALL_OPERATORS causes failure
# (Manual verification: comment out "isIPv4" from ALL_OPERATORS, run test, expect FAIL)
```

### Acceptance Criteria

- [ ] Test fails (RED) if operator missing from ALL_OPERATORS
- [ ] Test passes (GREEN) when all operators registered
- [ ] Test runs in <5 seconds for critical operators
- [ ] Test expanded (QA) to cover all 60+ operators in <30 seconds
- [ ] Removing any operator from ALL_OPERATORS causes test to fail (verified manually)
- [ ] Error messages clearly indicate which operator failed and why

### DO NOT

- ❌ Test operators not in ALL_OPERATORS (that's a unit test concern)
- ❌ Write slow tests (>30 seconds total)
- ❌ Skip database cleanup (use class_db_pool fixture)
- ❌ Test SQL generation (that's in unit tests)

---

## Phase 2: Critical Gaps - Operators & Type System

### Objective
Fill the most critical gaps: `sql/where/operators` and `core/type_system` (80 scalars with 0 integration tests).

### Context

**Current State**:
- 80+ custom scalar implementations in `src/fraiseql/types/scalars/`
- No integration tests verify scalars work in queries, mutations, WHERE clauses
- No registry to enumerate all scalars (unlike `ALL_OPERATORS`)

**Problem**:
- Scalars tested in isolation but not in complete GraphQL pipeline
- No test catches: scalar works in Python but fails in database roundtrip
- No test verifies scalar works in WHERE clause with operators

**Solution**:
- Create scalar registry helper for enumeration
- Test each scalar in: query, mutation, WHERE clause, database roundtrip

### Files to Create

- `src/fraiseql/testing/scalar_registry.py` - Helper to enumerate scalars
- `tests/integration/core/test_scalar_database_roundtrip.py`
- `tests/integration/core/test_scalar_where_clause_integration.py`
- `tests/integration/sql/where/test_where_operators_e2e.py`

### Implementation Steps

#### Step 2.1: Create Scalar Registry Helper [RED]

**Task**: Create helper function to enumerate all custom scalars.

**File**: `src/fraiseql/testing/scalar_registry.py`

```python
"""Scalar registry for testing.

Provides utilities to enumerate all custom scalars for meta-integration tests.
"""

from typing import Any

# Scalar test data (examples for each scalar type)
# Expand this as you discover scalars
SCALAR_TEST_DATA = {
    "UUID": "550e8400-e29b-41d4-a716-446655440000",
    "DateTime": "2024-01-15T10:30:00Z",
    "Date": "2024-01-15",
    "EmailAddress": "test@example.com",
    "URL": "https://example.com",
    "PhoneNumber": "+1-555-123-4567",
    "IPAddress": "192.168.1.1",
    "MACAddress": "00:1B:44:11:3A:B7",
    # Add more as needed
}


def get_all_custom_scalar_names() -> list[str]:
    """Get names of all custom scalars.

    Returns:
        List of scalar type names (e.g., ["UUID", "DateTime", ...])
    """
    return list(SCALAR_TEST_DATA.keys())


def get_scalar_test_value(scalar_name: str) -> Any:
    """Get example test value for a scalar.

    Args:
        scalar_name: Name of the scalar type

    Returns:
        Example value for testing

    Raises:
        KeyError: If scalar not in SCALAR_TEST_DATA
    """
    return SCALAR_TEST_DATA[scalar_name]
```

**Verification**:
```bash
python -c "from fraiseql.testing.scalar_registry import get_all_custom_scalar_names; print(len(get_all_custom_scalar_names()))"

# Expected: Number of scalars in SCALAR_TEST_DATA
```

#### Step 2.2: Test Scalar Database Roundtrip [GREEN]

**Task**: Test each scalar can be saved to database and retrieved.

**File**: `tests/integration/core/test_scalar_database_roundtrip.py`

```python
"""Test custom scalars persist correctly to database."""

import pytest
from fraiseql.testing.scalar_registry import (
    get_all_custom_scalar_names,
    get_scalar_test_value,
)


class TestScalarDatabaseRoundtrip:
    """Test all custom scalars work with database persistence."""

    @pytest.mark.asyncio
    @pytest.mark.parametrize("scalar_name", get_all_custom_scalar_names())
    async def test_scalar_persists_and_retrieves(
        self,
        scalar_name: str,
        db_connection,
        test_schema
    ):
        """Test scalar can be saved to and retrieved from database.

        Args:
            scalar_name: Name of scalar type (e.g., "UUID", "DateTime")
            db_connection: Database connection fixture
            test_schema: GraphQL schema fixture
        """
        from tests.utils.graphql_test_client import GraphQLTestClient

        client = GraphQLTestClient(test_schema)
        test_value = get_scalar_test_value(scalar_name)

        # Create record with scalar value via mutation
        mutation = f"""
        mutation CreateTest($value: {scalar_name}!) {{
            createTest(input: {{scalarField: $value}}) {{
                id
                scalarField
            }}
        }}
        """

        response = await client.query(
            query=mutation,
            result_type=dict,
            variables={"value": test_value}
        )

        assert response.ok, (
            f"Failed to create record with {scalar_name}: {response.errors}"
        )

        created_id = response.data["createTest"]["id"]
        created_value = response.data["createTest"]["scalarField"]

        # Retrieve record via query
        query = f"""
        query GetTest($id: ID!) {{
            test(id: $id) {{
                id
                scalarField
            }}
        }}
        """

        response = await client.query(
            query=query,
            result_type=dict,
            variables={"id": created_id}
        )

        assert response.ok, (
            f"Failed to retrieve record with {scalar_name}: {response.errors}"
        )

        retrieved_value = response.data["test"]["scalarField"]

        # Verify roundtrip
        assert retrieved_value == created_value, (
            f"Scalar {scalar_name} roundtrip failed: "
            f"created {created_value}, retrieved {retrieved_value}"
        )
```

**Verification**:
```bash
pytest tests/integration/core/test_scalar_database_roundtrip.py -v

# Expected output:
# test_scalar_persists_and_retrieves[UUID] PASSED
# test_scalar_persists_and_retrieves[DateTime] PASSED
# test_scalar_persists_and_retrieves[EmailAddress] PASSED
# ... (one test per scalar)
```

#### Step 2.3: Refactor for Test Performance [REFACTOR]

**Task**: Optimize test to run faster (currently might be slow with 80+ scalars).

**Changes**:
- Use single table with multiple scalar columns instead of creating table per test
- Batch insertions where possible
- Use class-scoped fixtures for database setup

**Verification**:
```bash
pytest tests/integration/core/test_scalar_database_roundtrip.py -v --durations=10

# Expected: <10 seconds for all scalar tests
```

#### Step 2.4: Add Edge Cases and Error Conditions [QA]

**Task**: Test edge cases like null values, invalid values, type coercion.

**Changes**:
- Test null scalar values
- Test invalid scalar values (expect errors)
- Test scalar arrays
- Test scalars in nested objects

**Verification**:
```bash
pytest tests/integration/core/test_scalar_database_roundtrip.py -v

# Expected: All tests pass, including edge cases
# Example: test_scalar_rejects_invalid_value[UUID] PASSED
```

### Verification Commands

```bash
# Test all scalars
pytest tests/integration/core/ -v -k "scalar"

# Check coverage
pytest tests/integration/core/ --cov=fraiseql.types.scalars --cov-report=html

# Performance benchmark
pytest tests/integration/core/ --durations=0
```

### Acceptance Criteria

- [ ] Scalar registry helper created with 80+ scalars
- [ ] All scalars tested in database roundtrip
- [ ] Test suite runs in <10 seconds
- [ ] Edge cases tested (null, invalid, arrays)
- [ ] Any scalar that fails roundtrip is flagged with clear error message
- [ ] Tests use class-scoped fixtures for performance

### DO NOT

- ❌ Test scalar parsing logic (that's in unit tests)
- ❌ Create one table per scalar (too slow)
- ❌ Skip cleanup between tests
- ❌ Hardcode scalar list (use registry)

---

## Phase 3: High Priority Areas - Mutations & Utils

### Objective
Address mutations (complex logic, no e2e validation) and utils (critical utilities affecting all features).

### Context

**Current State**:
- Mutations have unit tests for individual features (auto-populate, input conversion, etc.)
- No end-to-end test of complete mutation lifecycle (create → update → delete)
- Utils tested in isolation but not in real schema context

**Problem**:
- Mutation features work individually but might conflict when combined
- Case conversion works in unit tests but might fail with real schema field names
- No test verifies complete CRUD workflow

**Solution**:
- End-to-end mutation lifecycle test
- Test utils with real schema field names from introspection

### Files to Create

- `tests/integration/mutations/test_mutation_lifecycle.py`
- `tests/integration/mutations/test_nested_input_integration.py`
- `tests/integration/utils/test_case_conversion_real_schema.py`

### Implementation Steps

#### Step 3.1: Create Mutation Lifecycle Test [RED]

**Task**: Write test for complete CRUD workflow.

**File**: `tests/integration/mutations/test_mutation_lifecycle.py`

```python
"""End-to-end mutation lifecycle tests."""

import pytest


class TestMutationLifecycle:
    """Test complete mutation lifecycle: Create → Read → Update → Delete."""

    @pytest.mark.asyncio
    async def test_crud_workflow(self, db_connection, test_schema):
        """Test complete CRUD workflow with auto-population and validation.

        This test verifies:
        1. Create mutation with auto-populated fields (createdAt, updatedAt)
        2. Read query retrieves created record
        3. Update mutation modifies record
        4. Delete mutation removes record
        5. Read query confirms deletion
        """
        from tests.utils.graphql_test_client import GraphQLTestClient

        client = GraphQLTestClient(test_schema)

        # 1. CREATE
        create_mutation = """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                id
                name
                email
                createdAt
                updatedAt
            }
        }
        """

        response = await client.query(
            query=create_mutation,
            result_type=dict,
            variables={
                "input": {
                    "name": "John Doe",
                    "email": "john@example.com"
                }
            }
        )

        assert response.ok, f"Create failed: {response.errors}"
        user = response.data["createUser"]
        assert user["id"] is not None
        assert user["name"] == "John Doe"
        assert user["email"] == "john@example.com"
        assert user["createdAt"] is not None, "createdAt should be auto-populated"
        assert user["updatedAt"] is not None, "updatedAt should be auto-populated"

        user_id = user["id"]
        created_at = user["createdAt"]

        # 2. READ
        read_query = """
        query GetUser($id: ID!) {
            user(id: $id) {
                id
                name
                email
                createdAt
            }
        }
        """

        response = await client.query(
            query=read_query,
            result_type=dict,
            variables={"id": user_id}
        )

        assert response.ok, f"Read failed: {response.errors}"
        assert response.data["user"]["id"] == user_id
        assert response.data["user"]["createdAt"] == created_at

        # 3. UPDATE
        update_mutation = """
        mutation UpdateUser($id: ID!, $input: UpdateUserInput!) {
            updateUser(id: $id, input: $input) {
                id
                name
                email
                updatedAt
            }
        }
        """

        response = await client.query(
            query=update_mutation,
            result_type=dict,
            variables={
                "id": user_id,
                "input": {"name": "Jane Doe"}
            }
        )

        assert response.ok, f"Update failed: {response.errors}"
        updated_user = response.data["updateUser"]
        assert updated_user["name"] == "Jane Doe"
        assert updated_user["email"] == "john@example.com", "Email unchanged"

        # 4. DELETE
        delete_mutation = """
        mutation DeleteUser($id: ID!) {
            deleteUser(id: $id) {
                success
            }
        }
        """

        response = await client.query(
            query=delete_mutation,
            result_type=dict,
            variables={"id": user_id}
        )

        assert response.ok, f"Delete failed: {response.errors}"
        assert response.data["deleteUser"]["success"] is True

        # 5. VERIFY DELETION
        response = await client.query(
            query=read_query,
            result_type=dict,
            variables={"id": user_id}
        )

        assert response.ok
        assert response.data["user"] is None, "User should be deleted"
```

**Verification**:
```bash
pytest tests/integration/mutations/test_mutation_lifecycle.py -v

# Expected: FAILED - schema not set up yet (RED phase)
```

#### Step 3.2: Make Test Pass [GREEN]

**Task**: Configure test schema with User type and CRUD mutations.

**Verification**:
```bash
pytest tests/integration/mutations/test_mutation_lifecycle.py -v

# Expected: PASSED
```

#### Step 3.3: Refactor Test Structure [REFACTOR]

**Task**: Extract common patterns, improve readability.

**Changes**:
- Extract GraphQL queries to module-level constants
- Create helper methods for common assertions
- Use fixtures for test data

#### Step 3.4: Add More Mutation Scenarios [QA]

**Task**: Test edge cases and complex scenarios.

**Scenarios**:
- Nested input objects
- Mutation validation errors
- Concurrent mutations
- Batch mutations

**Verification**:
```bash
pytest tests/integration/mutations/ -v

# Expected: All mutation tests pass
```

### Verification Commands

```bash
# Test complete mutation suite
pytest tests/integration/mutations/ -v

# Test with coverage
pytest tests/integration/mutations/ --cov=fraiseql.mutations --cov-report=html

# Performance check
pytest tests/integration/mutations/ --durations=5
```

### Acceptance Criteria

- [ ] Complete CRUD lifecycle test passes
- [ ] Auto-population verified (createdAt, updatedAt)
- [ ] Nested input objects tested
- [ ] Validation errors tested
- [ ] Tests run in <5 seconds
- [ ] Error messages clearly indicate which step failed

### DO NOT

- ❌ Test mutation resolver logic in detail (that's unit tests)
- ❌ Test database constraints (that's database tests)
- ❌ Skip verification steps (each CRUD step must be verified)

---

## Phase 4: Systematic Coverage - Remaining Areas

### Objective
Address remaining 22 areas with integration test gaps using systematic approach.

### Context

**Current State**:
- Phases 1-3 covered: operators, scalars, mutations (highest priority)
- Remaining areas: decorators, validation, db utils, parsing, etc.

**Approach**:
Apply the same patterns from Phases 1-3:
- Decorators: Test combinations and schema integration
- Validation: Test in complete pipeline (not just isolated validation)
- DB utils: Test with real schema operations
- Parsing: Test with complex real-world queries

### Files to Create

For each remaining area, create integration tests following the pattern:

```
tests/integration/
  decorators/
    test_decorator_combinations.py
    test_decorator_schema_integration.py
  validation/
    test_input_validation_pipeline.py
  db/
    test_db_utils_integration.py
  core/
    test_parsing_integration.py
```

### Implementation Pattern (Repeat for Each Area)

For each remaining area, follow this 4-phase pattern:

#### Step 4.X.1: RED - Write Failing Test

Create test that fails because integration doesn't work yet.

#### Step 4.X.2: GREEN - Make Test Pass

Implement minimal changes to make test pass.

#### Step 4.X.3: REFACTOR - Improve Code Quality

Clean up implementation without changing behavior.

#### Step 4.X.4: QA - Add Edge Cases

Add comprehensive test coverage for edge cases.

### Example: Decorator Integration Tests

```python
# tests/integration/decorators/test_decorator_combinations.py

import pytest
from fraiseql.decorators import query, mutation, field


class TestDecoratorCombinations:
    """Test decorators work correctly when combined."""

    def test_query_and_field_decorators_together(self, test_schema):
        """Test @query and @field can be used on same type."""
        # Test implementation following RED → GREEN → REFACTOR → QA
        pass

    def test_mutation_with_validation_decorator(self, test_schema):
        """Test @mutation works with @validate."""
        pass
```

### Verification Commands

```bash
# Test each area systematically
pytest tests/integration/decorators/ -v
pytest tests/integration/validation/ -v
pytest tests/integration/db/ -v
pytest tests/integration/core/ -v

# Run all remaining integration tests
pytest tests/integration/ -v -k "not operators and not scalars and not mutations"

# Coverage for remaining areas
pytest tests/integration/ --cov=fraiseql --cov-report=html
```

### Acceptance Criteria

- [ ] All 22 remaining areas have integration tests
- [ ] Each area follows RED → GREEN → REFACTOR → QA pattern
- [ ] All tests pass consistently (<5% flakiness)
- [ ] Test suite completes in <10 minutes total
- [ ] Coverage increased to 85%+ for tested modules

### DO NOT

- ❌ Rush through areas without proper TDD workflow
- ❌ Copy-paste tests without understanding
- ❌ Skip refactoring step (code quality matters)
- ❌ Ignore QA step (edge cases prevent bugs)

---

## Phase 5: Quality Assurance & Documentation

### Objective
Ensure all integration tests are robust, documented, and prevent future regressions.

### Context

**Current State**:
- Phases 1-4 created integration tests for all 27 gap areas
- Tests pass but might not be maintainable by junior engineers
- No documentation on patterns used

**Goal**:
- Document integration test patterns
- Add CI automation
- Verify test quality and coverage

### Files to Modify/Create

- `docs/testing/integration-test-patterns.md` - Pattern documentation
- `tests/README.md` - Add integration test section
- `.github/workflows/integration-tests.yml` - CI configuration (if doesn't exist)

### Implementation Steps

#### Step 5.1: Document Integration Test Patterns [RED]

**Task**: Create documentation explaining the patterns used.

**File**: `docs/testing/integration-test-patterns.md`

**Content**:
```markdown
# Integration Test Patterns

## Meta-Integration Tests

**Purpose**: Prevent "works in isolation, fails in production" bugs.

**Pattern**:
1. Enumerate all components in a category (e.g., ALL_OPERATORS)
2. For each component, test in real GraphQL query
3. Assert no errors

**Example**: `test_all_operators_registration.py`

**When to use**:
- Registry-based features (operators, scalars)
- Features that require explicit registration

## End-to-End Tests

**Purpose**: Verify complete user workflows.

**Pattern**:
1. Create → Read → Update → Delete
2. Verify each step
3. Assert final state

**Example**: `test_mutation_lifecycle.py`

**When to use**:
- CRUD operations
- Multi-step workflows

## Component Integration Tests

**Purpose**: Test component interaction points.

**Pattern**:
1. Component A produces output
2. Component B consumes output
3. Assert B works correctly with A's output

**Example**: `test_decorator_combinations.py`

**When to use**:
- Features that combine (decorators, middleware)
- Data transformations (input → processing → output)
```

**Verification**:
```bash
# Check documentation exists
ls docs/testing/integration-test-patterns.md

# Verify markdown is valid
markdownlint docs/testing/integration-test-patterns.md
```

#### Step 5.2: Update Test README [GREEN]

**Task**: Add integration test section to `tests/README.md`.

**Changes**:
- Add "Integration Tests" section
- Link to pattern documentation
- Explain how to run tests
- Document fixtures available

#### Step 5.3: Add CI Integration [REFACTOR]

**Task**: Ensure integration tests run in CI.

**Note**: Check if `.github/workflows/` exists first. If not, document how to add.

**Verification**:
```bash
# Check if CI config exists
ls .github/workflows/

# If exists, verify integration tests are included
grep -r "pytest.*integration" .github/workflows/
```

#### Step 5.4: Quality Gate Verification [QA]

**Task**: Verify test quality meets standards.

**Checks**:
- [ ] All tests pass consistently (run 10 times)
- [ ] No flaky tests (failing randomly)
- [ ] Test coverage ≥85% for tested modules
- [ ] Test suite runs in <10 minutes
- [ ] Error messages are clear and actionable

**Verification**:
```bash
# Run tests 10 times to check for flakiness
for i in {1..10}; do pytest tests/integration/ -q || echo "FAILED on run $i"; done

# Check coverage
pytest tests/integration/ --cov=fraiseql --cov-report=term --cov-fail-under=85

# Check performance
time pytest tests/integration/

# Should complete in <10 minutes
```

### Verification Commands

```bash
# Verify documentation exists and is valid
ls docs/testing/integration-test-patterns.md
markdownlint docs/testing/

# Run complete integration test suite
pytest tests/integration/ -v

# Check coverage
pytest tests/integration/ --cov=fraiseql --cov-report=html
coverage report --fail-under=85

# Performance check
time pytest tests/integration/
```

### Acceptance Criteria

- [ ] Integration test patterns documented with examples
- [ ] `tests/README.md` updated with integration test section
- [ ] CI runs integration tests (or documented how to add)
- [ ] Test suite passes 10 consecutive runs (no flakiness)
- [ ] Coverage ≥85% for integration-tested modules
- [ ] Test suite completes in <10 minutes
- [ ] Documentation enables junior engineer to write new integration tests

### DO NOT

- ❌ Skip documentation (junior engineer needs clear examples)
- ❌ Accept flaky tests (fix or remove them)
- ❌ Ignore performance issues (slow tests won't get run)
- ❌ Write documentation without examples

---

## Risk Mitigation

### Critical Risks

#### 1. Test Flakiness
**Risk**: Database state pollution between tests causes random failures.

**Mitigation**:
- Use class-scoped `class_db_pool` fixture (see fraiseql-testing.md)
- Ensure database cleanup in teardown
- Use transactions that rollback
- Isolate test data with unique IDs

**Detection**:
```bash
# Run tests 10 times, should pass all runs
for i in {1..10}; do pytest tests/integration/ || echo "FAIL"; done
```

#### 2. Slow Test Suite
**Risk**: Integration tests take too long, developers skip them.

**Mitigation**:
- Use class-scoped fixtures (setup once per class)
- Batch operations where possible
- Use parametrized tests instead of loops
- Profile slow tests: `pytest --durations=10`

**Target**: <10 minutes for full integration test suite

#### 3. Missing Edge Cases
**Risk**: Meta-tests miss edge cases that cause bugs.

**Mitigation**:
- Combine meta-tests with targeted integration tests
- Add tests for known bug patterns (e.g., network operators bug)
- Use QA phase to add edge cases
- Code review focuses on test coverage

#### 4. Test Data Complexity
**Risk**: Creating realistic test data for 80+ scalars is complex.

**Mitigation**:
- Use `SCALAR_TEST_DATA` registry with example values
- Create factory functions for complex data
- Document test data patterns
- Reuse fixtures across tests

#### 5. Schema Configuration Complexity
**Risk**: `test_schema` fixture might not include all types needed.

**Mitigation**:
- Document required schema configuration
- Use modular schema building (add types as needed)
- Create helper functions for common schema patterns
- Reference fraiseql-testing.md for schema isolation patterns

### Success Metrics

**Coverage**:
- ✅ All 27 gap areas have integration tests
- ✅ 85%+ code coverage for integration-tested modules

**Reliability**:
- ✅ <5% test failure rate in CI
- ✅ Zero flaky tests (10 consecutive passes)

**Performance**:
- ✅ Integration test suite completes in <10 minutes
- ✅ No individual test takes >30 seconds

**Maintainability**:
- ✅ Junior engineers can write new integration tests using documented patterns
- ✅ Clear error messages when tests fail
- ✅ Documentation with examples for all patterns

---

## Implementation Order

**Dependencies**:
- Phase 1-5 depend on Phase 0 (must understand infrastructure first)
- Phase 5 depends on Phase 1-4 (can't document patterns until they exist)
- Phases 1-4 can be worked in parallel after Phase 0 (independent areas)

**Recommended sequence for junior engineer**:

1. **Phase 0** (Discovery) - MUST DO FIRST
   - Understand existing infrastructure
   - Document what's available
   - Identify what needs to be created

2. **Phase 1** (Meta-tests) - Highest ROI
   - Prevents most common bug pattern
   - Builds confidence in TDD workflow
   - Creates reusable patterns for later phases

3. **Phase 2 or 3** (Operators/Scalars OR Mutations) - High Priority
   - Can be done in either order
   - Both are critical gaps
   - Similar complexity

4. **Phase 4** (Remaining areas) - Systematic Completion
   - Apply patterns learned from Phases 1-3
   - Can be split into sub-tasks

5. **Phase 5** (Documentation & QA) - Polish
   - Document patterns discovered
   - Ensure quality standards met
   - Make maintainable for future engineers

**Parallel work opportunities**:
- After Phase 0, can work on Phase 1-4 in parallel if multiple engineers available
- Within Phase 4, different areas can be worked independently

---

## DO NOT - Global Rules

These rules apply to ALL phases:

### Testing

- ❌ Write integration tests that duplicate unit test logic
- ❌ Create slow tests (>30 seconds per test)
- ❌ Skip database cleanup between tests
- ❌ Forget to test error cases and edge conditions
- ❌ Write tests that depend on external services
- ❌ Ignore test failures in CI
- ❌ Accept flaky tests (fix or remove)

### Code Quality

- ❌ Assume APIs exist without verifying (always check in Phase 0)
- ❌ Hardcode values that should come from registry
- ❌ Skip refactoring step (code quality matters)
- ❌ Copy-paste without understanding
- ❌ Rush through TDD phases (each phase has a purpose)

### Documentation

- ❌ Skip documentation (junior engineer needs examples)
- ❌ Write documentation without code examples
- ❌ Assume patterns are obvious (document everything)
- ❌ Forget to update docs when code changes

### Performance

- ❌ Create one database table per test
- ❌ Skip performance optimization (slow tests won't get run)
- ❌ Ignore `--durations` warnings
- ❌ Use module-scoped fixtures when class-scoped would work

### CI/CD

- ❌ Commit failing tests
- ❌ Skip CI integration (tests must run automatically)
- ❌ Ignore CI failures
- ❌ Disable tests instead of fixing them

---

## Success Criteria - Final Checklist

When all phases complete, verify:

### Coverage
- [ ] All 27 gap areas have integration tests
- [ ] 85%+ code coverage for tested modules
- [ ] Meta-tests cover all operators in ALL_OPERATORS
- [ ] All 80+ custom scalars tested

### Quality
- [ ] All tests pass consistently (10 consecutive runs)
- [ ] Zero flaky tests
- [ ] Clear, actionable error messages
- [ ] Code review approved

### Performance
- [ ] Full integration suite runs in <10 minutes
- [ ] No individual test >30 seconds
- [ ] Class-scoped fixtures used appropriately

### Documentation
- [ ] Integration test patterns documented with examples
- [ ] Test infrastructure documented (fixtures, utilities)
- [ ] README updated with how to run tests
- [ ] Junior engineer can write new tests using docs

### Automation
- [ ] Tests run in CI automatically
- [ ] Coverage reports generated
- [ ] Performance metrics tracked

---

## References

- **Gap Analysis**: `/tmp/fraiseql-integration-test-gaps-analysis.md`
- **FraiseQL Testing Patterns**: `/home/lionel/.claude/skills/fraiseql-testing.md`
- **Test Infrastructure**: `docs/testing/existing-test-infrastructure.md` (create in Phase 0)
- **Integration Patterns**: `docs/testing/integration-test-patterns.md` (create in Phase 5)

---

**This plan transforms FraiseQL's testing from "works in isolation" to "works when integrated", preventing the class of bugs that caused the network operators issue.**
