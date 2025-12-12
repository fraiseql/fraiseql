# Phase 2: Integration & Verification (REFACTOR → QA)

## 🎯 Objective

Verify that auto-populated fields:
1. Appear in GraphQL schema introspection
2. Are queryable without errors
3. Only appear in response when requested (GraphQL spec compliance)

**Time**: 1 hour

---

## 📋 Context

Phase 1 fixed the decorator. Now we verify the fix works end-to-end:
- Schema generation picks up fields from `__gql_fields__`
- GraphQL executor allows querying these fields
- Fields only returned when explicitly requested

---

## 🔬 Integration Tests

### Step 1: Schema Introspection Test (20 min)

**Create**: `tests/integration/test_mutation_schema_complete.py`

```python
"""Test that auto-populated fields appear in GraphQL schema."""
import pytest
from graphql import graphql_sync
from fraiseql import FraiseQL
from fraiseql.mutations.decorators import success, mutation
from fraiseql.decorators import fraise_type, fraise_input


@fraise_type(sql_source="machines")
class Machine:
    id: str
    name: str


@fraise_input
class CreateMachineInput:
    name: str


@success
class CreateMachineSuccess:
    machine: Machine


@mutation
async def create_machine(info, input: CreateMachineInput) -> CreateMachineSuccess:
    """Dummy mutation for testing."""
    # Not actually called in introspection tests
    pass


@pytest.mark.asyncio
async def test_schema_includes_auto_populated_fields(db_pool):
    """GraphQL schema introspection should show all auto-populated fields."""

    fraiseql = FraiseQL(pool=db_pool)
    schema = fraiseql.build_schema()

    introspection_query = """
        query {
            __type(name: "CreateMachineSuccess") {
                fields {
                    name
                    type {
                        kind
                        name
                        ofType {
                            kind
                            name
                        }
                    }
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)

    assert result.errors is None, f"Introspection errors: {result.errors}"

    field_names = {f["name"] for f in result.data["__type"]["fields"]}

    # All expected fields must be present
    assert "machine" in field_names, "Original field missing"
    assert "status" in field_names, "status field missing from schema"
    assert "message" in field_names, "message field missing from schema"
    assert "errors" in field_names, "errors field missing from schema"
    assert "updatedFields" in field_names, "updatedFields missing (should be camelCase)"
    assert "id" in field_names, "id field missing from schema"

    print(f"✅ Schema fields: {sorted(field_names)}")


@pytest.mark.asyncio
async def test_field_types_correct(db_pool):
    """Auto-populated fields should have correct GraphQL types."""

    fraiseql = FraiseQL(pool=db_pool)
    schema = fraiseql.build_schema()

    introspection_query = """
        query {
            __type(name: "CreateMachineSuccess") {
                fields {
                    name
                    type {
                        kind
                        name
                        ofType {
                            kind
                            name
                        }
                    }
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)
    fields_by_name = {f["name"]: f["type"] for f in result.data["__type"]["fields"]}

    # status: String! (NON_NULL)
    assert fields_by_name["status"]["kind"] == "NON_NULL"
    assert fields_by_name["status"]["ofType"]["name"] == "String"

    # message: String (nullable)
    assert fields_by_name["message"]["kind"] == "SCALAR"
    assert fields_by_name["message"]["name"] == "String"

    # id: String (nullable)
    assert fields_by_name["id"]["kind"] == "SCALAR"
    assert fields_by_name["id"]["name"] == "String"

    print("✅ Field types correct")
```

**Run**:
```bash
pytest tests/integration/test_mutation_schema_complete.py -xvs
```

---

### Step 2: Query Execution Tests (20 min)

**Create**: `tests/integration/test_mutation_field_queries.py`

```python
"""Test that auto-populated fields are queryable and follow GraphQL spec."""
import pytest


@pytest.mark.asyncio
async def test_can_query_auto_populated_fields(graphql_client, db_pool):
    """Auto-populated fields should be queryable without errors."""

    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    status
                    message
                    errors { code message }
                    id
                    updatedFields
                    machine { id name }
                }
            }
        }
    """

    variables = {
        "input": {
            "name": "Test Machine",
        }
    }

    result = await graphql_client.execute(query, variables)

    # Should not have schema validation errors
    assert result.get("errors") is None, f"Query errors: {result.get('errors')}"

    # All queried fields should be in response
    data = result["data"]["createMachine"]
    assert "status" in data, "status field not in response"
    assert "message" in data, "message field not in response"
    assert "errors" in data, "errors field not in response"
    assert "id" in data, "id field not in response"
    assert "updatedFields" in data, "updatedFields not in response"
    assert "machine" in data, "machine field not in response"

    # Verify values
    assert data["status"] == "success"
    assert isinstance(data["errors"], list)
    assert isinstance(data["updatedFields"], list)

    print(f"✅ All fields queryable, response keys: {list(data.keys())}")


@pytest.mark.asyncio
async def test_fields_optional_in_query(graphql_client, db_pool):
    """Auto-populated fields should be optional (don't have to query them)."""

    # Query WITHOUT status, message, errors
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    machine { id name }
                }
            }
        }
    """

    variables = {"input": {"name": "Test Machine"}}
    result = await graphql_client.execute(query, variables)

    assert result.get("errors") is None
    data = result["data"]["createMachine"]

    # Only requested field should be present
    assert "machine" in data

    # Per CTO feedback: GraphQL executor should filter unrequested fields
    # These fields should NOT be in response if not requested
    assert "status" not in data, "status in response but not requested (GraphQL spec violation)"
    assert "message" not in data, "message in response but not requested"
    assert "id" not in data, "id in response but not requested"

    print(f"✅ Only requested fields in response: {list(data.keys())}")


@pytest.mark.asyncio
async def test_graphql_spec_compliance(graphql_client, db_pool):
    """Verify GraphQL spec: fields only in response if explicitly requested."""

    # Query with ONLY __typename
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    __typename
                }
            }
        }
    """

    variables = {"input": {"name": "Test"}}
    result = await graphql_client.execute(query, variables)

    data = result["data"]["createMachine"]

    # ONLY __typename should be present
    assert "__typename" in data
    assert len(data) == 1, f"Extra fields in response: {list(data.keys())}"

    print("✅ GraphQL spec compliant - only requested fields returned")
```

**Run**:
```bash
pytest tests/integration/test_mutation_field_queries.py -xvs
```

---

### Step 3: Rust Response + GraphQL Filtering Verification (10 min)

**Critical Test**: Verify that Rust builds complete response but GraphQL executor filters correctly.

Add to `test_mutation_field_queries.py`:

```python
@pytest.mark.asyncio
async def test_rust_response_graphql_filtering(graphql_client, db_pool):
    """Verify Rust builds complete response, GraphQL executor filters fields."""

    # Query 1: Request only 'status'
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    status
                }
            }
        }
    """

    result = await graphql_client.execute(query, {"input": {"name": "Test"}})
    data = result["data"]["createMachine"]

    # Only 'status' should be in response
    assert "status" in data
    assert "message" not in data, "GraphQL executor should filter unrequested fields"
    assert "machine" not in data

    # Query 2: Request 'status' and 'message'
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    status
                    message
                }
            }
        }
    """

    result = await graphql_client.execute(query, {"input": {"name": "Test"}})
    data = result["data"]["createMachine"]

    # Both requested fields should be present
    assert "status" in data
    assert "message" in data
    # But not unrequested ones
    assert "machine" not in data
    assert "errors" not in data

    print("✅ Rust + GraphQL executor working correctly")
```

---

### Step 4: External Validation - PrintOptim (10 min)

Run PrintOptim's failing tests to verify the fix:

```bash
cd ~/code/printoptim_backend

# Run mutation response structure tests
pytest tests/api/mutations/test_mutation_response_structure.py -xvs

# Expected: 138 previously failing tests now pass
```

**If tests fail**:
1. Check which fields are missing from schema
2. Verify field names match (camelCase vs snake_case)
3. Check if `updatedFields` is properly registered

---

## ✅ Acceptance Criteria

- [ ] Schema introspection shows all auto-populated fields
- [ ] All fields have correct GraphQL types
- [ ] Fields are queryable without "Cannot query field X" errors
- [ ] Fields only appear in response when explicitly requested
- [ ] GraphQL spec compliance verified
- [ ] Rust response builder + GraphQL executor interaction correct
- [ ] PrintOptim 138 tests pass

---

## 🔍 Debugging Commands

If tests fail, use these to debug:

```bash
# Check GraphQL schema introspection manually
pytest tests/integration/test_mutation_schema_complete.py::test_schema_includes_auto_populated_fields -xvs --pdb

# Test query execution with verbose output
pytest tests/integration/test_mutation_field_queries.py::test_can_query_auto_populated_fields -xvs -vv

# PrintOptim debugging
cd ~/code/printoptim_backend
pytest tests/api/mutations/test_mutation_response_structure.py -xvs --pdb
```

---

## 🐛 Common Issues & Solutions

### Issue 1: "Cannot query field 'updatedFields'"
**Cause**: Field name in decorator is `updated_fields` (snake_case) but GraphQL expects `updatedFields` (camelCase)

**Solution**: FraiseQL should auto-convert snake_case to camelCase. Verify `graphql_name=None` in `FraiseQLField` (which triggers auto-conversion).

### Issue 2: Fields still appearing when not requested
**Cause**: Rust response builder adds fields unconditionally

**Solution**: Per CTO feedback, this is OK - GraphQL executor should filter. If this fails, check GraphQL-Python version and executor configuration.

### Issue 3: PrintOptim tests still failing
**Cause**: Schema mismatch or field naming issue

**Solution**:
1. Run introspection in PrintOptim
2. Compare field names between FraiseQL schema and PrintOptim expectations
3. Check if any PrintOptim-specific fields are missing

---

## 📊 Success Metrics

### Before Fix
- ❌ 138 tests failing in PrintOptim
- ❌ "Cannot query field X" errors
- ❌ Fields in response without being requested

### After Fix
- ✅ All tests pass
- ✅ Fields queryable
- ✅ GraphQL spec compliant
- ✅ Schema introspection complete

---

**Next**: [Phase 3: Documentation & Commit](./phase-3-documentation-commit.md)
