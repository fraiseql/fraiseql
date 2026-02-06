# PrintOptim Migration Notes - FraiseQL v1.8.0-beta.1

## Quick Summary

- FraiseQL v1.8.0-beta.1 changes how validation errors work
- `noop:*` now returns Error type (not Success with null entity)
- Error type has `code` field (422, 404, 409, 500)
- Success type entity is ALWAYS non-null

---

## What to Update in PrintOptim

### 1. Success Types (~5-10 types to update)

**Pattern:** Remove `| None` from entity fields

```python
# Before (v1.7.x)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine | None = None  # ❌ Remove this

# After (v1.8.0)
@fraiseql.success
class CreateMachineSuccess:
    machine: Machine  # ✅ Always non-null
    cascade: Cascade | None = None  # CASCADE can still be None
```

**Files to check:**

- `printoptim_backend/mutations/machine/types.py`
- `printoptim_backend/mutations/contract/types.py`
- `printoptim_backend/mutations/user/types.py`
- etc.

---

### 2. Error Types (~5-10 types to update)

**Pattern:** Add `code: int` and `status: str` fields

```python
# Before (v1.7.x)
@fraiseql.failure
class CreateMachineError:
    message: str
    errors: list[Error] | None = None

# After (v1.8.0)
@fraiseql.failure
class CreateMachineError:
    code: int          # ✅ ADD - REST-like code (422, 404, etc.)
    status: str        # ✅ ADD - Domain status (noop:invalid_contract_id)
    message: str       # Keep
    cascade: Cascade | None = None  # Keep if you have CASCADE
```

**Files to check:**

- Same files as Success types
- Search for `@fraiseql.failure`

---

### 3. Test Assertions (~30-50 tests to update)

**Pattern:** Change Success → Error for validation failures

```python
# Before (v1.7.x)
result = execute_mutation(create_machine, input={"contractId": "invalid"})
assert result["__typename"] == "CreateMachineSuccess"  # ❌ WRONG
assert result["machine"] is None  # ❌ WRONG
assert result["cascade"]["status"] == "noop:invalid_contract_id"

# After (v1.8.0)
result = execute_mutation(create_machine, input={"contractId": "invalid"})
assert result["__typename"] == "CreateMachineError"  # ✅ CORRECT
assert result["code"] == 422  # ✅ CORRECT
assert result["status"] == "noop:invalid_contract_id"  # ✅ CORRECT
assert result["message"] is not None
```

**Files to check:**

- `printoptim_backend/tests/mutations/test_machine.py`
- `printoptim_backend/tests/mutations/test_contract.py`
- Any test that checks for `machine is None` or similar

**Quick find command:**

```bash
cd /home/lionel/code/printoptim_backend
grep -r "machine.*is None" tests/
grep -r "user.*is None" tests/
grep -r "contract.*is None" tests/
```

---

### 4. GraphQL Queries (Frontend - if any)

**Pattern:** Handle union types with fragments

```graphql
# Before (v1.7.x)
mutation CreateMachine($input: CreateMachineInput!) {
  createMachine(input: $input) {
    machine { id serialNumber }
    message
  }
}

# After (v1.8.0)
mutation CreateMachine($input: CreateMachineInput!) {
  createMachine(input: $input) {
    __typename
    ... on CreateMachineSuccess {
      machine { id serialNumber }
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

**If you have a frontend that uses GraphQL directly**, update all mutation queries.

---

## Migration Steps

### Step 1: Update FraiseQL Dependency

```bash
cd /home/lionel/code/printoptim_backend
# Update pyproject.toml
uv add fraiseql@1.8.0-beta.1  # Or whatever version
```

### Step 2: Run Tests to Find Failures

```bash
uv run pytest tests/ -v
# Expect ~30-50 failures
```

### Step 3: Fix Success Types

```bash
# Find all Success types with nullable entities
grep -r "| None = None" printoptim_backend/mutations/ | grep "@fraiseql.success" -A 5

# Update each one to remove | None from entity field
```

### Step 4: Fix Error Types

```bash
# Find all Error types
grep -r "@fraiseql.failure" printoptim_backend/mutations/

# Add code: int and status: str to each
```

### Step 5: Fix Test Assertions

```bash
# For each failing test:
# 1. Change "CreateMachineSuccess" → "CreateMachineError"
# 2. Change "machine is None" → check for code == 422
# 3. Add assertion for status field
```

### Step 6: Verify

```bash
uv run pytest tests/ -v
# All tests should pass
```

### Step 7: Deploy to Staging

```bash
# Deploy and test manually
# Verify mutations work correctly
```

### Step 8: Deploy to Production

```bash
# Once staging looks good
```

---

## Common Patterns

### Pattern 1: Validation Error (noop:*)

```python
# v1.7.x
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"] is None
assert "noop:" in result["cascade"]["status"]

# v1.8.0
assert result["__typename"] == "CreateMachineError"
assert result["code"] == 422  # Validation error
assert result["status"].startswith("noop:")
assert result["message"] is not None
```

### Pattern 2: Not Found Error

```python
# v1.7.x
assert result["__typename"] == "UpdateMachineError"
assert "not_found" in result["status"]

# v1.8.0
assert result["__typename"] == "UpdateMachineError"
assert result["code"] == 404  # Not found
assert result["status"].startswith("not_found:")
```

### Pattern 3: Success Case

```python
# v1.7.x
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"]["id"] is not None

# v1.8.0
assert result["__typename"] == "CreateMachineSuccess"
assert result["machine"]["id"] is not None  # SAME (no change for success)
```

---

## Estimated Time

**Total: 1-2 days**

- Find/update Success types: 1-2 hours
- Find/update Error types: 1-2 hours
- Fix failing tests: 4-6 hours
- Testing on staging: 2-4 hours
- Buffer for unexpected issues: 2-4 hours

---

## Questions?

- Check the FraiseQL commit: `418237f6`
- Review implementation plans: `.phases/validation-as-error-v1.8.0/`
- Ask Claude for help with specific patterns

---

**Internal use only - no need for public docs or deprecation warnings.**
