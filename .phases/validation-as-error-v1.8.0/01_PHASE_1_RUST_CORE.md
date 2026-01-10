# Phase 1: Rust Core Changes

**Timeline:** Week 1, Days 1-3
**Risk Level:** HIGH (core mutation pipeline)
**Dependencies:** None
**Blocking:** Phases 2-5

---

## Objective

Update Rust mutation pipeline to:
1. Return Error type for ALL non-success statuses (including `noop:*`)
2. Add `code` field to error responses (422, 404, 409, 500)
3. Ensure Success type never has null entity
4. Maintain HTTP 200 OK for all responses

---

## Files to Modify

### Critical Files (Must Change)
1. `fraiseql_rs/src/mutation/response_builder.rs` - Main response logic
2. `fraiseql_rs/src/mutation/mod.rs` - Status classification
3. `fraiseql_rs/src/mutation/types.rs` - Type definitions

### Supporting Files (May Need Updates)
4. `fraiseql_rs/src/mutation/tests.rs` - Unit tests
5. `fraiseql_rs/src/mutation/test_status_only.rs` - Status parsing tests

---

## Implementation Steps

### Step 1.1: Update Status Classification

**File:** `fraiseql_rs/src/mutation/mod.rs`

**Current Logic (WRONG):**
```rust
// Line 24 in response_builder.rs
let response_obj = if result.status.is_success() || result.status.is_noop() {
    build_success_response(...)  // ❌ noop returns Success
} else {
    build_error_response(...)
};
```

**New Logic (CORRECT):**
```rust
let response_obj = if result.status.is_success() {
    build_success_response(...)  // ✅ ONLY true success
} else {
    build_error_response_with_code(...)  // ✅ Everything else (noop, failed, etc.)
};
```

**Code Changes:**

**Location:** `fraiseql_rs/src/mutation/response_builder.rs:24`

**OLD:**
```rust
pub fn build_graphql_response(
    result: &MutationResult,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    _entity_type: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let response_obj = if result.status.is_success() || result.status.is_noop() {
        build_success_response(
            result,
            success_type,
            entity_field_name,
            auto_camel_case,
            success_type_fields,
            cascade_selections,
        )?
    } else {
        build_error_response(result, error_type, auto_camel_case)?
    };

    // Wrap in GraphQL response structure
    Ok(json!({
        "data": {
            field_name: response_obj
        }
    }))
}
```

**NEW:**
```rust
pub fn build_graphql_response(
    result: &MutationResult,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    _entity_type: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    // v1.8.0: Only TRUE success returns Success type
    // All errors (noop, failed, conflict, etc.) return Error type
    let response_obj = if result.status.is_success() {
        build_success_response(
            result,
            success_type,
            entity_field_name,
            auto_camel_case,
            success_type_fields,
            cascade_selections,
        )?
    } else {
        // NEW: Error response includes REST-like code
        build_error_response_with_code(
            result,
            error_type,
            auto_camel_case,
            cascade_selections,
        )?
    };

    // Always HTTP 200 OK (GraphQL convention)
    Ok(json!({
        "data": {
            field_name: response_obj
        }
    }))
}
```

---

### Step 1.2: Implement Error Response with Code

**File:** `fraiseql_rs/src/mutation/response_builder.rs`

**NEW FUNCTION:**
```rust
/// Build error response object with REST-like code field
///
/// Key behaviors:
/// - Adds `code` field (422, 404, 409, 500) for DX
/// - Preserves `status` field (domain semantics)
/// - Includes `message` field (human-readable)
/// - Adds CASCADE if selected
/// - HTTP 200 OK at transport layer (code is application-level only)
pub fn build_error_response_with_code(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Add __typename
    obj.insert("__typename".to_string(), json!(error_type));

    // Add REST-like code field (application-level, NOT HTTP status)
    let code = map_status_to_code(&result.status);
    obj.insert("code".to_string(), json!(code));

    // Add status (domain semantics)
    obj.insert("status".to_string(), json!(result.status.to_string()));

    // Add message
    obj.insert("message".to_string(), json!(result.message));

    // Add cascade if present AND requested in selection
    add_cascade_if_selected(&mut obj, result, cascade_selections, auto_camel_case)?;

    Ok(Value::Object(obj))
}

/// Map mutation status to REST-like code (application-level only)
///
/// These codes are for DX and categorization only.
/// HTTP response is always 200 OK (GraphQL convention).
fn map_status_to_code(status: &MutationStatus) -> i32 {
    match status {
        MutationStatus::Success(_) => {
            // Should never reach here (only errors call this function)
            500
        }
        MutationStatus::Noop(_) => {
            // Validation failure or business rule rejection
            422 // Unprocessable Entity
        }
        MutationStatus::Error(reason) => {
            let reason_lower = reason.to_lowercase();

            // Map error reasons to codes
            if reason_lower.starts_with("not_found:") {
                404 // Not Found
            } else if reason_lower.starts_with("unauthorized:") {
                401 // Unauthorized
            } else if reason_lower.starts_with("forbidden:") {
                403 // Forbidden
            } else if reason_lower.starts_with("conflict:") {
                409 // Conflict
            } else if reason_lower.starts_with("timeout:") {
                408 // Request Timeout
            } else if reason_lower.starts_with("failed:") {
                500 // Internal Server Error
            } else {
                // Unknown error type
                500
            }
        }
    }
}
```

**DEPRECATE OLD FUNCTION:**
```rust
/// Build error response object (DEPRECATED - use build_error_response_with_code)
///
/// This function is kept for backward compatibility during migration.
/// It will be removed in v2.0.0.
#[deprecated(
    since = "1.8.0",
    note = "Use build_error_response_with_code instead"
)]
pub fn build_error_response(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
) -> Result<Value, String> {
    // Delegate to new function
    build_error_response_with_code(result, error_type, auto_camel_case, None)
}
```

---

### Step 1.3: Update Success Response Validation

**File:** `fraiseql_rs/src/mutation/response_builder.rs`

**Current:** Success response allows null entity (lines 101-171)

**Change:** Add validation to ensure entity exists for Success type

**Location:** `fraiseql_rs/src/mutation/response_builder.rs:79-171`

**ADD VALIDATION:**
```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
    cascade_selections: Option<&str>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Add __typename
    obj.insert("__typename".to_string(), json!(success_type));

    // Add id from entity_id if present
    if let Some(ref entity_id) = result.entity_id {
        obj.insert("id".to_string(), json!(entity_id));
    }

    // Add message
    obj.insert("message".to_string(), json!(result.message));

    // v1.8.0: SUCCESS MUST HAVE ENTITY (non-null guarantee)
    if result.entity.is_none() {
        return Err(format!(
            "Success type '{}' requires non-null entity. \
             Status '{}' returned null entity. \
             This indicates a logic error: non-success statuses (noop:*, failed:*, etc.) \
             should return Error type, not Success type.",
            success_type,
            result.status.to_string()
        ));
    }

    // Add entity with __typename and camelCase keys
    if let Some(entity) = &result.entity {
        // ... existing entity processing logic ...
    }

    // Rest of function unchanged...
}
```

---

### Step 1.4: Update Status Enum Methods

**File:** `fraiseql_rs/src/mutation/mod.rs`

**Current Methods:**
```rust
impl MutationStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, MutationStatus::Noop(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, MutationStatus::Error(_))
    }

    pub fn http_code(&self) -> i32 {
        match self {
            MutationStatus::Success(_) => 200,
            MutationStatus::Noop(_) => 200,  // ❌ Wrong - should be 422 at app level
            MutationStatus::Error(reason) => { /* ... */ }
        }
    }
}
```

**New Methods:**
```rust
impl MutationStatus {
    pub fn is_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, MutationStatus::Noop(_))
    }

    /// Returns true if this status should return Error type
    ///
    /// v1.8.0: Both Noop and Error return Error type
    pub fn is_error(&self) -> bool {
        matches!(self, MutationStatus::Error(_) | MutationStatus::Noop(_))
    }

    /// Returns true if this status should return Success type
    ///
    /// v1.8.0: Only Success(_) returns Success type
    pub fn is_graphql_success(&self) -> bool {
        matches!(self, MutationStatus::Success(_))
    }

    /// Map status to HTTP code (ALWAYS 200 for GraphQL)
    ///
    /// GraphQL always returns HTTP 200 OK.
    /// Use application_code() for REST-like categorization.
    pub fn http_code(&self) -> i32 {
        200 // Always 200 OK for GraphQL
    }

    /// Map status to application-level code (for DX and categorization)
    ///
    /// This is NOT an HTTP status code. It's an application-level field
    /// that mirrors REST semantics for better developer experience.
    pub fn application_code(&self) -> i32 {
        match self {
            MutationStatus::Success(_) => 200,
            MutationStatus::Noop(_) => 422, // Validation/business rule
            MutationStatus::Error(reason) => {
                let reason_lower = reason.to_lowercase();
                if reason_lower.starts_with("not_found:") {
                    404
                } else if reason_lower.starts_with("unauthorized:") {
                    401
                } else if reason_lower.starts_with("forbidden:") {
                    403
                } else if reason_lower.starts_with("conflict:") {
                    409
                } else if reason_lower.starts_with("timeout:") {
                    408
                } else {
                    500
                }
            }
        }
    }

    /// Convert status to string
    pub fn to_string(&self) -> String {
        match self {
            MutationStatus::Success(s) => s.clone(),
            MutationStatus::Noop(s) => s.clone(),
            MutationStatus::Error(s) => s.clone(),
        }
    }
}
```

---

### Step 1.5: Update Type Definitions (if needed)

**File:** `fraiseql_rs/src/mutation/types.rs`

**Review:** Check if `MutationResult` struct needs updates

**Current:**
```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullResponse {
    pub status: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}
```

**No changes needed** - entity remains `Option<Value>` because database can return null.
The validation happens in `build_success_response` to ensure Success type never gets null entity.

---

## Testing Strategy

### Step 1.6: Update Unit Tests

**File:** `fraiseql_rs/src/mutation/tests.rs`

**Tests to Update:**

```rust
#[test]
fn test_noop_returns_error_type_v1_9() {
    let result = MutationResult {
        status: MutationStatus::Noop("noop:invalid_contract_id".to_string()),
        message: "Contract not found".to_string(),
        entity: None,
        cascade: Some(json!({"status": "noop:invalid_contract_id"})),
        ..Default::default()
    };

    let response = build_graphql_response(
        &result,
        "createMachine",
        "CreateMachineSuccess",
        "CreateMachineError",
        Some("machine"),
        Some("Machine"),
        true,
        None,
        Some(r#"{"status": true}"#),
    ).unwrap();

    let data = &response["data"]["createMachine"];
    assert_eq!(data["__typename"], "CreateMachineError");
    assert_eq!(data["code"], 422);
    assert_eq!(data["status"], "noop:invalid_contract_id");
    assert_eq!(data["message"], "Contract not found");
    assert!(data["cascade"].is_object());
}

#[test]
fn test_not_found_returns_error_type_with_404() {
    let result = MutationResult {
        status: MutationStatus::Error("not_found:machine".to_string()),
        message: "Machine not found".to_string(),
        entity: None,
        cascade: None,
        ..Default::default()
    };

    let response = build_graphql_response(&result, ...).unwrap();
    let data = &response["data"]["deleteMachine"];

    assert_eq!(data["__typename"], "DeleteMachineError");
    assert_eq!(data["code"], 404);
    assert_eq!(data["status"], "not_found:machine");
}

#[test]
fn test_conflict_returns_error_type_with_409() {
    let result = MutationResult {
        status: MutationStatus::Error("conflict:duplicate_serial".to_string()),
        message: "Serial number already exists".to_string(),
        entity: None,
        cascade: None,
        ..Default::default()
    };

    let response = build_graphql_response(&result, ...).unwrap();
    let data = &response["data"]["createMachine"];

    assert_eq!(data["__typename"], "CreateMachineError");
    assert_eq!(data["code"], 409);
    assert_eq!(data["status"], "conflict:duplicate_serial");
}

#[test]
fn test_success_with_null_entity_returns_error() {
    // v1.8.0: Success type with null entity should return error
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Created".to_string(),
        entity: None, // ❌ Null entity with Success status
        cascade: None,
        ..Default::default()
    };

    let response = build_graphql_response(&result, ...).unwrap_err();
    assert!(response.contains("Success type"));
    assert!(response.contains("requires non-null entity"));
}

#[test]
fn test_success_always_has_entity() {
    let result = MutationResult {
        status: MutationStatus::Success("created".to_string()),
        message: "Machine created".to_string(),
        entity: Some(json!({"id": "123", "name": "Test"})),
        cascade: None,
        ..Default::default()
    };

    let response = build_graphql_response(&result, ...).unwrap();
    let data = &response["data"]["createMachine"];

    assert_eq!(data["__typename"], "CreateMachineSuccess");
    assert!(data["machine"].is_object());
    assert_eq!(data["machine"]["id"], "123");
}
```

**File:** `fraiseql_rs/src/mutation/test_status_only.rs`

**Update status classification tests:**

```rust
#[test]
fn test_noop_is_error_v1_9() {
    let status = MutationStatus::from_str("noop:unchanged");
    assert!(status.is_noop());
    assert!(status.is_error()); // ✅ v1.8.0: noop is error
    assert!(!status.is_success());
    assert_eq!(status.application_code(), 422);
    assert_eq!(status.http_code(), 200); // Still HTTP 200
}

#[test]
fn test_not_found_is_error() {
    let status = MutationStatus::from_str("not_found:user");
    assert!(!status.is_noop());
    assert!(status.is_error());
    assert!(!status.is_success());
    assert_eq!(status.application_code(), 404);
    assert_eq!(status.http_code(), 200);
}

#[test]
fn test_conflict_is_error() {
    let status = MutationStatus::from_str("conflict:duplicate");
    assert!(status.is_error());
    assert_eq!(status.application_code(), 409);
}

#[test]
fn test_success_is_not_error() {
    let status = MutationStatus::from_str("created");
    assert!(status.is_success());
    assert!(!status.is_error());
    assert!(!status.is_noop());
    assert_eq!(status.application_code(), 200);
}
```

---

## Verification Checklist

### Code Changes
- [ ] `response_builder.rs:24` - Remove `|| result.status.is_noop()`
- [ ] `response_builder.rs` - Add `build_error_response_with_code` function
- [ ] `response_builder.rs` - Add `map_status_to_code` function
- [ ] `response_builder.rs:79` - Add null entity validation in success response
- [ ] `mod.rs` - Update `is_error()` to include Noop
- [ ] `mod.rs` - Add `is_graphql_success()` method
- [ ] `mod.rs` - Update `http_code()` to always return 200
- [ ] `mod.rs` - Add `application_code()` method

### Testing
- [ ] All existing tests updated for new behavior
- [ ] New test: `test_noop_returns_error_type_v1_9`
- [ ] New test: `test_not_found_returns_error_type_with_404`
- [ ] New test: `test_conflict_returns_error_type_with_409`
- [ ] New test: `test_success_with_null_entity_returns_error`
- [ ] All unit tests pass (`cargo test`)

### Documentation
- [ ] Add inline comments explaining v1.8.0 changes
- [ ] Update function doc comments
- [ ] Add deprecation notices for old functions

---

## Expected Output After Phase 1

**Before:**
```json
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
```

**After:**
```json
{
  "data": {
    "createMachine": {
      "__typename": "CreateMachineError",
      "code": 422,
      "status": "noop:invalid_contract_id",
      "message": "Contract not found",
      "cascade": {"status": "noop:invalid_contract_id"}
    }
  }
}
```

---

## Next Steps

Once Phase 1 is complete:
1. Run full Rust test suite: `cargo test`
2. Run Rust benchmarks to ensure no regression
3. Commit changes: `git commit -m "feat(mutations)!: validation as Error type (v1.8.0) [RUST]"`
4. Proceed to Phase 2: Python Layer Updates

**Blocking:** Python layer (Phase 2) depends on these Rust changes.
