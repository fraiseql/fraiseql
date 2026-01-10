# Phase 3: Response Building

**Duration**: 2-3 days
**Objective**: Build GraphQL-compliant Success and Error responses
**Status**: COMPLETED

**Prerequisites**: Phase 1 & 2 complete (types, parsing, entity processing working)

## Overview

Build the final GraphQL response structure:
- Success responses with entity, message, and optional CASCADE
- Error responses with status, message, errors array
- Schema validation against Success type fields

## Tasks

### Task 3.1: Response Builder - Success

**File**: `fraiseql_rs/src/mutation/response_builder.rs` (NEW)

**Objective**: Build GraphQL Success response

**Key behaviors**:
- CASCADE at success level (sibling to entity, NOT nested inside entity)
- Entity field name derived from entity_type or explicit parameter
- Wrapper fields promoted to success level

**Implementation**: See `/tmp/fraiseql_rust_greenfield_implementation_plan_v2.md` lines 1105-1456 for full code

**Key test cases**:
```rust
#[test]
fn test_build_success_simple() {
    // Simple format → Success response
}

#[test]
fn test_build_success_with_cascade() {
    // CASCADE must be at success level, NOT in entity
}

#[test]
fn test_wrapper_fields_promoted() {
    // Fields from wrapper should be at success level
}
```

**Acceptance Criteria**:
- [x] Success responses have correct structure
- [x] CASCADE placed at success level (NOT nested in entity)
- [x] __typename added to response and entity
- [x] camelCase applied
- [x] Wrapper fields promoted correctly

---

### Task 3.2: Response Builder - Error

**File**: `fraiseql_rs/src/mutation/response_builder.rs` (UPDATE)

**Objective**: Build GraphQL Error response

**Key behaviors**:
- Extract error code from status string (part after ':')
- Auto-generate errors array if not in metadata
- Map status to HTTP code

**Implementation**: See plan lines 1273-1353

**Key test cases**:
```rust
#[test]
fn test_build_error() {
    // Error format → Error response
}

#[test]
fn test_error_code_extraction() {
    // "failed:validation" → code: "validation"
}

#[test]
fn test_http_code_mapping() {
    // Status → HTTP code (422, 404, 401, etc.)
}
```

**Acceptance Criteria**:
- [x] Error responses have correct structure
- [x] Error code extracted from status
- [x] HTTP codes mapped correctly
- [x] Errors array auto-generated if missing

---

### Task 3.3: Main Pipeline Integration

**File**: `fraiseql_rs/src/mutation/mod.rs` (UPDATE)

**Objective**: Create main pipeline function that ties everything together

**Implementation**:

```rust
// fraiseql_rs/src/mutation/mod.rs

mod types;
mod parser;
mod entity_processor;
mod response_builder;

pub use types::{MutationError, Result};
use parser::parse_mutation_response;
use entity_processor::process_entity_with_typename;
use response_builder::build_graphql_response;
use serde_json::json;

/// Main entry point: Build complete GraphQL mutation response
///
/// # Arguments
/// * `mutation_json` - Raw JSONB from PostgreSQL
/// * `field_name` - GraphQL field name (e.g., "createUser")
/// * `success_type` - Success type name (e.g., "CreateUserSuccess")
/// * `error_type` - Error type name (e.g., "CreateUserError")
/// * `entity_field_name` - Field name for entity (e.g., "user")
/// * `entity_type` - Entity GraphQL type (e.g., "User")
/// * `auto_camel_case` - Convert snake_case → camelCase
///
/// # Returns
/// Serialized JSON bytes ready for HTTP response
pub fn build_mutation_response(
    mutation_json: &str,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    auto_camel_case: bool,
) -> Result<Vec<u8>> {
    // STEP 1: Parse mutation response (auto-detect format)
    let response = parse_mutation_response(mutation_json, entity_type)?;

    // STEP 2: Build GraphQL response
    let graphql_response = build_graphql_response(
        &response,
        field_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        auto_camel_case,
    )?;

    // STEP 3: Serialize to bytes
    serde_json::to_vec(&graphql_response)
        .map_err(|e| MutationError::SerializationFailed(e.to_string()))
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_simple() {
        let json = r#"{"id": "123", "name": "John"}"#;
        let result = build_mutation_response(
            json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(
            response["data"]["createUser"]["__typename"],
            "CreateUserSuccess"
        );
        assert_eq!(
            response["data"]["createUser"]["user"]["__typename"],
            "User"
        );
    }

    #[test]
    fn test_end_to_end_cascade() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_type": "User",
            "entity": {"id": "123"},
            "cascade": {"updated": []}
        }"#;

        let result = build_mutation_response(
            json,
            "createUser",
            "CreateUserSuccess",
            "CreateUserError",
            Some("user"),
            Some("User"),
            true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let mutation_result = &response["data"]["createUser"];

        // CASCADE at success level
        assert!(mutation_result["cascade"].is_object());
        // NOT in entity
        assert!(mutation_result["user"]["cascade"].is_null());
    }
}
```

**Acceptance Criteria**:
- [x] End-to-end pipeline works
- [x] Simple format → Success response
- [x] Full format → Success/Error response
- [x] CASCADE never nested in entity
- [x] All integration tests pass

---

### Task 3.4: PyO3 Bindings (INITIAL)

**File**: `fraiseql_rs/src/lib.rs` (UPDATE)

**Objective**: Expose `build_mutation_response` to Python

**Implementation**:

```rust
// Add to fraiseql_rs/src/lib.rs

use pyo3::prelude::*;

#[pyfunction]
fn build_mutation_response(
    mutation_json: &str,
    field_name: &str,
    success_type: &str,
    error_type: &str,
    entity_field_name: Option<&str>,
    entity_type: Option<&str>,
    _cascade_selections: Option<&str>,  // Unused for now
    auto_camel_case: bool,
    _success_type_fields: Option<Vec<String>>,  // For future schema validation
) -> PyResult<Vec<u8>> {
    crate::mutation::build_mutation_response(
        mutation_json,
        field_name,
        success_type,
        error_type,
        entity_field_name,
        entity_type,
        auto_camel_case,
    )
    .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
}

#[pymodule]
fn fraiseql_rs(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_mutation_response, m)?)?;
    Ok(())
}
```

**Test from Python**:
```python
# Quick test
import fraiseql_rs
import json

result_bytes = fraiseql_rs.build_mutation_response(
    '{"id": "123", "name": "Test"}',
    "createUser",
    "CreateUserSuccess",
    "CreateUserError",
    "user",
    "User",
    None,
    True,
    None,
)

response = json.loads(result_bytes)
print(response)
```

**Acceptance Criteria**:
- [x] Function exposed to Python
- [x] Can be called from Python
- [x] Returns valid JSON bytes
- [x] Errors propagate correctly

---

## Phase 3 Completion Checklist

- [x] Task 3.1: Success response builder works
- [x] Task 3.2: Error response builder works
- [x] Task 3.3: Main pipeline integration complete
- [x] Task 3.4: PyO3 bindings working
- [x] All Rust tests pass: `cargo test`
- [x] No warnings: `cargo clippy`
- [x] Can call from Python successfully
- [x] Code coverage >90%
- [x] Existing Python tests still pass

**Verification**:
```bash
# Rust tests
cd fraiseql_rs
cargo test
cargo clippy

# Python binding test
cd ..
python3 -c "
import fraiseql_rs
import json
result = fraiseql_rs.build_mutation_response(
    '{\"id\": \"123\"}', 'test', 'TestSuccess', 'TestError',
    'entity', 'Entity', None, True, None
)
print(json.loads(result))
"

# Existing tests should still pass
pytest tests/unit/mutations/test_rust_executor.py -v
```

## Next Phase

Once Phase 3 is complete, proceed to **Phase 4: Python Integration** where we'll simplify Python code to use the new Rust pipeline.
