# Phase 1: Core Rust Types

**Duration**: 1-2 days
**Objective**: Define foundational type system and format detection
**Status**: COMPLETED ✅

## Overview

This phase creates the core type system that represents mutation responses in Rust. It's purely additive - we're not changing any existing Python code yet, just adding new Rust modules.

## Tasks

### Task 1.1: Core Types & Status Classification

**File**: `fraiseql_rs/src/mutation/types.rs` (NEW)

**Objective**: Define the foundational types for mutation responses

**Implementation**:

```rust
// fraiseql_rs/src/mutation/types.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Mutation response format (auto-detected)
#[derive(Debug, Clone, PartialEq)]
pub enum MutationResponse {
    /// Simple format: entity-only response (no status field)
    Simple(SimpleResponse),
    /// Full format: mutation_response with status/message/entity
    Full(FullResponse),
}

/// Simple format: Just entity JSONB
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleResponse {
    /// Entity data (entire JSONB)
    pub entity: Value,
}

/// Full mutation response format
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FullResponse {
    pub status: String,                        // REQUIRED
    pub message: String,                       // REQUIRED
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_type: Option<String>,           // PascalCase type name
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_fields: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cascade: Option<Value>,                // Just another optional field
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

/// Status classification (parsed from status string)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusKind {
    Success(String),  // success, created, updated, deleted
    Noop(String),     // noop:reason
    Error(String),    // failed:reason, not_found:reason, etc.
}

impl StatusKind {
    /// Parse status string into classification
    pub fn from_str(status: &str) -> Self {
        let status_lower = status.to_lowercase();

        // Error prefixes
        if status_lower.starts_with("failed:")
            || status_lower.starts_with("unauthorized:")
            || status_lower.starts_with("forbidden:")
            || status_lower.starts_with("not_found:")
            || status_lower.starts_with("conflict:")
            || status_lower.starts_with("timeout:")
        {
            StatusKind::Error(status.to_string())
        }
        // Noop prefix
        else if status_lower.starts_with("noop:") {
            StatusKind::Noop(status.to_string())
        }
        // Success keywords
        else if matches!(
            status_lower.as_str(),
            "success" | "created" | "updated" | "deleted" | "completed" | "ok" | "new"
        ) {
            StatusKind::Success(status.to_string())
        }
        // Unknown - default to success (backward compat)
        else {
            StatusKind::Success(status.to_string())
        }
    }

    pub fn is_success(&self) -> bool {
        matches!(self, StatusKind::Success(_))
    }

    pub fn is_error(&self) -> bool {
        matches!(self, StatusKind::Error(_))
    }

    /// Map to HTTP status code
    pub fn http_code(&self) -> u16 {
        match self {
            StatusKind::Success(_) | StatusKind::Noop(_) => 200,
            StatusKind::Error(reason) => {
                let reason_lower = reason.to_lowercase();
                if reason_lower.contains("not_found") {
                    404
                } else if reason_lower.contains("unauthorized") {
                    401
                } else if reason_lower.contains("forbidden") {
                    403
                } else if reason_lower.contains("conflict") {
                    409
                } else if reason_lower.contains("validation") || reason_lower.contains("invalid") {
                    422
                } else if reason_lower.contains("timeout") {
                    408
                } else {
                    500
                }
            }
        }
    }
}

/// Error type for mutation processing
#[derive(Debug, Clone, thiserror::Error)]
pub enum MutationError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Entity type required when entity is present")]
    MissingEntityType,

    #[error("Entity type must be PascalCase, got: {0}")]
    InvalidEntityType(String),

    #[error("Schema validation failed: {0}")]
    SchemaValidation(String),

    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
}

pub type Result<T> = std::result::Result<T, MutationError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_kind_success() {
        assert!(StatusKind::from_str("success").is_success());
        assert!(StatusKind::from_str("created").is_success());
        assert!(StatusKind::from_str("UPDATED").is_success());
    }

    #[test]
    fn test_status_kind_error() {
        let status = StatusKind::from_str("failed:validation");
        assert!(status.is_error());
        assert_eq!(status.http_code(), 422);
    }

    #[test]
    fn test_status_kind_http_codes() {
        assert_eq!(StatusKind::from_str("not_found:user").http_code(), 404);
        assert_eq!(StatusKind::from_str("unauthorized:token").http_code(), 401);
        assert_eq!(StatusKind::from_str("conflict:duplicate").http_code(), 409);
    }

    #[test]
    fn test_simple_response_serde() {
        use serde_json::json;

        let simple = SimpleResponse {
            entity: json!({"id": "123", "name": "Test"}),
        };

        let serialized = serde_json::to_string(&simple).unwrap();
        let deserialized: SimpleResponse = serde_json::from_str(&serialized).unwrap();

        assert_eq!(simple, deserialized);
    }
}
```

**Dependencies to add to Cargo.toml**:
```toml
[dependencies]
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "1.0"
```

**Verification**:
```bash
cd fraiseql_rs
cargo test mutation::types --lib
```

**Expected output**: All tests pass

**Acceptance Criteria**:
- [ ] File compiles without warnings
- [ ] All types serialize/deserialize correctly
- [ ] Status parsing handles all known formats
- [ ] HTTP code mapping correct
- [ ] Tests pass with >90% coverage

---

### Task 1.2: Format Detection Logic

**File**: `fraiseql_rs/src/mutation/parser.rs` (NEW)

**Objective**: Auto-detect Simple vs Full format and parse JSON to types

**Implementation**:

```rust
// fraiseql_rs/src/mutation/parser.rs

use crate::mutation::types::*;
use serde_json::Value;

/// Parse JSONB string into MutationResponse
///
/// Automatically detects format:
/// - Full: Has valid status field
/// - Simple: No status field OR invalid status value
pub fn parse_mutation_response(
    json_str: &str,
    default_entity_type: Option<&str>,
) -> Result<MutationResponse> {
    // Parse JSON
    let value: Value = serde_json::from_str(json_str)
        .map_err(|e| MutationError::InvalidJson(e.to_string()))?;

    // Detect format
    if is_full_format(&value) {
        parse_full(value, default_entity_type)
            .map(MutationResponse::Full)
    } else {
        parse_simple(value)
            .map(MutationResponse::Simple)
    }
}

/// Check if value is full format (has valid status field)
fn is_full_format(value: &Value) -> bool {
    if let Some(status) = value.get("status").and_then(|s| s.as_str()) {
        is_valid_mutation_status(status)
    } else {
        false
    }
}

/// Check if status string is a valid mutation status
fn is_valid_mutation_status(status: &str) -> bool {
    const VALID_PREFIXES: &[&str] = &[
        "success", "created", "updated", "deleted", "completed", "ok", "new",
        "failed:", "unauthorized:", "forbidden:", "not_found:", "conflict:", "timeout:",
        "noop:",
    ];

    let status_lower = status.to_lowercase();
    VALID_PREFIXES.iter().any(|prefix| {
        status_lower == *prefix || status_lower.starts_with(prefix)
    })
}

/// Parse simple format (entity only)
fn parse_simple(value: Value) -> Result<SimpleResponse> {
    Ok(SimpleResponse { entity: value })
}

/// Parse full mutation response format
fn parse_full(value: Value, default_entity_type: Option<&str>) -> Result<FullResponse> {
    // Required fields
    let status = value.get("status")
        .and_then(|s| s.as_str())
        .ok_or_else(|| MutationError::MissingField("status".to_string()))?
        .to_string();

    let message = value.get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("")
        .to_string();

    // Optional fields
    let entity_type = value.get("entity_type")
        .and_then(|t| t.as_str())
        .map(String::from)
        .or_else(|| default_entity_type.map(String::from));

    let entity = value.get("entity")
        .filter(|e| !e.is_null())
        .cloned();

    let updated_fields = value.get("updated_fields")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });

    // CASCADE: support both "cascade" and "_cascade" (backward compat)
    let cascade = value.get("cascade")
        .or_else(|| value.get("_cascade"))
        .filter(|c| !c.is_null())
        .cloned();

    let metadata = value.get("metadata")
        .filter(|m| !m.is_null())
        .cloned();

    Ok(FullResponse {
        status,
        message,
        entity_type,
        entity,
        updated_fields,
        cascade,
        metadata,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_simple_format() {
        let json = r#"{"id": "123", "name": "Test"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Simple(_)));
    }

    #[test]
    fn test_detect_full_format() {
        let json = r#"{"status": "success", "message": "OK"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Full(_)));
    }

    #[test]
    fn test_parse_simple() {
        let json = r#"{"id": "123", "name": "Test"}"#;
        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Simple(simple) => {
                assert_eq!(simple.entity.get("id").unwrap(), "123");
            }
            _ => panic!("Expected Simple format"),
        }
    }

    #[test]
    fn test_parse_full_with_cascade() {
        let json = r#"{
            "status": "created",
            "message": "Success",
            "entity_type": "User",
            "entity": {"id": "123", "name": "John"},
            "cascade": {"updated": []}
        }"#;

        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Full(full) => {
                assert_eq!(full.status, "created");
                assert_eq!(full.entity_type, Some("User".to_string()));
                assert!(full.cascade.is_some());
            }
            _ => panic!("Expected Full format"),
        }
    }

    #[test]
    fn test_cascade_underscore_backward_compat() {
        let json = r#"{
            "status": "success",
            "message": "OK",
            "_cascade": {"updated": []}
        }"#;

        let response = parse_mutation_response(json, None).unwrap();

        match response {
            MutationResponse::Full(full) => {
                assert!(full.cascade.is_some());
            }
            _ => panic!("Expected Full format"),
        }
    }

    #[test]
    fn test_invalid_status_treated_as_simple() {
        // status field exists but value is not a valid mutation status
        let json = r#"{"status": "some_random_field", "data": "value"}"#;
        let response = parse_mutation_response(json, None).unwrap();
        assert!(matches!(response, MutationResponse::Simple(_)));
    }
}
```

**Verification**:
```bash
cd fraiseql_rs
cargo test mutation::parser --lib
```

**Acceptance Criteria**:
- [ ] Format detection works for both formats
- [ ] Parsing handles all field types correctly
- [ ] CASCADE extracted from both `cascade` and `_cascade`
- [ ] Invalid status values treated as Simple format
- [ ] Tests cover edge cases
- [ ] Error messages clear and actionable

---

### Task 1.3: Module Setup

**File**: `fraiseql_rs/src/mutation/mod.rs` (NEW)

**Objective**: Create module structure and public API

**Implementation**:

```rust
// fraiseql_rs/src/mutation/mod.rs

mod types;
mod parser;

pub use types::{
    MutationResponse, SimpleResponse, FullResponse,
    StatusKind, MutationError, Result
};
pub use parser::parse_mutation_response;

// Placeholder for future functions
// pub use validator::validate_mutation_response;
// pub use entity_processor::process_entity;
// pub use response_builder::build_graphql_response;
```

**Update**: `fraiseql_rs/src/lib.rs`

Add to the file:
```rust
// Add to existing lib.rs
pub mod mutation;
```

**Verification**:
```bash
cd fraiseql_rs
cargo build
cargo test
```

**Acceptance Criteria**:
- [ ] Module compiles without warnings
- [ ] All tests pass
- [ ] Public API accessible from lib root
- [ ] No breaking changes to existing code

---

## Phase 1 Completion Checklist

Before moving to Phase 2:

- [ ] All Task 1.1 acceptance criteria met
- [ ] All Task 1.2 acceptance criteria met
- [ ] All Task 1.3 acceptance criteria met
- [ ] `cargo test` passes 100%
- [ ] `cargo clippy` shows no warnings
- [ ] Code coverage >90% for new modules
- [ ] Existing Python tests still pass (no changes made)

**Verification command**:
```bash
# In fraiseql_rs/
cargo test
cargo clippy
cargo tarpaulin --out Stdout --exclude-files 'tests/*'

# In root (Python tests should still pass)
pytest tests/unit/mutations/test_rust_executor.py -v
```

## Notes

- This phase is **purely additive** - no Python code changes
- All existing tests should continue to pass
- Focus on correctness over performance
- Keep functions small and testable
- Document any assumptions in comments

## Next Phase

Once Phase 1 is complete, proceed to **Phase 2: Entity Processing** which will handle entity extraction, __typename injection, and CASCADE processing.
