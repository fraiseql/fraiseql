# Phase 3: Test Implementation - Auto-Populate Mutation Fields

## Objective

Verify that the Rust changes correctly auto-populate `status` and `errors` fields in success responses without breaking existing functionality.

## TDD Stage

QA (Quality Assurance - verify implementation works correctly)

## Context

**From Phase 2**:
- Modified `build_success_response()` in Rust to add `status` and `errors` fields
- Compiled Rust extension successfully
- Python can import updated extension

**This Phase**:
- Run existing test suite (should all pass)
- Write new tests for auto-populated fields
- Test with real mutation scenarios
- Verify backward compatibility

**Next Phase**:
- Phase 4: Update documentation and create migration guide

## Files to Test

### Existing Test Files (Should Still Pass)
1. `fraiseql_rs/src/mutation/tests/mod.rs` - Rust unit tests
2. `fraiseql_rs/src/mutation/tests/status_tests.rs` - Status parsing tests
3. `fraiseql_rs/src/mutation/tests/error_array_generation.rs` - Error array tests
4. `tests/integration/database/mutations/` - Python integration tests (if exist)

### New Test File to Create
1. `fraiseql_rs/src/mutation/tests/auto_populate_fields_tests.rs` - New test module

## Implementation Steps

### Step 1: Run existing Rust tests

**Commands**:
```bash
cd fraiseql_rs

# Run all tests
cargo test

# Run mutation tests only
cargo test --lib mutation

# Run with output
cargo test -- --nocapture

cd ..
```

**Expected output**:
```
running 23 tests
test mutation::tests::status_tests::test_parse_success ... ok
test mutation::tests::status_tests::test_parse_noop ... ok
test mutation::tests::error_array_generation::test_auto_generate ... ok
...
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured
```

**If tests fail**:
- Review error messages carefully
- Check if field order matters in any assertions
- Verify JSON structure assertions still match
- Most likely culprit: tests checking exact field count or field list

### Step 2: Create new Rust test file

**File**: `fraiseql_rs/src/mutation/tests/auto_populate_fields_tests.rs`

**Content**:
```rust
//! Tests for auto-populated fields in mutation responses

use crate::mutation::{MutationResult, MutationStatus};
use crate::mutation::response_builder::build_success_response;
use serde_json::{json, Value};

#[test]
fn test_success_response_has_status_field() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("Operation completed".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "name": "Test User"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,  // auto_camel_case
        None,  // success_type_fields
        None,  // cascade_selections
    ).expect("Failed to build response");

    // Verify
    let obj = response.as_object().expect("Response should be object");

    // Check status field exists
    assert!(obj.contains_key("status"), "Response missing 'status' field");

    // Check status value
    let status = obj.get("status").expect("status field should exist");
    assert_eq!(status.as_str(), Some("success"), "status should be 'success'");
}

#[test]
fn test_success_response_has_errors_field() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("Operation completed".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "name": "Test User"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    // Verify
    let obj = response.as_object().expect("Response should be object");

    // Check errors field exists
    assert!(obj.contains_key("errors"), "Response missing 'errors' field");

    // Check errors is empty array
    let errors = obj.get("errors").expect("errors field should exist");
    let errors_array = errors.as_array().expect("errors should be array");
    assert_eq!(errors_array.len(), 0, "errors array should be empty for success");
}

#[test]
fn test_success_response_all_standard_fields() {
    // Setup
    let result = MutationResult {
        status: MutationStatus::Success("success:created".to_string()),
        message: Some("User created successfully".to_string()),
        entity_id: Some("123e4567-e89b-12d3-a456-426614174000".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123", "email": "test@example.com"})),
        updated_fields: Some(vec!["email".to_string(), "name".to_string()]),
        cascade: None,
        metadata: None,
    };

    // Execute
    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    // Verify all standard fields present
    let obj = response.as_object().expect("Response should be object");

    assert!(obj.contains_key("__typename"), "Missing __typename");
    assert!(obj.contains_key("id"), "Missing id");
    assert!(obj.contains_key("message"), "Missing message");
    assert!(obj.contains_key("status"), "Missing status");
    assert!(obj.contains_key("errors"), "Missing errors");
    assert!(obj.contains_key("user"), "Missing user entity");
    assert!(obj.contains_key("updatedFields"), "Missing updatedFields");

    // Verify values
    assert_eq!(obj.get("__typename").unwrap().as_str(), Some("CreateUserSuccess"));
    assert_eq!(obj.get("status").unwrap().as_str(), Some("success:created"));
    assert_eq!(obj.get("message").unwrap().as_str(), Some("User created successfully"));

    let errors = obj.get("errors").unwrap().as_array().unwrap();
    assert_eq!(errors.len(), 0, "Success should have empty errors array");
}

#[test]
fn test_success_status_preserves_detail() {
    // Test that status detail is preserved (e.g., "success:created")
    let result = MutationResult {
        status: MutationStatus::Success("success:updated".to_string()),
        message: Some("Updated".to_string()),
        entity_id: Some("abc-123".to_string()),
        entity_type: Some("Post".to_string()),
        entity: Some(json!({"id": "abc-123"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    let response = build_success_response(
        &result,
        "UpdatePostSuccess",
        Some("post"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    let obj = response.as_object().unwrap();
    let status = obj.get("status").unwrap().as_str().unwrap();

    assert_eq!(status, "success:updated", "Status detail should be preserved");
}

#[test]
fn test_success_fields_order() {
    // Verify fields appear in expected order for consistent API
    let result = MutationResult {
        status: MutationStatus::Success("success".to_string()),
        message: Some("OK".to_string()),
        entity_id: Some("123".to_string()),
        entity_type: Some("User".to_string()),
        entity: Some(json!({"id": "123"})),
        updated_fields: None,
        cascade: None,
        metadata: None,
    };

    let response = build_success_response(
        &result,
        "CreateUserSuccess",
        Some("user"),
        true,
        None,
        None,
    ).expect("Failed to build response");

    let obj = response.as_object().unwrap();
    let keys: Vec<&String> = obj.keys().collect();

    // Check that standard fields come before entity field
    let typename_idx = keys.iter().position(|&k| k == "__typename").unwrap();
    let id_idx = keys.iter().position(|&k| k == "id").unwrap();
    let message_idx = keys.iter().position(|&k| k == "message").unwrap();
    let status_idx = keys.iter().position(|&k| k == "status").unwrap();
    let errors_idx = keys.iter().position(|&k| k == "errors").unwrap();
    let user_idx = keys.iter().position(|&k| k == "user").unwrap();

    // Verify ordering
    assert!(typename_idx < id_idx, "__typename should come before id");
    assert!(id_idx < message_idx, "id should come before message");
    assert!(message_idx < status_idx, "message should come before status");
    assert!(status_idx < errors_idx, "status should come before errors");
    assert!(errors_idx < user_idx, "errors should come before entity");
}
```

### Step 3: Register new test module

**File**: `fraiseql_rs/src/mutation/tests/mod.rs`

**Add at the end**:
```rust
mod auto_populate_fields_tests;
```

### Step 4: Run new tests

**Commands**:
```bash
cd fraiseql_rs

# Run only new tests
cargo test auto_populate_fields_tests

# Run with verbose output
cargo test auto_populate_fields_tests -- --nocapture

cd ..
```

**Expected output**:
```
running 6 tests
test mutation::tests::auto_populate_fields_tests::test_success_response_has_status_field ... ok
test mutation::tests::auto_populate_fields_tests::test_success_response_has_errors_field ... ok
test mutation::tests::auto_populate_fields_tests::test_success_response_all_standard_fields ... ok
test mutation::tests::auto_populate_fields_tests::test_success_status_preserves_detail ... ok
test mutation::tests::auto_populate_fields_tests::test_success_fields_order ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured
```

### Step 5: Run full Rust test suite

**Commands**:
```bash
cd fraiseql_rs

# Run ALL tests (old + new)
cargo test

# Check for any failures
echo $?  # Should be 0

cd ..
```

**Expected outcome**: All tests pass ✅

**If any tests fail**:
1. Read error message carefully
2. Check which test failed and why
3. Common issues:
   - Tests checking exact field count (update expected count +2)
   - Tests checking field list (add "status" and "errors" to expected list)
   - Tests with hardcoded JSON (add new fields to expected JSON)

### Step 6: Test with Python integration (if available)

**Find existing mutation tests**:
```bash
find tests -name "*.py" -path "*mutation*" -type f
```

**Run mutation tests**:
```bash
# If integration tests exist
uv run pytest tests/integration/mutations/ -v

# Or run all tests
uv run pytest tests/ -k mutation -v
```

**Expected outcome**:
- Existing tests should pass (backward compatible)
- New fields appear in responses but don't break existing assertions

**If tests fail**:
- Check if tests assert on exact response structure
- Update test expectations to include `status` and `errors` fields

### Step 7: Manual integration test with real database

**Create test file**: `tests/manual/test_auto_populate.py`

```python
"""Manual test for auto-populated mutation fields."""
import asyncio
import fraiseql
from fraiseql.gql.app import create_fraiseql_app
from fraiseql.types.errors import Error


@fraiseql.success
class CreateTestSuccess:
    """Test success type."""
    test_entity: dict  # Just a simple dict for testing
    # status, message, errors should be auto-populated


@fraiseql.failure
class CreateTestError:
    """Test error type."""
    # status, message, errors should be auto-populated
    pass


@fraiseql.mutation(function="app.test_create")
class CreateTest:
    """Test mutation."""
    input: dict
    success: CreateTestSuccess
    failure: CreateTestError


async def test_auto_populate():
    """Test that status and errors are auto-populated."""
    # This test requires a PostgreSQL connection
    # Adapt connection string as needed
    app = create_fraiseql_app(
        db_url="postgresql://user:pass@localhost/testdb",
        mutations=[CreateTest],
    )

    # Execute mutation (requires app.test_create function in database)
    query = """
    mutation {
        createTest(input: {name: "test"}) {
            __typename
            ... on CreateTestSuccess {
                status
                message
                errors {
                    code
                    identifier
                    message
                }
                testEntity
            }
            ... on CreateTestError {
                status
                message
                errors {
                    code
                    identifier
                    message
                }
            }
        }
    }
    """

    result = await app.execute(query)

    # Verify response structure
    assert result.errors is None, f"GraphQL errors: {result.errors}"

    data = result.data["createTest"]
    print(f"Response: {data}")

    # Verify auto-populated fields
    assert "status" in data, "status field missing"
    assert "message" in data, "message field missing"
    assert "errors" in data, "errors field missing"

    if data["__typename"].endswith("Success"):
        assert data["status"].startswith("success"), f"Expected success status, got {data['status']}"
        assert isinstance(data["errors"], list), "errors should be list"
        assert len(data["errors"]) == 0, "Success should have empty errors array"
    else:
        assert not data["status"].startswith("success"), f"Error should not have success status"
        assert isinstance(data["errors"], list), "errors should be list"
        assert len(data["errors"]) > 0, "Error should have errors array with items"

    print("✅ Auto-population test passed!")


if __name__ == "__main__":
    asyncio.run(test_auto_populate())
```

**Run manual test** (if database available):
```bash
python tests/manual/test_auto_populate.py
```

**Expected output**:
```
Response: {'__typename': 'CreateTestSuccess', 'status': 'success', 'message': 'Created', 'errors': [], 'testEntity': {...}}
✅ Auto-population test passed!
```

### Step 8: Verify error responses still work

**Test that error responses are unchanged**:

```bash
cd fraiseql_rs

# Run error-specific tests
cargo test error_array_generation
cargo test status_tests

cd ..
```

**Expected outcome**: All error tests pass (no changes to error logic)

## Verification Commands

```bash
# Full test suite
cd fraiseql_rs && cargo test && cd ..

# New tests only
cd fraiseql_rs && cargo test auto_populate_fields_tests && cd ..

# Python tests (if available)
uv run pytest tests/ -v

# Lint check
cd fraiseql_rs && cargo clippy && cd ..
```

## Expected Outcome

### Rust Tests Should:
- ✅ All existing tests pass (backward compatible)
- ✅ 6 new tests pass (auto-populate functionality)
- ✅ No warnings from cargo clippy
- ✅ Total test count increased by 6

### Python Tests Should:
- ✅ All existing integration tests pass
- ✅ Mutation responses include new fields
- ✅ No breaking changes for existing code

### Manual Testing Should Reveal:
- ✅ Success responses have `status: "success"` (or variant)
- ✅ Success responses have `errors: []` (empty array)
- ✅ Error responses unchanged (still have all fields)
- ✅ Entity fields still present and correct
- ✅ updatedFields still present (if applicable)

## Acceptance Criteria

- [ ] All existing Rust tests pass (0 failures)
- [ ] 6 new Rust tests added and passing
- [ ] `test_success_response_has_status_field` passes
- [ ] `test_success_response_has_errors_field` passes
- [ ] `test_success_response_all_standard_fields` passes
- [ ] `test_success_status_preserves_detail` passes
- [ ] `test_success_fields_order` passes
- [ ] All existing Python tests pass (if any)
- [ ] Manual integration test confirms behavior
- [ ] Error responses still work correctly (unchanged)
- [ ] No cargo clippy warnings

## DO NOT

- **DO NOT skip any tests** - run full suite to ensure no regressions
- **DO NOT ignore test failures** - fix or understand every failure
- **DO NOT modify test data** to make tests pass - fix implementation instead
- **DO NOT commit yet** - wait for Phase 4 documentation
- **DO NOT deploy to production** - needs documentation first

## Notes

### Common Test Failure Scenarios

**Scenario 1: Field count mismatch**
```
Error: Expected 5 fields, got 7
```
**Fix**: Update test to expect 7 fields (5 old + 2 new)

**Scenario 2: Unexpected field in response**
```
Error: Unexpected field 'status' in response
```
**Fix**: Update test to allow/expect 'status' and 'errors' fields

**Scenario 3: Field order changed**
```
Error: Expected field 'user' at position 4, found at position 6
```
**Fix**: Update test to not depend on exact field positions, or adjust expected positions

### Debugging Tips

**Print actual response**:
```rust
let response = build_success_response(...).unwrap();
eprintln!("Actual response: {:#?}", response);
```

**Check field names**:
```rust
let obj = response.as_object().unwrap();
eprintln!("Fields: {:?}", obj.keys().collect::<Vec<_>>());
```

**Compare with expected**:
```rust
eprintln!("Expected: {:?}", expected_fields);
eprintln!("Actual: {:?}", actual_fields);
```

### Integration Test Considerations

If you have access to a test database:
1. Create simple mutation function that returns `app.mutation_response`
2. Execute via GraphQL
3. Verify response structure matches expectations
4. Test both success and error cases

If no test database available:
- Rust unit tests are sufficient for this phase
- Integration testing can happen in Phase 4 (documentation phase)

### Performance Testing (Optional)

**Before and after benchmark**:
```bash
cd fraiseql_rs

# Run benchmarks
cargo bench --bench mutation_benchmark

cd ..
```

**Expected**: Negligible performance impact (2 field insertions is ~1ns overhead)

### Next Phase Preview

**Phase 4** will:
1. Document new behavior in CHANGELOG
2. Write migration guide for existing users
3. Update tutorial examples to use simplified pattern
4. Add API reference documentation
5. Create before/after code examples
6. Final verification and commit
