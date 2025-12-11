# Phase 3: Testing Strategy

## 🎯 Testing Goals

1. ✅ Verify auto-populated fields appear in GraphQL schema
2. ✅ Verify fields are queryable via GraphQL
3. ✅ Verify backward compatibility (existing tests still pass)
4. ✅ Verify edge cases handled correctly
5. ✅ Verify introspection returns correct field info

---

## 🧪 Test File Structure

### New Test Files to Create

1. **`tests/unit/mutations/test_auto_populate_schema_fields.py`** - Unit tests for decorator
2. **`tests/integration/test_mutation_schema_introspection.py`** - Schema generation tests
3. **`tests/integration/test_mutation_field_queries.py`** - End-to-end query tests

---

## 📝 Unit Tests: Decorator Behavior

**File**: `tests/unit/mutations/test_auto_populate_schema_fields.py`

### Test 1: `@success` Adds Fields to `__gql_fields__`

```python
import pytest
from fraiseql.mutations.decorators import success
from fraiseql.decorators import fraise_type


@fraise_type
class Machine:
    id: str
    name: str


def test_success_decorator_adds_fields_to_gql_fields():
    """Test that @success decorator adds status, message, errors to __gql_fields__."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    # Verify fields in __gql_fields__
    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    assert "machine" in gql_fields, "Original field should be present"
    assert "status" in gql_fields, "Auto-injected status field missing"
    assert "message" in gql_fields, "Auto-injected message field missing"
    assert "errors" in gql_fields, "Auto-injected errors field missing"
    assert "id" in gql_fields, "Auto-injected id field missing (entity detected)"
    assert "updated_fields" in gql_fields, "Auto-injected updated_fields missing"

    # Verify field types
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].field_type == str | None
    # errors type check (list[Error] | None)
    assert gql_fields["id"].field_type == str | None
    assert gql_fields["updated_fields"].field_type == list[str] | None
```

### Test 2: `@failure` Adds Fields to `__gql_fields__`

```python
from fraiseql.mutations.decorators import failure


def test_failure_decorator_adds_fields_to_gql_fields():
    """Test that @failure decorator adds status, message, errors to __gql_fields__."""

    @failure
    class CreateMachineError:
        error_code: str

    gql_fields = getattr(CreateMachineError, "__gql_fields__", {})

    assert "error_code" in gql_fields
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    # No entity field, so id should NOT be added
    assert "id" not in gql_fields or gql_fields.get("id") is None
```

### Test 3: User-Defined Fields Not Overridden

```python
from fraiseql.fields import fraise_field


def test_user_defined_fields_not_overridden():
    """Test that user-defined status/message/errors are not overridden."""

    @success
    class CreateMachineSuccess:
        machine: Machine
        status: str = "custom_success"
        message: str = fraise_field(description="Custom message")

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    # User-defined fields should be preserved
    assert gql_fields["status"].field_type == str
    assert gql_fields["message"].description == "Custom message"
    # Errors should still be auto-injected
    assert "errors" in gql_fields
```

### Test 4: No Entity Field → No `id` Field

```python
def test_no_entity_field_no_id():
    """Test that id is not added when no entity field present."""

    @success
    class DeleteSuccess:
        """Delete confirmation without entity."""
        pass

    gql_fields = getattr(DeleteSuccess, "__gql_fields__", {})

    # Standard fields should be present
    assert "status" in gql_fields
    assert "message" in gql_fields
    assert "errors" in gql_fields
    assert "updated_fields" in gql_fields

    # But id should NOT be present (no entity field)
    assert "id" not in gql_fields
```

### Test 5: Field Descriptions Set Correctly

```python
def test_auto_field_descriptions():
    """Test that auto-injected fields have proper descriptions."""

    @success
    class CreateMachineSuccess:
        machine: Machine

    gql_fields = getattr(CreateMachineSuccess, "__gql_fields__", {})

    assert "status" in gql_fields["status"].description.lower()
    assert "message" in gql_fields["message"].description.lower()
    assert "error" in gql_fields["errors"].description.lower()
```

---

## 🔍 Integration Tests: Schema Generation

**File**: `tests/integration/test_mutation_schema_introspection.py`

### Test 1: GraphQL Schema Includes Auto-Populated Fields

```python
import pytest
from graphql import GraphQLSchema, graphql_sync
from fraiseql import FraiseQL


@pytest.mark.asyncio
async def test_schema_includes_auto_populated_fields(db_pool):
    """Test that GraphQL schema includes auto-populated fields."""

    # Define types
    @fraise_type(sql_source="machines")
    class Machine:
        id: str
        name: str

    @success
    class CreateMachineSuccess:
        machine: Machine

    @mutation
    async def create_machine(info, input: CreateMachineInput) -> CreateMachineSuccess:
        # ... implementation
        pass

    # Build schema
    fraiseql = FraiseQL(pool=db_pool)
    schema = fraiseql.build_schema()

    # Introspection query to get type fields
    introspection_query = """
        query {
            __type(name: "CreateMachineSuccess") {
                fields {
                    name
                    type {
                        name
                        kind
                        ofType {
                            name
                            kind
                        }
                    }
                }
            }
        }
    """

    result = graphql_sync(schema, introspection_query)

    assert result.errors is None, f"Introspection errors: {result.errors}"

    fields = {f["name"] for f in result.data["__type"]["fields"]}

    # Verify all expected fields present
    assert "machine" in fields
    assert "status" in fields, "status field missing from schema"
    assert "message" in fields, "message field missing from schema"
    assert "errors" in fields, "errors field missing from schema"
    assert "id" in fields, "id field missing from schema"
    assert "updatedFields" in fields, "updatedFields missing (should be camelCase)"
```

### Test 2: Field Types Correct in Schema

```python
@pytest.mark.asyncio
async def test_auto_populated_field_types(db_pool):
    """Test that auto-populated fields have correct GraphQL types."""

    @fraise_type(sql_source="machines")
    class Machine:
        id: str

    @success
    class CreateMachineSuccess:
        machine: Machine

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
    fields = {f["name"]: f["type"] for f in result.data["__type"]["fields"]}

    # status: String! (NON_NULL)
    assert fields["status"]["kind"] == "NON_NULL"
    assert fields["status"]["ofType"]["name"] == "String"

    # message: String (nullable)
    assert fields["message"]["kind"] == "SCALAR"
    assert fields["message"]["name"] == "String"

    # errors: [Error!] (nullable list of non-null Error)
    assert fields["errors"]["kind"] == "LIST"
    # ... detailed type checking
```

---

## 🚀 End-to-End Tests: Query Execution

**File**: `tests/integration/test_mutation_field_queries.py`

### Test 1: Can Query Auto-Populated Fields

```python
import pytest


@pytest.mark.asyncio
async def test_query_auto_populated_fields(graphql_client, db_pool):
    """Test that auto-populated fields can be queried."""

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
            "type": "CNC"
        }
    }

    result = await graphql_client.execute(query, variables)

    # Should not have errors
    assert result.get("errors") is None, f"Query errors: {result.get('errors')}"

    # Response should include all queried fields
    data = result["data"]["createMachine"]
    assert "status" in data, "status field not in response"
    assert "message" in data, "message field not in response"
    assert "errors" in data, "errors field not in response"
    assert "id" in data, "id field not in response"
    assert "updatedFields" in data, "updatedFields field not in response"
    assert "machine" in data, "machine field not in response"

    # Verify field values
    assert data["status"] == "success"
    assert isinstance(data["errors"], list)
    assert len(data["errors"]) == 0  # Success response has no errors
```

### Test 2: Fields Optional in Query (Don't Have to Select)

```python
@pytest.mark.asyncio
async def test_auto_fields_optional_in_query(graphql_client, db_pool):
    """Test that auto-populated fields are optional to query."""

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

    variables = {"input": {"name": "Test Machine", "type": "CNC"}}

    result = await graphql_client.execute(query, variables)

    assert result.get("errors") is None
    data = result["data"]["createMachine"]

    # Only requested field should be present
    assert "machine" in data
    # Auto-populated fields should NOT be in response (proper GraphQL behavior)
    assert "status" not in data
    assert "message" not in data
    assert "id" not in data
```

### Test 3: Minimal Query Returns Only Requested Fields

```python
@pytest.mark.asyncio
async def test_minimal_query_no_extra_fields(graphql_client, db_pool):
    """Test that minimal queries don't return unrequested fields (GraphQL spec compliance)."""

    query = """
        mutation CreateMachine($input: CreateMachineInput!) {
            createMachine(input: $input) {
                ... on CreateMachineSuccess {
                    __typename
                }
            }
        }
    """

    variables = {"input": {"name": "Test", "type": "CNC"}}
    result = await graphql_client.execute(query, variables)

    data = result["data"]["createMachine"]

    # Only __typename should be present
    assert "__typename" in data
    assert "status" not in data, "status should not be in response (not requested)"
    assert "machine" not in data, "machine should not be in response (not requested)"
```

---

## 🔧 Regression Tests: Existing Functionality

### Test 1: Existing FraiseQL Tests Pass

```bash
# Run full test suite
pytest tests/unit/ tests/integration/ -v

# Specifically check mutation tests
pytest tests/unit/mutations/ -v
pytest tests/integration/test_mutations*.py -v
```

### Test 2: PrintOptim Backend Tests Pass

```bash
# In printoptim_backend directory
pytest tests/api/test_mutations.py -v

# Should see 138 previously failing tests now pass
```

---

## 📊 Test Coverage Requirements

### Minimum Coverage Targets

- **Decorator code**: 100% coverage (critical path)
- **Schema generation**: 95% coverage
- **Integration tests**: All success/failure paths

### Coverage Commands

```bash
# Generate coverage report
pytest --cov=src/fraiseql/mutations/decorators \
       --cov=src/fraiseql/core/graphql_type \
       --cov-report=html \
       tests/

# View report
open htmlcov/index.html
```

---

## 🐛 Edge Case Test Matrix

| Scenario | Expected Behavior | Test |
|----------|------------------|------|
| No entity field | `id` not added | ✅ test_no_entity_field_no_id |
| User defines `status` | User's field preserved | ✅ test_user_defined_fields_not_overridden |
| User defines `message` with `field()` | User's field preserved | ✅ test_user_defined_fields_not_overridden |
| Multiple entity fields | `id` added | ✅ test_success_decorator_adds_fields_to_gql_fields |
| Failure type | No `id` by default | ✅ test_failure_decorator_adds_fields_to_gql_fields |
| Empty success class | All auto-fields added except `id` | ✅ test_no_entity_field_no_id |
| Query only `status` | Only `status` in response | ✅ test_minimal_query_no_extra_fields |
| Query all fields | All fields in response | ✅ test_query_auto_populated_fields |
| Query no auto-fields | No auto-fields in response | ✅ test_auto_fields_optional_in_query |

---

## ✅ Test Execution Order

1. **Unit tests first** - Validate decorator behavior
   ```bash
   pytest tests/unit/mutations/test_auto_populate_schema_fields.py -v
   ```

2. **Integration tests** - Validate schema generation
   ```bash
   pytest tests/integration/test_mutation_schema_introspection.py -v
   ```

3. **End-to-end tests** - Validate query execution
   ```bash
   pytest tests/integration/test_mutation_field_queries.py -v
   ```

4. **Regression tests** - Ensure nothing broke
   ```bash
   pytest tests/ -v
   ```

5. **External validation** - PrintOptim tests
   ```bash
   cd ~/code/printoptim_backend
   pytest tests/api/test_mutations.py -v
   ```

---

## 🎯 Acceptance Criteria

- [ ] All unit tests pass (100% coverage on decorator)
- [ ] All integration tests pass
- [ ] All regression tests pass (existing FraiseQL tests)
- [ ] PrintOptim 138 tests pass
- [ ] GraphQL introspection shows all fields
- [ ] Fields are queryable without errors
- [ ] Fields are optional in queries (GraphQL spec compliant)
- [ ] No extra fields in response when not requested

---

**Next**: [Phase 4: Migration Guide](./phase-4-migration.md)
