# Phase 3: Integration & Verification (Updated)

## 🎯 Objective

Verify that the complete fix works end-to-end:
1. Python: Auto-populated fields appear in GraphQL schema
2. Rust: Only requested fields returned in mutation responses
3. GraphQL spec compliance: No unrequested fields in responses

**Time**: 1 hour

---

## 📋 Context

We've completed:
- ✅ Phase 1: Python decorator adds fields to `__gql_fields__`
- ✅ Phase 2: Rust filters response based on field selection

Now we verify both parts work together correctly.

---

## 🔬 Integration Tests

### Step 1: Schema Introspection Test (15 min)

**Verify Python decorator fix worked** - fields visible in schema.

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
    """Test mutation."""
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
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)

    assert result.errors is None, f"Introspection errors: {result.errors}"

    field_names = {f["name"] for f in result.data["__type"]["fields"]}

    # All expected fields must be present in schema
    assert "machine" in field_names, "Original field missing"
    assert "status" in field_names, "status missing from schema"
    assert "message" in field_names, "message missing from schema"
    assert "errors" in field_names, "errors missing from schema"
    assert "updatedFields" in field_names, "updatedFields missing (should be camelCase)"
    assert "id" in field_names, "id missing from schema"

    print(f"✅ Schema fields: {sorted(field_names)}")
```

**Run**:
```bash
pytest tests/integration/test_mutation_schema_complete.py::test_schema_includes_auto_populated_fields -xvs
```

---

### Step 2: End-to-End Mutation Tests (30 min)

**Verify Rust filtering works** - only requested fields in response.

**Create**: `tests/integration/test_mutation_field_selection_e2e.py`

```python
"""End-to-end tests for mutation field selection."""
import pytest


@pytest.mark.asyncio
async def test_only_requested_fields_in_response(graphql_client, db_pool):
    """Verify only requested fields appear in mutation response."""

    # Query that requests ONLY 'machine' field
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

    assert result.get("errors") is None, f"Query errors: {result.get('errors')}"

    data = result["data"]["createMachine"]

    # Should include __typename (always present)
    assert "__typename" in data

    # Should include requested field
    assert "machine" in data, "machine should be present (requested)"

    # Should NOT include unrequested fields (CRITICAL TEST)
    assert "id" not in data, "id should NOT be in response (not requested)"
    assert "message" not in data, "message should NOT be in response (not requested)"
    assert "status" not in data, "status should NOT be in response (not requested)"
    assert "errors" not in data, "errors should NOT be in response (not requested)"
    assert "updatedFields" not in data, "updatedFields should NOT be in response (not requested)"

    print(f"✅ Response contains only requested fields: {list(data.keys())}")


@pytest.mark.asyncio
async def test_all_fields_queryable(graphql_client, db_pool):
    """Verify all auto-populated fields are queryable."""

    # Query that requests ALL auto-populated fields
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    id
                    status
                    message
                    errors { code message }
                    updatedFields
                    machine { id name }
                }
            }
        }
    """

    variables = {"input": {"name": "Test Machine"}}
    result = await graphql_client.execute(query, variables)

    # Should not have schema validation errors
    assert result.get("errors") is None, f"Query errors: {result.get('errors')}"

    data = result["data"]["createMachine"]

    # All requested fields should be present
    assert "id" in data, "id should be queryable"
    assert "status" in data, "status should be queryable"
    assert "message" in data, "message should be queryable"
    assert "errors" in data, "errors should be queryable"
    assert "updatedFields" in data, "updatedFields should be queryable"
    assert "machine" in data, "machine should be queryable"

    # Verify field values
    assert data["status"] == "success"
    assert isinstance(data["errors"], list)
    assert len(data["errors"]) == 0
    assert isinstance(data["updatedFields"], list)

    print(f"✅ All fields queryable and present: {list(data.keys())}")


@pytest.mark.asyncio
async def test_partial_field_selection(graphql_client, db_pool):
    """Verify partial field selection works correctly."""

    # Request only status, message, and machine
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    status
                    message
                    machine { id }
                }
            }
        }
    """

    variables = {"input": {"name": "Test"}}
    result = await graphql_client.execute(query, variables)

    assert result.get("errors") is None

    data = result["data"]["createMachine"]

    # Requested fields should be present
    assert "status" in data
    assert "message" in data
    assert "machine" in data

    # Unrequested fields should NOT be present
    assert "id" not in data, "id not requested"
    assert "errors" not in data, "errors not requested"
    assert "updatedFields" not in data, "updatedFields not requested"

    print(f"✅ Partial selection works: {list(data.keys())}")


@pytest.mark.asyncio
async def test_minimal_query_only_typename(graphql_client, db_pool):
    """Verify minimal query with only __typename works."""

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

    # Only __typename should be present
    assert "__typename" in data
    assert len(data) == 1, f"Only __typename should be present, got: {list(data.keys())}"

    print("✅ Minimal query returns only __typename")


@pytest.mark.asyncio
async def test_error_response_field_selection(graphql_client, db_pool):
    """Verify field selection works for error responses too."""

    # Trigger an error (e.g., duplicate name)
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineError {
                    errors { code message }
                }
            }
        }
    """

    # First create a machine
    await graphql_client.execute(query, {"input": {"name": "Duplicate"}})

    # Try to create again (should error)
    result = await graphql_client.execute(query, {"input": {"name": "Duplicate"}})

    data = result["data"]["createMachine"]

    # Should have __typename and errors
    assert "__typename" in data
    assert "errors" in data

    # Should NOT have unrequested error fields
    assert "code" not in data, "code not requested"
    assert "status" not in data, "status not requested"
    assert "message" not in data, "message not requested"

    print(f"✅ Error response filtering works: {list(data.keys())}")
```

**Run**:
```bash
pytest tests/integration/test_mutation_field_selection_e2e.py -xvs
```

---

### Step 3: GraphQL Spec Compliance Test (10 min)

**Verify we're not violating GraphQL spec.**

Add to `test_mutation_field_selection_e2e.py`:

```python
@pytest.mark.asyncio
async def test_graphql_spec_compliance_no_extra_fields(graphql_client, db_pool):
    """
    GraphQL Spec Compliance Test.

    Per GraphQL spec: Only fields explicitly requested in the selection set
    should appear in the response. Returning unrequested fields is a spec violation.

    This was the original bug - Rust was returning ALL fields regardless of selection.
    """

    # Request minimal fields
    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    machine { id }
                }
            }
        }
    """

    result = await graphql_client.execute(query, {"input": {"name": "Spec Test"}})
    data = result["data"]["createMachine"]

    # Define ALL possible fields that could be returned
    all_possible_fields = {
        "__typename",  # Always allowed
        "machine",     # Requested
        # Unrequested fields (should NOT be present):
        "id",
        "status",
        "message",
        "errors",
        "updatedFields",
        "cascade",
    }

    # Fields that should be present
    expected_fields = {"__typename", "machine"}

    # Fields that should NOT be present (GraphQL spec violation if present)
    forbidden_fields = all_possible_fields - expected_fields

    # Check for spec violations
    violations = []
    for field in forbidden_fields:
        if field in data:
            violations.append(field)

    assert len(violations) == 0, (
        f"GraphQL Spec Violation: Unrequested fields in response: {violations}\n"
        f"Response contained: {list(data.keys())}\n"
        f"Expected only: {expected_fields}"
    )

    print("✅ GraphQL spec compliant - no unrequested fields in response")
```

---

### Step 4: External Validation - PrintOptim (5 min)

**Run PrintOptim's mutation tests** to verify the fix works in production usage.

```bash
cd ~/code/printoptim_backend

# Run mutation response structure tests
pytest tests/api/mutations/test_mutation_response_structure.py -xvs

# Expected: 138 previously failing tests now pass
```

**If tests fail**:
1. Check field names (camelCase vs snake_case)
2. Verify `updatedFields` is in schema and queryable
3. Check if PrintOptim queries ALL fields or subset

---

## ✅ Acceptance Criteria

- [ ] Schema introspection shows all auto-populated fields
- [ ] Only requested fields appear in mutation responses
- [ ] All auto-populated fields are queryable without errors
- [ ] Partial field selection works correctly
- [ ] Minimal query (__typename only) works
- [ ] Error response field selection works
- [ ] GraphQL spec compliance verified
- [ ] No unrequested fields in responses
- [ ] PrintOptim 138 tests pass

---

## 🔍 Debugging Commands

If tests fail, use these to debug:

```bash
# Check schema introspection
pytest tests/integration/test_mutation_schema_complete.py -xvs --pdb

# Test field selection
pytest tests/integration/test_mutation_field_selection_e2e.py::test_only_requested_fields_in_response -xvs -vv

# Check Rust filtering directly
cd fraiseql_rs
cargo test field_selection -- --nocapture

# PrintOptim debugging
cd ~/code/printoptim_backend
pytest tests/api/mutations/ -xvs --pdb -k "field_selection"
```

---

## 🐛 Common Issues & Solutions

### Issue 1: "Cannot query field X" errors
**Cause**: Python decorator didn't add field to `__gql_fields__`

**Solution**:
- Check Phase 1 implementation
- Verify decorator is creating `FraiseQLField` instances
- Run: `python3 -c "from mutations import CreateMachineSuccess; print(CreateMachineSuccess.__gql_fields__.keys())"`

### Issue 2: Unrequested fields still in response
**Cause**: Rust filtering not working

**Solution**:
- Check Phase 2 implementation
- Verify `should_include_field()` logic
- Check `success_type_fields` is being passed correctly from Python

### Issue 3: ALL fields missing from response
**Cause**: Field selection too aggressive or wrong field names

**Solution**:
- Check field name case conversion (camelCase vs snake_case)
- Verify `success_type_fields` contains correct field names
- Check if `None` selection is being handled (should return all fields)

### Issue 4: `__typename` missing
**Cause**: `__typename` being filtered

**Solution**: Ensure `__typename` is added BEFORE field selection check

### Issue 5: PrintOptim tests still failing
**Cause**: Field naming mismatch or schema incompatibility

**Solution**:
1. Run PrintOptim with verbose output: `pytest -xvs -vv`
2. Check which fields PrintOptim expects vs what's in schema
3. Verify `updatedFields` camelCase conversion

---

## 📊 Success Metrics

### Before Fix
- ❌ Schema missing auto-populated fields
- ❌ "Cannot query field X" errors
- ❌ All fields returned regardless of selection
- ❌ 138 PrintOptim tests failing

### After Fix
- ✅ All auto-populated fields in schema
- ✅ All fields queryable
- ✅ Only requested fields in response
- ✅ GraphQL spec compliant
- ✅ 138 PrintOptim tests passing

---

## 🎯 Final Verification Checklist

Before proceeding to Phase 4:

- [ ] Schema introspection test passes
- [ ] All field selection tests pass
- [ ] Partial selection test passes
- [ ] Minimal query test passes
- [ ] Error response test passes
- [ ] GraphQL spec compliance test passes
- [ ] PrintOptim external validation passes
- [ ] No regressions in existing FraiseQL tests

**If all checkboxes checked**: ✅ Ready for Phase 4

**If any checkbox unchecked**: ❌ Debug and fix before proceeding

---

**Next**: [Phase 4: Documentation & Commit](./phase-4-documentation-commit.md)
