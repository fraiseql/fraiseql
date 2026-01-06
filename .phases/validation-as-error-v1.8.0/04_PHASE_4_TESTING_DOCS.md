# Phase 4: Testing & Documentation

**Timeline:** Week 2, Days 4-5
**Risk Level:** LOW (documentation and test updates)
**Dependencies:** Phases 1-3
**Blocking:** Phase 5 (release)

---

## Objective

1. Update ALL FraiseQL tests to reflect v1.8.0 behavior
2. Create comprehensive migration guide
3. Update all documentation
4. Add deprecation warnings
5. Create code examples

---

## Testing Updates

### Section 4.1: Integration Tests

**File:** `tests/integration/test_graphql_cascade.py`

**Update all CASCADE tests for new error behavior:**

```python
import pytest
from tests.conftest import execute_graphql

class TestCascadeV190:
    """Test CASCADE with v1.8.0 error handling."""

    def test_cascade_validation_error_returns_error_type(self, graphql_schema):
        """Validation failures return Error type with CASCADE."""
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    __typename
                    ... on CreatePostSuccess {
                        post { id title }
                        cascade { status }
                    }
                    ... on CreatePostError {
                        code
                        status
                        message
                        cascade { status reason }
                    }
                }
            }
        """

        result = execute_graphql(mutation, {
            "input": {
                "categoryId": "00000000-0000-0000-0000-000000000000",  # Invalid
                "title": "Test Post"
            }
        })

        data = result["data"]["createPost"]

        # v1.8.0: Returns Error type
        assert data["__typename"] == "CreatePostError"
        assert data["code"] == 422
        assert data["status"] == "noop:invalid_category_id"
        assert "category" in data["message"].lower()

        # CASCADE still works with Error type
        assert data["cascade"] is not None
        assert data["cascade"]["status"] == "noop:invalid_category_id"

    def test_cascade_success_always_has_entity(self, graphql_schema):
        """Success type always has non-null entity with CASCADE."""
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    __typename
                    ... on CreatePostSuccess {
                        post { id title }
                        cascade { status updated { __typename id } }
                    }
                }
            }
        """

        result = execute_graphql(mutation, {
            "input": {
                "categoryId": "valid-category-id",
                "title": "Test Post"
            }
        })

        data = result["data"]["createPost"]

        assert data["__typename"] == "CreatePostSuccess"
        assert data["post"] is not None  # ✅ Never null
        assert data["post"]["id"] is not None
        assert data["cascade"] is not None

    def test_cascade_entity_fields_without_querying_cascade(self, graphql_schema):
        """Can query entity without querying CASCADE."""
        mutation = """
            mutation CreatePost($input: CreatePostInput!) {
                createPost(input: $input) {
                    __typename
                    ... on CreatePostSuccess {
                        post { id title }
                        # Not querying cascade
                    }
                }
            }
        """

        result = execute_graphql(mutation, {
            "input": {
                "categoryId": "valid-category-id",
                "title": "Test Post"
            }
        })

        data = result["data"]["createPost"]

        assert data["__typename"] == "CreatePostSuccess"
        assert data["post"] is not None
        assert "cascade" not in data  # Not queried, not returned

    def test_error_with_cascade_metadata(self, graphql_schema):
        """Error type can include CASCADE metadata."""
        mutation = """
            mutation DeletePost($id: ID!) {
                deletePost(id: $id) {
                    __typename
                    ... on DeletePostError {
                        code
                        status
                        message
                        cascade { status reason trigger }
                    }
                }
            }
        """

        result = execute_graphql(mutation, {"id": "nonexistent-id"})

        data = result["data"]["deletePost"]

        assert data["__typename"] == "DeletePostError"
        assert data["code"] == 404
        assert data["status"] == "not_found:post"
        assert data["cascade"] is not None
```

---

### Section 4.2: Mutation Pipeline Tests

**File:** `tests/integration/graphql/mutations/test_mutation_error_handling.py`

```python
import pytest

class TestMutationErrorHandlingV190:
    """Test mutation error handling in v1.8.0."""

    def test_noop_returns_error_type_with_422(self):
        """noop:* statuses return Error type with code 422."""
        result = execute_mutation(
            "createMachine",
            input={"contractId": "invalid-id", "modelId": "valid-model"}
        )

        assert result["__typename"] == "CreateMachineError"
        assert result["code"] == 422
        assert result["status"].startswith("noop:")
        assert result["message"] is not None

    def test_not_found_returns_error_type_with_404(self):
        """not_found:* statuses return Error type with code 404."""
        result = execute_mutation(
            "deleteMachine",
            id="nonexistent-machine-id"
        )

        assert result["__typename"] == "DeleteMachineError"
        assert result["code"] == 404
        assert result["status"] == "not_found:machine"

    def test_conflict_returns_error_type_with_409(self):
        """conflict:* statuses return Error type with code 409."""
        # Create machine
        machine1 = execute_mutation("createMachine", input={
            "serialNumber": "DUPLICATE-123",
            "modelId": "valid-model"
        })
        assert machine1["__typename"] == "CreateMachineSuccess"

        # Try to create duplicate
        machine2 = execute_mutation("createMachine", input={
            "serialNumber": "DUPLICATE-123",  # Same serial
            "modelId": "valid-model"
        })

        assert machine2["__typename"] == "CreateMachineError"
        assert machine2["code"] == 409
        assert machine2["status"] == "conflict:duplicate_serial_number"

    def test_success_always_has_entity(self):
        """Success type always has non-null entity."""
        result = execute_mutation("createMachine", input={
            "serialNumber": "VALID-123",
            "modelId": "valid-model",
            "contractId": "valid-contract"
        })

        assert result["__typename"] == "CreateMachineSuccess"
        assert result["machine"] is not None
        assert result["machine"]["id"] is not None
        assert result["machine"]["serialNumber"] == "VALID-123"

    def test_http_always_200(self):
        """HTTP status is always 200 OK (even for errors)."""
        import httpx

        response = httpx.post("/graphql", json={
            "query": """
                mutation { createMachine(input: {contractId: "invalid"}) {
                    __typename
                    ... on CreateMachineError { code status }
                }}
            """
        })

        # HTTP level: always 200
        assert response.status_code == 200

        # Application level: code field indicates error type
        data = response.json()["data"]["createMachine"]
        assert data["__typename"] == "CreateMachineError"
        assert data["code"] == 422  # Application-level code
```

---

### Section 4.3: Backward Compatibility Tests

**File:** `tests/integration/graphql/mutations/test_backward_compatibility.py`

```python
import pytest
import warnings

class TestBackwardCompatibilityV190:
    """Test backward compatibility and deprecation warnings."""

    def test_error_as_data_prefixes_deprecated(self):
        """error_as_data_prefixes is deprecated."""
        from fraiseql.mutations import MutationErrorConfig

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")

            # Try to use old error_as_data_prefixes
            config = MutationErrorConfig(
                error_as_data_prefixes={"noop:", "blocked:"}
            )

            # Should have deprecation warning
            assert len(w) > 0
            assert issubclass(w[0].category, DeprecationWarning)
            assert "error_as_data_prefixes" in str(w[0].message)

    def test_always_return_as_data_deprecated(self):
        """always_return_as_data is deprecated."""
        from fraiseql.mutations import MutationErrorConfig

        with warnings.catch_warnings(record=True) as w:
            warnings.simplefilter("always")

            config = MutationErrorConfig(always_return_as_data=True)
            config.is_error_status("noop:test")

            # Should warn about deprecated flag
            assert len(w) > 0
            assert "always_return_as_data" in str(w[0].message)

    def test_strict_status_config_still_works(self):
        """STRICT_STATUS_CONFIG still works (now same as DEFAULT)."""
        from fraiseql.mutations import STRICT_STATUS_CONFIG

        # Should not raise error
        assert STRICT_STATUS_CONFIG is not None
        assert STRICT_STATUS_CONFIG.is_error_status("noop:test") is True
```

---

## Documentation Updates

### Section 4.4: Migration Guide

**File:** `docs/migrations/v1.8.0.md`

```markdown
# Migration Guide: FraiseQL v1.7.x → v1.8.0

**Date:** 2024-12-06
**Breaking Changes:** Yes
**Upgrade Difficulty:** Medium (requires code changes)

---

## Overview

FraiseQL v1.8.0 implements a major architectural improvement to mutation error handling, following recommendations from Tim Berners-Lee's architectural review.

**Key Changes:**
- Validation failures now return **Error type** (not Success with null entity)
- Error type includes **`code` field** (422, 404, 409, 500)
- Success type entity is **always non-null**
- All mutations return **union types**

---

## What Changed

### Before (v1.7.x) - DEPRECATED

```python
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌ Nullable
    message: str
    cascade: Cascade | None = None

# GraphQL returned:
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineSuccess",  # ❌ "Success" for failure
      "machine": null,                       # ❌ Null entity
      "cascade": {"status": "noop:invalid_contract_id"}
    }
  }
}
```

### After (v1.8.0) - CORRECT

```python
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable
    cascade: Cascade | None = None

@fraiseql.failure
class CreateMachineError:
    code: int         # ✅ NEW: REST-like code
    status: str
    message: str
    cascade: Cascade | None = None

# GraphQL returns:
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineError",  # ✅ Error type
      "code": 422,                         # ✅ Validation error
      "status": "noop:invalid_contract_id",
      "message": "Contract not found"
    }
  }
}
```

---

## Migration Steps

### Step 1: Update Success Types

**Make entity fields non-nullable:**

```python
# OLD (v1.7.x)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌ Remove | None

# NEW (v1.8.0)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Non-nullable
```

### Step 2: Update Error Types

**Add `code` field:**

```python
# OLD (v1.7.x)
@fraiseql.failure
class CreateMachineError:
    message: str
    errors: list[Error] | None = None

# NEW (v1.8.0)
@fraiseql.failure
class CreateMachineError:
    code: int          # ✅ Add this field
    status: str        # ✅ Add domain status
    message: str
    cascade: Cascade | None = None  # Optional
```

### Step 3: Update Test Assertions

**Change expected typename for validation failures:**

```python
# OLD (v1.7.x)
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"] is None
assert result["cascade"]["status"] == "noop:invalid_contract_id"

# NEW (v1.8.0)
assert result["__typename"] == "CreateMachineError"
assert result["code"] == 422
assert result["status"] == "noop:invalid_contract_id"
assert result["message"] is not None
```

### Step 4: Update GraphQL Fragments

**Handle union types:**

```graphql
# OLD (v1.7.x)
mutation CreateMachine($input: CreateMachineInput!) {
  createMachine(input: $input) {
    machine { id }
    message
    cascade { status }
  }
}

# NEW (v1.8.0)
mutation CreateMachine($input: CreateMachineInput!) {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess {
      machine { id }
      cascade { status }
    }
    ... on CreateMachineError {
      code
      status
      message
      cascade { status reason }
    }
  }
}
```

### Step 5: Update Client Code

**Handle Error type:**

```typescript
// OLD (v1.7.x)
const result = data.createMachine;
if (result.machine !== null) {
  handleSuccess(result.machine);
} else {
  handleValidationError(result.cascade);
}

// NEW (v1.8.0)
const result = data.createMachine;
switch (result.__typename) {
  case "CreateMachineSuccess":
    handleSuccess(result.machine);  // machine always exists
    break;
  case "CreateMachineError":
    switch (result.code) {
      case 422:
        handleValidation(result.status, result.message);
        break;
      case 404:
        handleNotFound(result.status);
        break;
      default:
        handleError(result);
    }
    break;
}
```

---

## Status Code Mapping

| Status Pattern | GraphQL Type | Code | HTTP | Meaning |
|---------------|--------------|------|------|---------|
| `created`, `updated` | Success | - | 200 | Operation succeeded |
| `noop:*` | Error | 422 | 200 | Validation/business rule |
| `not_found:*` | Error | 404 | 200 | Resource missing |
| `unauthorized:*` | Error | 401 | 200 | Authentication failure |
| `forbidden:*` | Error | 403 | 200 | Permission denied |
| `conflict:*` | Error | 409 | 200 | Resource conflict |
| `timeout:*` | Error | 408 | 200 | Operation timeout |
| `failed:*` | Error | 500 | 200 | System failure |

**Note:** HTTP is always 200 OK (GraphQL convention). The `code` field is application-level only.

---

## Affected Tests

### Validation Error Tests

All tests asserting `CreateXxxSuccess` with `entity: null` need updates:

```python
# Examples needing updates:
- test_create_machine_with_invalid_contract
- test_create_machine_with_invalid_model
- test_delete_machine_not_found
- test_update_entity_not_found
- test_create_price_dates_outside_contract_period
```

**Find affected tests:**
```bash
grep -r "assert.*Success" tests/ | grep "machine.*None"
grep -r "assert.*Success" tests/ | grep "cascade.*noop"
```

---

## Common Issues

### Issue 1: "Success type requires non-null entity"

**Error:**
```
ValueError: Success type 'CreateMachineSuccess' requires non-null entity.
Status 'noop:invalid_contract_id' returned null entity.
```

**Cause:** Database function returned `noop:*` status.

**Fix:** Database should return validation errors, not Success. But v1.8.0 Rust layer now handles this automatically by returning Error type.

### Issue 2: "Error type missing required 'code' field"

**Error:**
```
ValueError: Error type 'CreateMachineError' must have 'code: int' field.
```

**Fix:** Add `code: int` field to Error type:

```python
@fraiseql.failure
class CreateMachineError:
    code: int  # ← Add this
    status: str
    message: str
```

### Issue 3: GraphQL Query Fails on Union Type

**Error:**
```
GraphQL error: Cannot query field "machine" on type "CreateMachineResult"
```

**Fix:** Use fragments for union types:

```graphql
mutation {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess { machine { id } }
    ... on CreateMachineError { code message }
  }
}
```

---

## Checklist

### Code Changes
- [ ] Update all Success types (remove nullable entities)
- [ ] Update all Error types (add `code` field)
- [ ] Update test assertions (check Error type for validation)
- [ ] Update GraphQL fragments (handle union types)
- [ ] Update client error handling

### Testing
- [ ] All validation error tests pass
- [ ] All success tests pass
- [ ] No null entities in Success type
- [ ] All Error types have `code` field

### Verification
- [ ] `pytest tests/` passes
- [ ] `mypy src/` passes
- [ ] GraphQL schema validates
- [ ] Client builds without errors

---

## Support

Questions? Check:
- [FraiseQL v1.8.0 Documentation](https://fraiseql.io/docs/v1.8.0)
- [GitHub Discussions](https://github.com/fraiseql/fraiseql/discussions)
- [Migration Examples](https://github.com/fraiseql/fraiseql/tree/main/examples/v1.8.0-migration)

---

## Timeline

- **v1.8.0-beta.1:** Released 2024-12-XX (beta period: 1 week)
- **v1.8.0:** Final release 2024-12-XX
- **v1.7.x:** Security fixes only (deprecated 2025-03-XX)
```

---

### Section 4.5: Update Status Strings Documentation

**File:** `docs/mutations/status-strings.md`

**Update section on noop:**

```markdown
### 3. Noop Prefix (Validation/Business Rule Failures)

⚠️ **v1.8.0 BREAKING CHANGE:** `noop:*` statuses now return **Error type** (not Success).

Indicates validation failure or business rule rejection. Maps to Error type with code 422.

| Prefix | GraphQL Type | Code | Meaning |
|--------|--------------|------|---------|
| `noop:` | Error | 422 | Validation or business rule failure |

**Common noop reasons:**
- `noop:invalid_contract_id` - Foreign key validation failed
- `noop:dates_outside_contract` - Business rule violation
- `noop:unchanged` - No fields changed (idempotent operation)
- `noop:duplicate` - Entity already exists

**Example:**
```sql
IF NOT FOUND THEN
    RETURN ('noop:invalid_contract_id', 'Contract not found', ...)::mutation_response;
END IF;
```

**GraphQL Response (v1.8.0):**
```json
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineError",  ← Error type
      "code": 422,                         ← Unprocessable Entity
      "status": "noop:invalid_contract_id",
      "message": "Contract not found"
    }
  }
}
```

**Migration from v1.7.x:**

OLD (v1.7.x):
```json
{
  "__typename": "CreateMachineSuccess",  ❌ Wrong
  "machine": null,                       ❌ Null entity
  "cascade": {"status": "noop:..."}
}
```

NEW (v1.8.0):
```json
{
  "__typename": "CreateMachineError",    ✅ Correct
  "code": 422,
  "status": "noop:...",
  "message": "..."
}
```
```

---

## Verification Checklist

### Testing
- [ ] All integration tests updated
- [ ] All unit tests updated
- [ ] CASCADE tests updated
- [ ] Backward compatibility tests added
- [ ] All tests pass

### Documentation
- [ ] Migration guide complete
- [ ] Status strings doc updated
- [ ] CASCADE docs updated
- [ ] API reference updated
- [ ] Code examples added

### Examples
- [ ] Before/after client code examples
- [ ] GraphQL query examples
- [ ] Python decorator examples
- [ ] Test assertion examples

---

## Next Steps

Once Phase 4 is complete:
1. Review all documentation for clarity
2. Get docs peer-reviewed
3. Commit changes: `git commit -m "docs!: v1.8.0 migration guide and test updates"`
4. Proceed to Phase 5: Verification & Release

**Blocking:** Release (Phase 5) depends on complete documentation and passing tests.
