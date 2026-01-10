# FraiseQL v1.8.0: Quick Reference Card

**Validation as Error Type - Cheat Sheet**

---

## 🎯 Core Changes

| Aspect | v1.7.x (OLD) | v1.8.0 (NEW) |
|--------|--------------|--------------|
| **Validation failures** | Success type | Error type ✅ |
| **Success entity** | `Machine \| None` | `Machine` (non-null) ✅ |
| **Error code field** | None | `code: int` ✅ |
| **Return type** | Single type | Union type ✅ |
| **HTTP status** | 200 OK | 200 OK (unchanged) |

---

## 📋 Status Code Mapping

| Status Pattern | Type | Code | HTTP | Meaning |
|---------------|------|------|------|---------|
| `created` | Success | - | 200 | Created successfully |
| `updated` | Success | - | 200 | Updated successfully |
| `deleted` | Success | - | 200 | Deleted successfully |
| `noop:*` | **Error** | **422** | 200 | Validation/business rule |
| `not_found:*` | Error | 404 | 200 | Resource missing |
| `unauthorized:*` | Error | 401 | 200 | Auth failure |
| `forbidden:*` | Error | 403 | 200 | Permission denied |
| `conflict:*` | Error | 409 | 200 | Resource conflict |
| `timeout:*` | Error | 408 | 200 | Operation timeout |
| `failed:*` | Error | 500 | 200 | System failure |

---

## 🔧 Code Changes

### Python: Success Type

```python
# OLD ❌
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # Nullable
    message: str
    cascade: Cascade | None = None

# NEW ✅
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # Non-nullable!
    cascade: Cascade | None = None
```

### Python: Error Type

```python
# OLD ❌
@fraiseql.failure
class CreateMachineError:
    message: str
    errors: list[Error] | None = None

# NEW ✅
@fraiseql.failure
class CreateMachineError:
    code: int          # NEW: REST-like code
    status: str        # NEW: Domain status
    message: str       # Human-readable
    cascade: Cascade | None = None
```

### GraphQL: Query

```graphql
# OLD ❌
mutation CreateMachine($input: CreateMachineInput!) {
  createMachine(input: $input) {
    machine { id }
    message
    cascade { status }
  }
}

# NEW ✅
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

### TypeScript: Client Code

```typescript
// OLD ❌
const result = data.createMachine;
if (result.machine !== null) {
  handleSuccess(result.machine);
} else if (result.cascade?.status?.startsWith("noop:")) {
  handleValidationError(result.cascade);
}

// NEW ✅
const result = data.createMachine;
switch (result.__typename) {
  case "CreateMachineSuccess":
    handleSuccess(result.machine);  // machine guaranteed non-null
    break;
  case "CreateMachineError":
    switch (result.code) {
      case 422: handleValidation(result.status, result.message); break;
      case 404: handleNotFound(result.status); break;
      case 409: handleConflict(result.status); break;
      default: handleError(result);
    }
    break;
}
```

### Python: Test Assertions

```python
# OLD ❌
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"] is None
assert result["cascade"]["status"] == "noop:invalid_contract_id"

# NEW ✅
assert result["__typename"] == "CreateMachineError"
assert result["code"] == 422
assert result["status"] == "noop:invalid_contract_id"
assert result["message"] is not None
```

---

## 📦 GraphQL Schema

### Before (v1.7.x)

```graphql
type CreateMachineSuccess {
  machine: Machine    # Nullable ❌
  message: String!
  cascade: Cascade
}

type CreateMachineError {
  message: String!
  errors: [Error!]!
}

extend type Mutation {
  createMachine(input: CreateMachineInput!): CreateMachineSuccess!
}
```

### After (v1.8.0)

```graphql
union CreateMachineResult = CreateMachineSuccess | CreateMachineError

type CreateMachineSuccess {
  machine: Machine!   # Non-nullable ✅
  cascade: Cascade
}

type CreateMachineError {
  code: Int!          # NEW ✅
  status: String!
  message: String!
  cascade: Cascade
}

extend type Mutation {
  createMachine(input: CreateMachineInput!): CreateMachineResult!
}
```

---

## 🔍 Response Examples

### Validation Error

```json
// OLD (v1.7.x) ❌
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineSuccess",
      "machine": null,
      "message": "Contract not found",
      "cascade": {"status": "noop:invalid_contract_id"}
    }
  }
}

// NEW (v1.8.0) ✅
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineError",
      "code": 422,
      "status": "noop:invalid_contract_id",
      "message": "Contract not found or access denied",
      "cascade": {"status": "noop:invalid_contract_id", "reason": "contract_does_not_exist"}
    }
  }
}
```

### Not Found Error

```json
// NEW (v1.8.0) ✅
{
  "data": {
    "deleteMachine": {
      "__typename": "DeleteMachineError",
      "code": 404,
      "status": "not_found:machine",
      "message": "Machine not found"
    }
  }
}
```

### Success

```json
// NEW (v1.8.0) ✅
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineSuccess",
      "machine": {  // Always non-null ✅
        "id": "123",
        "serialNumber": "MACHINE-001",
        "model": { "id": "model-1", "name": "Model X" }
      },
      "cascade": {
        "status": "created",
        "updated": [
          {"__typename": "Machine", "id": "123"},
          {"__typename": "User", "id": "user-1"}
        ]
      }
    }
  }
}
```

---

## 📝 Migration Checklist

### Code Updates
- [ ] Remove `| None` from Success type entity fields
- [ ] Add `code: int` field to Error types
- [ ] Add `status: str` field to Error types
- [ ] Update mutation return types to unions

### Test Updates
- [ ] Change assertions from `CreateMachineSuccess` to `CreateMachineError` for validation
- [ ] Assert `code == 422` for validation errors
- [ ] Assert `code == 404` for not found errors
- [ ] Assert `code == 409` for conflict errors
- [ ] Remove assertions for `machine is None`

### GraphQL Updates
- [ ] Add `__typename` to all mutation queries
- [ ] Add fragments for Success and Error types
- [ ] Handle union types in client code

### Frontend Updates
- [ ] Update error handling to check `code` field
- [ ] Update success handling (entity always exists)
- [ ] Update TypeScript types (if codegen used)

---

## 🚨 Common Pitfalls

### Pitfall 1: Forgot to Make Entity Non-Nullable

```python
# ❌ WRONG - Still nullable
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None

# ✅ CORRECT
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine
```

### Pitfall 2: Missing Code Field

```python
# ❌ WRONG - Missing code
@fraiseql.failure
class CreateMachineError:
    status: str
    message: str

# ✅ CORRECT
@fraiseql.failure
class CreateMachineError:
    code: int  # Required!
    status: str
    message: str
```

### Pitfall 3: Forgot Union Type Fragment

```graphql
# ❌ WRONG - Direct field access on union
mutation {
  createMachine(input: $input) {
    machine { id }  # Error: no "machine" on union
  }
}

# ✅ CORRECT - Use fragments
mutation {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess { machine { id } }
    ... on CreateMachineError { code message }
  }
}
```

### Pitfall 4: Still Checking for Null Entity

```typescript
// ❌ WRONG - No longer needed
if (result.__typename === "CreateMachineSuccess") {
  if (result.machine !== null) {  // Unnecessary check
    handleSuccess(result.machine);
  }
}

// ✅ CORRECT - Entity always exists in Success
if (result.__typename === "CreateMachineSuccess") {
  handleSuccess(result.machine);  // No null check needed
}
```

---

## 🎓 Key Principles

1. **Success = Has Entity**
   - Success type ALWAYS has non-null entity
   - No exceptions, no null checks needed

2. **Error = No Entity (but has code)**
   - Error type has `code`, `status`, `message`
   - Use `code` for categorization (422, 404, 409, 500)
   - Use `status` for details (`noop:invalid_contract_id`)

3. **HTTP Always 200**
   - GraphQL convention: HTTP 200 OK for all responses
   - `code` field is application-level only (not HTTP status)

4. **Validation = Error (422)**
   - `noop:*` statuses return Error type with code 422
   - Not Success type with null entity (that was wrong)

---

## 📚 Resources

- **Migration Guide:** `docs/migrations/v1.8.0.md`
- **Phase Plans:** `.phases/validation-as-error-v1.8.0/`
- **Status Strings:** `docs/mutations/status-strings.md`
- **Tim's Feedback:** `/tmp/fraiseql_tim_feedback_analysis.md`

---

**Questions?** See the [full implementation plan](./README.md) or [migration guide](./04_PHASE_4_TESTING_DOCS.md#section-44-migration-guide).
