# FraiseQL Status Taxonomy Implementation Plan

**Goal:** Implement minimal status taxonomy in Rust with proper error detection, following TDD methodology.

**Context:** Fix bug where `error_config` is ignored in HTTP mode because Rust hardcodes only `"failed:"` and `"noop:"` prefixes. Implement comprehensive status handling based on GraphQL Cascade feedback.

**Related:**

- Bug report: `/home/lionel/code/printoptim_backend_manual_migration/docs/FRAISEQL_BUG_ERROR_CONFIG_NOT_PASSED_TO_RUST.md`
- GraphQL Cascade issue: https://github.com/graphql-cascade/graphql-cascade/issues/1
- Current Rust code: `fraiseql_rs/src/mutation/mod.rs:88-96`

---

## Phase 1: RED - Write Failing Tests for Status Detection

**Objective:** Write comprehensive tests for all status categories before changing implementation.

### Task 1.1: Add test file for status taxonomy

**File:** `fraiseql_rs/src/mutation/tests.rs`

**Add tests:**

```rust
#[cfg(test)]
mod test_status_taxonomy {
    use super::*;

    // SUCCESS KEYWORDS (no colon)
    #[test]
    fn test_success_keywords() {
        assert!(MutationStatus::from_str("success").is_success());
        assert!(MutationStatus::from_str("created").is_success());
        assert!(MutationStatus::from_str("updated").is_success());
        assert!(MutationStatus::from_str("deleted").is_success());
    }

    // ERROR PREFIXES (colon-separated)
    #[test]
    fn test_failed_prefix() {
        let status = MutationStatus::from_str("failed:validation");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, "validation"),
            _ => panic!("Expected Error variant"),
        }
    }

    #[test]
    fn test_unauthorized_prefix() {
        let status = MutationStatus::from_str("unauthorized:token_expired");
        assert!(status.is_error());
    }

    #[test]
    fn test_forbidden_prefix() {
        let status = MutationStatus::from_str("forbidden:insufficient_permissions");
        assert!(status.is_error());
    }

    #[test]
    fn test_not_found_prefix() {
        let status = MutationStatus::from_str("not_found:user_missing");
        assert!(status.is_error());
    }

    #[test]
    fn test_conflict_prefix() {
        let status = MutationStatus::from_str("conflict:duplicate_email");
        assert!(status.is_error());
    }

    #[test]
    fn test_timeout_prefix() {
        let status = MutationStatus::from_str("timeout:database_query");
        assert!(status.is_error());
    }

    // NOOP PREFIX (success with no changes)
    #[test]
    fn test_noop_prefix() {
        let status = MutationStatus::from_str("noop:unchanged");
        assert!(status.is_noop());
        match status {
            MutationStatus::Noop(reason) => assert_eq!(reason, "unchanged"),
            _ => panic!("Expected Noop variant"),
        }
    }

    #[test]
    fn test_noop_duplicate() {
        let status = MutationStatus::from_str("noop:duplicate");
        assert!(status.is_noop());
    }

    // CASE INSENSITIVITY
    #[test]
    fn test_case_insensitive_error_prefix() {
        assert!(MutationStatus::from_str("FAILED:validation").is_error());
        assert!(MutationStatus::from_str("Unauthorized:token").is_error());
        assert!(MutationStatus::from_str("Conflict:DUPLICATE").is_error());
    }

    #[test]
    fn test_case_insensitive_success() {
        assert!(MutationStatus::from_str("SUCCESS").is_success());
        assert!(MutationStatus::from_str("Created").is_success());
    }

    // EDGE CASES
    #[test]
    fn test_status_with_multiple_colons() {
        let status = MutationStatus::from_str("failed:validation:email_invalid");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, "validation:email_invalid"),
            _ => panic!("Expected Error with full reason"),
        }
    }

    #[test]
    fn test_error_prefix_without_reason() {
        let status = MutationStatus::from_str("failed:");
        assert!(status.is_error());
        match status {
            MutationStatus::Error(reason) => assert_eq!(reason, ""),
            _ => panic!("Expected Error with empty reason"),
        }
    }

    #[test]
    fn test_unknown_status_becomes_success() {
        // Unknown statuses default to success for backward compatibility
        let status = MutationStatus::from_str("unknown_status");
        assert!(status.is_success());
    }

    #[test]
    fn test_empty_status() {
        let status = MutationStatus::from_str("");
        assert!(status.is_success());
    }
}
```

### Task 1.2: Run tests to verify they FAIL

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test test_status_taxonomy -- --nocapture
```

**Expected:** Most tests fail because current implementation only recognizes `"noop:"` and `"failed:"`.

**Acceptance Criteria:**

- [ ] Test file created with 20+ test cases
- [ ] Tests cover: success keywords, all error prefixes, noop, case insensitivity, edge cases
- [ ] `cargo test` shows failing tests (RED phase complete)

---

## Phase 2: GREEN - Implement Minimal Status Taxonomy

**Objective:** Make all tests pass with minimal implementation.

### Task 2.1: Update `MutationStatus::from_str()` implementation

**File:** `fraiseql_rs/src/mutation/mod.rs:88-127`

**Replace current implementation:**

```rust
impl MutationStatus {
    /// Parse status string into enum with minimal taxonomy
    ///
    /// # Status Categories
    ///
    /// ## Success (no colon)
    /// - "success", "created", "updated", "deleted"
    ///
    /// ## Error (colon-separated)
    /// - "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:"
    ///
    /// ## Noop (colon-separated, success with no changes)
    /// - "noop:"
    ///
    /// # Case Insensitivity
    /// All status strings are matched case-insensitively.
    ///
    /// # Examples
    /// ```
    /// assert!(MutationStatus::from_str("success").is_success());
    /// assert!(MutationStatus::from_str("failed:validation").is_error());
    /// assert!(MutationStatus::from_str("noop:unchanged").is_noop());
    /// assert!(MutationStatus::from_str("CONFLICT:duplicate").is_error());
    /// ```
    pub fn from_str(status: &str) -> Self {
        let status_lower = status.to_lowercase();

        // ERROR PREFIXES - Return Error type
        if status_lower.starts_with("failed:")
            || status_lower.starts_with("unauthorized:")
            || status_lower.starts_with("forbidden:")
            || status_lower.starts_with("not_found:")
            || status_lower.starts_with("conflict:")
            || status_lower.starts_with("timeout:")
        {
            // Extract reason after first colon
            let colon_pos = status.find(':').unwrap_or(status.len());
            let reason = if colon_pos < status.len() - 1 {
                &status[colon_pos + 1..]
            } else {
                ""
            };
            MutationStatus::Error(reason.to_string())
        }
        // NOOP PREFIX - Return Noop (success with no changes)
        else if status_lower.starts_with("noop:") {
            let colon_pos = status.find(':').unwrap_or(status.len());
            let reason = if colon_pos < status.len() - 1 {
                &status[colon_pos + 1..]
            } else {
                ""
            };
            MutationStatus::Noop(reason.to_string())
        }
        // SUCCESS KEYWORDS - Return Success
        else if matches!(
            status_lower.as_str(),
            "success" | "created" | "updated" | "deleted"
        ) {
            MutationStatus::Success(status.to_string())
        }
        // DEFAULT - Unknown statuses become Success (backward compatibility)
        else {
            // Note: In production, this should log a warning
            MutationStatus::Success(status.to_string())
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, MutationStatus::Noop(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, MutationStatus::Error(_))
    }

    /// Map status to HTTP code
    pub fn http_code(&self) -> i32 {
        match self {
            MutationStatus::Success(_) => 200,
            MutationStatus::Noop(_) => 200,  // Noop is success (no change made)
            MutationStatus::Error(reason) => {
                // Map error reasons to HTTP status codes
                let reason_lower = reason.to_lowercase();
                if reason_lower.contains("not_found") || reason_lower.contains("missing") {
                    404
                } else if reason_lower.contains("unauthorized") || reason_lower.contains("unauthenticated") {
                    401
                } else if reason_lower.contains("forbidden") || reason_lower.contains("permission") {
                    403
                } else if reason_lower.contains("conflict") || reason_lower.contains("duplicate") {
                    409
                } else if reason_lower.contains("validation") || reason_lower.contains("invalid") {
                    422
                } else if reason_lower.contains("timeout") {
                    408
                } else {
                    500  // Generic internal error
                }
            }
        }
    }
}
```

### Task 2.2: Update `VALID_STATUS_PREFIXES` constant

**File:** `fraiseql_rs/src/mutation/mod.rs:149-152`

**Update to reflect new prefixes:**

```rust
/// Valid mutation status prefixes/values for format detection
const VALID_STATUS_PREFIXES: &[&str] = &[
    // Success keywords (no colon)
    "success", "created", "updated", "deleted",
    // Error prefixes
    "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:",
    // Noop prefix
    "noop:",
];
```

### Task 2.3: Run tests to verify they PASS

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test test_status_taxonomy -- --nocapture
```

**Expected:** All tests pass (GREEN phase complete).

**Acceptance Criteria:**

- [ ] All 20+ tests pass
- [ ] `cargo test` shows no failures
- [ ] GREEN phase complete

---

## Phase 3: REFACTOR - Add Integration Tests

**Objective:** Test full mutation pipeline with new status taxonomy.

### Task 3.1: Add integration test for mutation response building

**File:** `fraiseql_rs/src/mutation/tests.rs`

**Add integration tests:**

```rust
#[cfg(test)]
mod test_mutation_response_integration {
    use super::*;

    #[test]
    fn test_build_error_response_validation() {
        let mutation_json = r#"{
            "status": "failed:validation_error",
            "message": "Invalid email format",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();

        // Parse JSON to verify structure
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be error type
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserError"
        );
        assert_eq!(
            response["data"]["createUser"]["message"],
            "Invalid email format"
        );
    }

    #[test]
    fn test_build_error_response_conflict() {
        let mutation_json = r#"{
            "status": "conflict:duplicate_email",
            "message": "Email already exists",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be error type with conflict status
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserError"
        );
        assert!(response["data"]["createUser"]["status"]
            .as_str()
            .unwrap()
            .starts_with("conflict:"));
    }

    #[test]
    fn test_build_noop_response() {
        let mutation_json = r#"{
            "status": "noop:duplicate",
            "message": "Already exists",
            "entity_id": "123",
            "entity_type": "User",
            "entity": {"id": "123", "email": "test@example.com"},
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Noop should be SUCCESS type (no change, but not an error)
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserSuccess"
        );
        assert_eq!(
            response["data"]["createUser"]["message"],
            "Already exists"
        );
    }

    #[test]
    fn test_build_success_response() {
        let mutation_json = r#"{
            "status": "created",
            "message": "User created successfully",
            "entity_id": "456",
            "entity_type": "User",
            "entity": {"id": "456", "email": "new@example.com", "name": "Test User"},
            "updated_fields": ["email", "name"],
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        // Should be success type
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserSuccess"
        );
        assert!(response["data"]["createUser"]["user"].is_object());
        assert_eq!(response["data"]["createUser"]["user"]["id"], "456");
    }

    #[test]
    fn test_unauthorized_error() {
        let mutation_json = r#"{
            "status": "unauthorized:token_expired",
            "message": "Authentication token has expired",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "updateProfile",
            "UpdateProfileSuccess",
            "UpdateProfileError",
            None,
            None,
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        assert_eq!(
            response["data"]["updateProfile"]["__typename"],
            "UpdateProfileError"
        );
    }

    #[test]
    fn test_timeout_error() {
        let mutation_json = r#"{
            "status": "timeout:database_query",
            "message": "Database query exceeded 30 second timeout",
            "entity_id": null,
            "entity_type": null,
            "entity": null,
            "updated_fields": null,
            "cascade": null,
            "metadata": null
        }"#;

        let result = build_mutation_response(
            mutation_json,
            "processLargeDataset",
            "ProcessSuccess",
            "ProcessError",
            None,
            None,
            None,
            true,
        );

        assert!(result.is_ok());
        let response_bytes = result.unwrap();
        let response_str = String::from_utf8(response_bytes).unwrap();
        let response: serde_json::Value = serde_json::from_str(&response_str).unwrap();

        assert_eq!(
            response["data"]["processLargeDataset"]["__typename"],
            "ProcessError"
        );
        assert!(response["data"]["processLargeDataset"]["status"]
            .as_str()
            .unwrap()
            .starts_with("timeout:"));
    }
}
```

### Task 3.2: Run full test suite

```bash
cd /home/lionel/code/fraiseql/fraiseql_rs
cargo test
```

**Expected:** All tests pass (unit + integration).

**Acceptance Criteria:**

- [ ] Integration tests added (6+ scenarios)
- [ ] All tests pass
- [ ] Code coverage includes error detection logic

---

## Phase 4: QA - Python Integration Testing

**Objective:** Verify Python ↔ Rust integration works correctly.

### Task 4.1: Create Python test for status taxonomy

**File:** `tests/test_mutations/test_status_taxonomy.py`

**Add test:**

```python
"""Test status taxonomy in Python → Rust → Python flow."""
import pytest
from fraiseql.mutations.error_config import MutationErrorConfig


@pytest.mark.asyncio
async def test_validation_error_detected(db_connection, clear_registry):
    """Test that validation: prefix is detected as error."""
    # Create test function that returns validation error
    await db_connection.execute("""
        CREATE FUNCTION test_validation_error(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN (
                'failed:validation_error',
                'Invalid email format',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_validation_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    # Execute via Rust path
    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_validation_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    # Should return error type
    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "validation" in response["data"]["testMutation"]["status"]


@pytest.mark.asyncio
async def test_conflict_error_detected(db_connection, clear_registry):
    """Test that conflict: prefix is detected as error."""
    await db_connection.execute("""
        CREATE FUNCTION test_conflict_error(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN (
                'conflict:duplicate_email',
                'Email already exists',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_conflict_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_conflict_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "conflict:" in response["data"]["testMutation"]["status"]


@pytest.mark.asyncio
async def test_noop_returns_success_type(db_connection, clear_registry):
    """Test that noop: prefix returns success type (not error)."""
    await db_connection.execute("""
        CREATE FUNCTION test_noop_status(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN (
                'noop:duplicate',
                'Already exists',
                '123',
                'TestEntity',
                '{"id": "123"}'::jsonb,
                NULL,
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_noop_status")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_noop_status",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    # Noop should be SUCCESS type (no change is not an error)
    assert response["data"]["testMutation"]["__typename"] == "TestSuccess"


@pytest.mark.asyncio
async def test_timeout_error_detected(db_connection, clear_registry):
    """Test that timeout: prefix is detected as error."""
    await db_connection.execute("""
        CREATE FUNCTION test_timeout_error(input_data JSONB)
        RETURNS mutation_result_v2 AS $$
        BEGIN
            RETURN (
                'timeout:database_query',
                'Query exceeded timeout',
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL
            )::mutation_result_v2;
        END;
        $$ LANGUAGE plpgsql;
    """)

    import fraiseql
    from fraiseql.mutations import mutation

    @fraiseql.type
    class TestSuccess:
        id: str
        message: str

    @fraiseql.type
    class TestError:
        message: str
        status: str

    @mutation(function="test_timeout_error")
    class TestMutation:
        input: dict
        success: TestSuccess
        error: TestError

    from fraiseql.mutations.rust_executor import execute_mutation_rust

    result = await execute_mutation_rust(
        conn=db_connection,
        function_name="test_timeout_error",
        input_data={},
        field_name="testMutation",
        success_type="TestSuccess",
        error_type="TestError",
    )

    response = result.to_json()
    assert response["data"]["testMutation"]["__typename"] == "TestError"
    assert "timeout:" in response["data"]["testMutation"]["status"]
```

### Task 4.2: Run Python tests

```bash
cd /home/lionel/code/fraiseql
uv run pytest tests/test_mutations/test_status_taxonomy.py -v
```

**Expected:** All Python integration tests pass.

**Acceptance Criteria:**

- [ ] Python tests added (4+ scenarios)
- [ ] Tests cover error detection, noop handling, success cases
- [ ] All tests pass

---

## Phase 5: Documentation

**Objective:** Document the status taxonomy for FraiseQL users.

### Task 5.1: Create status string documentation

**File:** `docs/mutations/status-strings.md`

**Content:**

```markdown
# FraiseQL Status String Conventions

FraiseQL uses status strings in PostgreSQL functions to indicate mutation outcomes. These strings are parsed by the Rust layer and mapped to GraphQL Success/Error types.

## Status Categories

### 1. Success Statuses (No Colon)

Simple keywords indicating successful operations:

| Status | Meaning | GraphQL Type |
|--------|---------|--------------|
| `success` | Generic success | Success |
| `created` | Entity created | Success |
| `updated` | Entity modified | Success |
| `deleted` | Entity removed | Success |

**Example:**
```sql
RETURN ('created', 'User created successfully', v_user_id, 'User', v_user_json, ...)::mutation_result_v2;
```

### 2. Error Prefixes (Colon-Separated)

Prefixes indicating operation failures. These map to the Error type in GraphQL.

| Prefix | Meaning | HTTP Code | Example |
|--------|---------|-----------|---------|
| `failed:` | Generic failure | 500 | `failed:validation_error` |
| `unauthorized:` | Authentication required | 401 | `unauthorized:token_expired` |
| `forbidden:` | Insufficient permissions | 403 | `forbidden:admin_only` |
| `not_found:` | Resource doesn't exist | 404 | `not_found:user_missing` |
| `conflict:` | Resource conflict | 409 | `conflict:duplicate_email` |
| `timeout:` | Operation timeout | 408 | `timeout:external_api` |

**Example:**

```sql
IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
    RETURN ('conflict:duplicate_email', 'Email already exists', ...)::mutation_result_v2;
END IF;
```

### 3. Noop Prefix (Success with No Changes)

Indicates no change was made, but it's not an error. Maps to Success type.

| Prefix | Meaning | GraphQL Type |
|--------|---------|--------------|
| `noop:` | No operation performed | Success |

**Common noop reasons:**

- `noop:duplicate` - Entity already exists (idempotent operation)
- `noop:unchanged` - No fields changed
- `noop:blocked` - Blocked by business rules

**Example:**

```sql
INSERT INTO subscriptions (user_id, plan_id)
VALUES (v_user_id, v_plan_id)
ON CONFLICT DO NOTHING;

IF NOT FOUND THEN
    RETURN ('noop:duplicate', 'Already subscribed', v_user_id, ...)::mutation_result_v2;
END IF;
```

## Case Insensitivity

All status strings are matched **case-insensitively**:

```sql
'SUCCESS' = 'success' = 'Success'  ✅
'FAILED:validation' = 'failed:validation'  ✅
'Conflict:DUPLICATE' = 'conflict:duplicate'  ✅
```

## Complete Example

```sql
CREATE FUNCTION create_user(input_data JSONB)
RETURNS mutation_result_v2 AS $$
DECLARE
    v_email TEXT;
    v_user_id UUID;
    v_user_json JSONB;
BEGIN
    v_email := input_data->>'email';

    -- Validation error
    IF v_email IS NULL OR v_email = '' THEN
        RETURN (
            'failed:validation_error',
            'Email is required',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_result_v2;
    END IF;

    -- Conflict error (duplicate)
    IF EXISTS (SELECT 1 FROM users WHERE email = v_email) THEN
        RETURN (
            'conflict:duplicate_email',
            'Email already exists',
            NULL, NULL, NULL, NULL, NULL, NULL
        )::mutation_result_v2;
    END IF;

    -- Success - create user
    INSERT INTO users (email, name)
    VALUES (v_email, input_data->>'name')
    RETURNING id, row_to_json(users.*) INTO v_user_id, v_user_json;

    RETURN (
        'created',
        'User created successfully',
        v_user_id::TEXT,
        'User',
        v_user_json,
        ARRAY['email', 'name'],
        NULL,
        NULL
    )::mutation_result_v2;
END;
$$ LANGUAGE plpgsql;
```

## GraphQL Response Mapping

| PostgreSQL Status | GraphQL Type | HTTP | Example Response |
|-------------------|--------------|------|------------------|
| `created` | Success | 200 | `{ "__typename": "CreateUserSuccess", ... }` |
| `failed:validation` | Error | 422 | `{ "__typename": "CreateUserError", ... }` |
| `conflict:duplicate` | Error | 409 | `{ "__typename": "CreateUserError", ... }` |
| `noop:duplicate` | Success | 200 | `{ "__typename": "CreateUserSuccess", ... }` |
| `timeout:database` | Error | 408 | `{ "__typename": "CreateUserError", ... }` |

## Best Practices

### ✅ DO

- Use specific error prefixes (`conflict:`, `not_found:`) over generic `failed:`
- Include descriptive reasons after the colon: `failed:email_format_invalid`
- Use `noop:` for idempotent operations that encounter existing data
- Return appropriate entity data even for noop/error cases when available

### ❌ DON'T

- Don't use `duplicate:` as a prefix - use `conflict:duplicate` (error) or `noop:duplicate` (success)
- Don't mix prefix categories: `failed:noop:...` is confusing
- Don't include sensitive information in status strings (use message field)
- Don't create custom prefixes - use the standard ones

## Migration from Old Patterns

If you have existing functions using custom statuses:

| Old Pattern | New Pattern | Type |
|-------------|-------------|------|
| `validation:field_required` | `failed:validation_error` | Error |
| `error:database` | `failed:database_error` | Error |
| `duplicate:email` | `conflict:duplicate_email` | Error |
| `already_exists` | `noop:duplicate` | Success |


```

### Task 5.2: Update Python error_config documentation
**File:** `src/fraiseql/mutations/error_config.py`

**Update docstring:**
```python
"""Configurable error detection for mutations.

⚠️ DEPRECATION NOTICE:
Starting with FraiseQL v0.2.0, error detection is handled by the Rust layer
using standardized status prefixes. The error_config parameter is only used
in non-HTTP mode (direct GraphQL execution).

For HTTP mode (production), use status strings in PostgreSQL functions:
- Error prefixes: "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:"
- Noop prefix: "noop:"
- Success keywords: "success", "created", "updated", "deleted"

See docs/mutations/status-strings.md for complete guide.
"""
```

### Task 5.3: Add changelog entry

**File:** `CHANGELOG.md`

**Add entry:**

```markdown
## [Unreleased]

### Fixed
- **CRITICAL**: Fixed bug where custom `error_config` was ignored in HTTP mode (production)
  - Error detection now happens in Rust layer using status string prefixes
  - All mutations via FastAPI now correctly map status strings to Success/Error types
  - Fixes issue where `validation:`, `conflict:`, and other custom prefixes returned as Success

### Added
- Comprehensive status taxonomy in Rust mutation layer
  - Error prefixes: `failed:`, `unauthorized:`, `forbidden:`, `not_found:`, `conflict:`, `timeout:`
  - Noop prefix: `noop:` (returns Success type with no changes)
  - Success keywords: `success`, `created`, `updated`, `deleted`
  - Case-insensitive status matching
- Documentation: `docs/mutations/status-strings.md` - Complete guide to status string conventions

### Changed
- `error_config` parameter is now deprecated for HTTP mode (still works in non-HTTP mode)
- Status detection moved from Python `parse_mutation_result()` to Rust `MutationStatus::from_str()`

### Migration Guide
If you have mutations using custom `error_config`:

**Before:**
```python
CUSTOM_ERROR_CONFIG = MutationErrorConfig(
    error_prefixes={"validation:", "error:", "failed:"}
)

@mutation(function="create_user", error_config=CUSTOM_ERROR_CONFIG)
class CreateUser: ...
```

**After (update PostgreSQL functions):**

```sql
-- Use standardized prefixes in PostgreSQL
RETURN ('failed:validation_error', 'Invalid email', ...)::mutation_result_v2;
RETURN ('conflict:duplicate_email', 'Email exists', ...)::mutation_result_v2;
RETURN ('noop:duplicate', 'Already exists', ...)::mutation_result_v2;
```

No Python changes needed - `error_config` can be removed.

```

**Acceptance Criteria:**
- [ ] Status string documentation created
- [ ] Python error_config updated with deprecation notice
- [ ] Changelog entry added
- [ ] Documentation reviewed for clarity

---

## Phase 6: Deployment & Verification

**Objective:** Deploy to FraiseQL and verify PrintOptim bug is fixed.

### Task 6.1: Build and test FraiseQL
```bash
cd /home/lionel/code/fraiseql
uv run maturin develop --release
uv run pytest tests/ -v
```

**Expected:** All tests pass with new Rust code.

### Task 6.2: Update PrintOptim to test the fix

**File:** Create test in PrintOptim to verify `validation:` prefix works

```bash
cd /home/lionel/code/printoptim_backend_manual_migration
# Update FraiseQL dependency to local version
uv pip install -e /home/lionel/code/fraiseql
```

### Task 6.3: Run PrintOptim's failing test

```bash
cd /home/lionel/code/printoptim_backend_manual_migration
# Run the test mentioned in bug report
uv run pytest -xvs tests/test_public_address_mutations.py::test_create_public_address_validation_error
```

**Expected:** Test now passes - `validation:` errors return Error type.

### Task 6.4: Create verification issue in PrintOptim docs

**File:** Update bug report with resolution

Add to `/home/lionel/code/printoptim_backend_manual_migration/docs/FRAISEQL_BUG_ERROR_CONFIG_NOT_PASSED_TO_RUST.md`:

```markdown
## ✅ RESOLVED

**Fixed in:** FraiseQL v0.2.0 (commit: [hash])
**Resolution:** Implemented comprehensive status taxonomy in Rust layer

### What Changed
- Error detection moved to Rust `MutationStatus::from_str()`
- All standard error prefixes now recognized: `failed:`, `validation:`, `conflict:`, `timeout:`, etc.
- `error_config` no longer needed for HTTP mode

### Verification
- [x] PrintOptim validation tests pass
- [x] `validation:` prefix returns Error type
- [x] `conflict:` prefix returns Error type
- [x] `noop:` prefix returns Success type
- [x] HTTP mode and non-HTTP mode consistent

### Migration Required
None - existing PrintOptim code works with updated FraiseQL. The `PRINTOPTIM_ERROR_CONFIG` can be removed in future cleanup.
```

**Acceptance Criteria:**

- [ ] FraiseQL built successfully with Rust changes
- [ ] All FraiseQL tests pass
- [ ] PrintOptim updated to use new FraiseQL
- [ ] PrintOptim validation test passes
- [ ] Bug report updated with resolution

---

## Success Criteria

✅ **RED Phase Complete:** Comprehensive failing tests written
✅ **GREEN Phase Complete:** All tests pass with minimal implementation
✅ **REFACTOR Phase Complete:** Integration tests added
✅ **QA Phase Complete:** Python integration verified
✅ **Documentation Complete:** Status strings documented
✅ **Deployment Complete:** PrintOptim bug fixed

## Related Files

**Rust:**

- `fraiseql_rs/src/mutation/mod.rs` - Core implementation
- `fraiseql_rs/src/mutation/tests.rs` - Unit tests

**Python:**

- `src/fraiseql/mutations/error_config.py` - Python config (deprecated for HTTP mode)
- `tests/test_mutations/test_status_taxonomy.py` - Integration tests

**Documentation:**

- `docs/mutations/status-strings.md` - User guide
- `CHANGELOG.md` - Release notes
- `/home/lionel/code/printoptim_backend_manual_migration/docs/FRAISEQL_BUG_ERROR_CONFIG_NOT_PASSED_TO_RUST.md` - Bug report

**GraphQL Cascade:**

- https://github.com/graphql-cascade/graphql-cascade/issues/1 - Spec proposal
