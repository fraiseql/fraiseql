# Phase 2: Entity Processing

**Duration**: 2-3 days
**Objective**: Handle entity extraction, __typename injection, and CASCADE processing
**Status**: COMPLETED ✅

**Prerequisites**: Phase 1 complete (core types and parsing working)

## Overview

This phase implements the core transformation logic:
1. Detect and extract entities from wrapper objects
2. Add `__typename` to all entities
3. Apply camelCase conversion
4. Process CASCADE data

Still purely Rust - no Python changes yet.

## Tasks

### Task 2.1: Entity Processor - Wrapper Detection

**File**: `fraiseql_rs/src/mutation/entity_processor.rs` (NEW)

**Objective**: Detect and extract entities from wrapper objects, handle direct entities

**Context**: PostgreSQL functions sometimes return wrapped entities:
- Wrapper: `{"post": {...}, "message": "Created"}` - entity nested inside
- Direct: `{"id": "123", "title": "..."}` - entity is the entire object

**Implementation**:

```rust
// fraiseql_rs/src/mutation/entity_processor.rs

use serde_json::{Map, Value};

/// Process entity: extract from wrapper if needed
pub fn process_entity(
    entity: &Value,
    entity_field_name: Option<&str>,
) -> ProcessedEntity {
    // Check if entity is a wrapper object
    let (actual_entity, wrapper_fields) = detect_and_extract_wrapper(
        entity,
        entity_field_name,
    );

    ProcessedEntity {
        entity: actual_entity.clone(),
        wrapper_fields,
    }
}

/// Result of entity processing
#[derive(Debug, Clone)]
pub struct ProcessedEntity {
    /// Entity data (extracted from wrapper if needed)
    pub entity: Value,
    /// Fields extracted from wrapper (if any)
    pub wrapper_fields: Map<String, Value>,
}

/// Detect if entity is a wrapper and extract nested entity
///
/// Wrapper format: {"post": {...}, "message": "..."}
/// Direct format: {"id": "123", "title": "..."}
///
/// Returns: (actual_entity, wrapper_fields)
fn detect_and_extract_wrapper(
    entity: &Value,
    entity_field_name: Option<&str>,
) -> (&Value, Map<String, Value>) {
    let mut wrapper_fields = Map::new();

    // Only process objects
    let Value::Object(entity_map) = entity else {
        return (entity, wrapper_fields);
    };

    // Check if entity contains a field matching entity_field_name
    if let Some(field_name) = entity_field_name {
        if let Some(nested_entity) = entity_map.get(field_name) {
            // This is a wrapper! Extract nested entity and other fields
            for (key, value) in entity_map {
                if key != field_name {
                    // Copy non-entity fields from wrapper
                    wrapper_fields.insert(key.clone(), value.clone());
                }
            }

            return (nested_entity, wrapper_fields);
        }
    }

    // Not a wrapper, return as-is
    (entity, wrapper_fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_detect_wrapper() {
        let entity = json!({
            "post": {"id": "123", "title": "Test"},
            "message": "Success",
            "extra": "data"
        });

        let (actual, wrapper_fields) = detect_and_extract_wrapper(
            &entity,
            Some("post"),
        );

        assert_eq!(actual.get("id").unwrap(), "123");
        assert_eq!(wrapper_fields.get("message").unwrap(), "Success");
        assert_eq!(wrapper_fields.get("extra").unwrap(), "data");
    }

    #[test]
    fn test_direct_entity() {
        let entity = json!({"id": "123", "title": "Test"});

        let (actual, wrapper_fields) = detect_and_extract_wrapper(
            &entity,
            Some("post"),
        );

        assert_eq!(actual, &entity);
        assert!(wrapper_fields.is_empty());
    }

    #[test]
    fn test_no_field_name_no_wrapper() {
        let entity = json!({"post": {"id": "123"}, "message": "Test"});

        let (actual, wrapper_fields) = detect_and_extract_wrapper(
            &entity,
            None,
        );

        // Without field name hint, treat entire object as entity
        assert_eq!(actual, &entity);
        assert!(wrapper_fields.is_empty());
    }

    #[test]
    fn test_process_entity_wrapper() {
        let entity = json!({
            "user": {"id": "123", "name": "John"},
            "count": 5
        });

        let processed = process_entity(&entity, Some("user"));

        assert_eq!(processed.entity.get("id").unwrap(), "123");
        assert_eq!(processed.wrapper_fields.get("count").unwrap(), 5);
    }
}
```

**Verification**:
```bash
cd fraiseql_rs
cargo test entity_processor::tests --lib
```

**Acceptance Criteria**:
- [ ] Wrapper detection works correctly
- [ ] Direct entities processed correctly
- [ ] Wrapper fields extracted properly
- [ ] Tests cover both wrapper and direct formats
- [ ] Edge cases handled (null, arrays, etc.)

---

### Task 2.2: __typename Injection & camelCase

**File**: `fraiseql_rs/src/mutation/entity_processor.rs` (UPDATE)

**Objective**: Add `__typename` to entities and apply camelCase conversion

**Prerequisites**:
- Check if `fraiseql_rs/src/camel_case.rs` exists (it should from existing code)
- If not, we'll create a simple implementation

**Implementation** (add to existing file):

```rust
// Add to fraiseql_rs/src/mutation/entity_processor.rs

use crate::camel_case::to_camel_case;  // Assumes this exists
use serde_json::json;

/// Add __typename to entity (recursively for nested objects)
pub fn add_typename_to_entity(
    entity: &Value,
    entity_type: &str,
    auto_camel_case: bool,
) -> Value {
    match entity {
        Value::Object(map) => {
            let mut result = Map::with_capacity(map.len() + 1);

            // Add __typename first
            result.insert("__typename".to_string(), json!(entity_type));

            // Transform keys and recursively process nested values
            for (key, val) in map {
                let transformed_key = if auto_camel_case {
                    to_camel_case(key)
                } else {
                    key.clone()
                };

                // Recursively transform nested objects (but don't add __typename)
                let transformed_val = transform_value(val, auto_camel_case);
                result.insert(transformed_key, transformed_val);
            }

            Value::Object(result)
        }
        Value::Array(arr) => {
            // For arrays, add __typename to each element
            let transformed: Vec<Value> = arr.iter()
                .map(|v| add_typename_to_entity(v, entity_type, auto_camel_case))
                .collect();
            Value::Array(transformed)
        }
        other => other.clone(),
    }
}

/// Transform value (camelCase conversion, no __typename)
fn transform_value(value: &Value, auto_camel_case: bool) -> Value {
    match value {
        Value::Object(map) => {
            let mut result = Map::new();
            for (key, val) in map {
                let transformed_key = if auto_camel_case {
                    to_camel_case(key)
                } else {
                    key.clone()
                };
                result.insert(transformed_key, transform_value(val, auto_camel_case));
            }
            Value::Object(result)
        }
        Value::Array(arr) => {
            let transformed: Vec<Value> = arr.iter()
                .map(|v| transform_value(v, auto_camel_case))
                .collect();
            Value::Array(transformed)
        }
        other => other.clone(),
    }
}

/// Update ProcessedEntity to include entity with __typename
pub fn process_entity_with_typename(
    entity: &Value,
    entity_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
) -> ProcessedEntity {
    // Extract from wrapper
    let (actual_entity, wrapper_fields) = detect_and_extract_wrapper(
        entity,
        entity_field_name,
    );

    // Add __typename
    let entity_with_typename = add_typename_to_entity(
        actual_entity,
        entity_type,
        auto_camel_case,
    );

    ProcessedEntity {
        entity: entity_with_typename,
        wrapper_fields,
    }
}

#[cfg(test)]
mod typename_tests {
    use super::*;

    #[test]
    fn test_add_typename() {
        let entity = json!({
            "id": "123",
            "first_name": "John",
            "nested": {"key": "value"}
        });

        let result = add_typename_to_entity(&entity, "User", true);

        assert_eq!(result.get("__typename").unwrap(), "User");
        assert_eq!(result.get("firstName").unwrap(), "John");  // camelCase
        assert!(result.get("first_name").is_none());  // Original removed
    }

    #[test]
    fn test_add_typename_no_camel_case() {
        let entity = json!({
            "id": "123",
            "first_name": "John"
        });

        let result = add_typename_to_entity(&entity, "User", false);

        assert_eq!(result.get("__typename").unwrap(), "User");
        assert_eq!(result.get("first_name").unwrap(), "John");  // Kept original
    }

    #[test]
    fn test_typename_in_array() {
        let entity = json!([
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ]);

        let result = add_typename_to_entity(&entity, "User", false);

        if let Value::Array(arr) = result {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].get("__typename").unwrap(), "User");
            assert_eq!(arr[1].get("__typename").unwrap(), "User");
        } else {
            panic!("Expected array");
        }
    }

    #[test]
    fn test_process_with_typename() {
        let entity = json!({
            "user": {"id": "123", "first_name": "John"},
            "count": 5
        });

        let processed = process_entity_with_typename(
            &entity,
            "User",
            Some("user"),
            true,
        );

        // Entity should have __typename and camelCase
        assert_eq!(processed.entity.get("__typename").unwrap(), "User");
        assert_eq!(processed.entity.get("firstName").unwrap(), "John");

        // Wrapper fields should be extracted
        assert_eq!(processed.wrapper_fields.get("count").unwrap(), 5);
    }
}
```

**Check for camelCase utility**:
```bash
# Check if it exists
ls fraiseql_rs/src/camel_case.rs

# If not, create simple implementation
# See Task 2.2b below
```

**Acceptance Criteria**:
- [ ] __typename added to all entities
- [ ] camelCase conversion works
- [ ] Nested objects transformed recursively
- [ ] Arrays of entities handled correctly
- [ ] Tests cover all cases

---

### Task 2.2b: camelCase Utility (IF NEEDED)

**File**: `fraiseql_rs/src/camel_case.rs` (CHECK IF EXISTS)

**Only create if the file doesn't exist**. Check first:

```bash
ls fraiseql_rs/src/camel_case.rs
```

If it exists, skip this task. If not, create:

```rust
// fraiseql_rs/src/camel_case.rs

/// Convert snake_case to camelCase
pub fn to_camel_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut capitalize_next = false;

    for c in s.chars() {
        if c == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(c.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(c);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_camel_case() {
        assert_eq!(to_camel_case("first_name"), "firstName");
        assert_eq!(to_camel_case("user_id"), "userId");
        assert_eq!(to_camel_case("already_camel"), "alreadyCamel");
        assert_eq!(to_camel_case("no_underscores"), "noUnderscores");
        assert_eq!(to_camel_case("__typename"), "__typename");
    }
}
```

And update `fraiseql_rs/src/lib.rs`:
```rust
pub mod camel_case;
```

---

### Task 2.3: CASCADE Processing

**File**: `fraiseql_rs/src/mutation/entity_processor.rs` (UPDATE)

**Objective**: Process CASCADE data (just add __typename, like any other field)

**Implementation** (add to existing file):

```rust
// Add to fraiseql_rs/src/mutation/entity_processor.rs

/// Process CASCADE data: add __typename
pub fn process_cascade(cascade: &Value, auto_camel_case: bool) -> Value {
    match cascade {
        Value::Object(map) => {
            let mut result = Map::with_capacity(map.len() + 1);

            // Add __typename for GraphQL
            result.insert("__typename".to_string(), json!("Cascade"));

            // Transform keys and recursively process values
            for (key, val) in map {
                let transformed_key = if auto_camel_case {
                    to_camel_case(key)
                } else {
                    key.clone()
                };
                result.insert(transformed_key, transform_value(val, auto_camel_case));
            }

            Value::Object(result)
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod cascade_tests {
    use super::*;

    #[test]
    fn test_process_cascade() {
        let cascade = json!({
            "updated": [],
            "deleted": [],
            "invalidations": []
        });

        let result = process_cascade(&cascade, true);

        assert_eq!(result.get("__typename").unwrap(), "Cascade");
        assert!(result.get("updated").is_some());
        assert!(result.get("deleted").is_some());
        assert!(result.get("invalidations").is_some());
    }

    #[test]
    fn test_cascade_with_data() {
        let cascade = json!({
            "updated": [
                {"type_name": "User", "id": "123"}
            ],
            "deleted": [],
            "invalidations": ["users"]
        });

        let result = process_cascade(&cascade, false);

        assert_eq!(result.get("__typename").unwrap(), "Cascade");
        let updated = result.get("updated").unwrap().as_array().unwrap();
        assert_eq!(updated.len(), 1);
    }
}
```

**Acceptance Criteria**:
- [ ] CASCADE processed with __typename
- [ ] camelCase conversion applied
- [ ] Nested CASCADE data transformed correctly
- [ ] Tests pass

---

### Task 2.4: Module Exports

**File**: `fraiseql_rs/src/mutation/mod.rs` (UPDATE)

**Objective**: Export new functions

```rust
// Update fraiseql_rs/src/mutation/mod.rs

mod types;
mod parser;
mod entity_processor;  // NEW

pub use types::{
    MutationResponse, SimpleResponse, FullResponse,
    StatusKind, MutationError, Result
};
pub use parser::parse_mutation_response;
pub use entity_processor::{  // NEW
    ProcessedEntity,
    process_entity,
    process_entity_with_typename,
    add_typename_to_entity,
    process_cascade,
};
```

**Verification**:
```bash
cd fraiseql_rs
cargo build
cargo test
cargo clippy
```

---

## Phase 2 Completion Checklist

Before moving to Phase 3:

- [ ] Task 2.1 complete: Wrapper detection works
- [ ] Task 2.2 complete: __typename injection works
- [ ] Task 2.2b complete: camelCase utility exists (or already existed)
- [ ] Task 2.3 complete: CASCADE processing works
- [ ] Task 2.4 complete: Module exports updated
- [ ] All tests pass: `cargo test`
- [ ] No warnings: `cargo clippy`
- [ ] Code coverage >90%: `cargo tarpaulin`
- [ ] Existing Python tests still pass (no changes made yet)

**Verification commands**:
```bash
# Rust tests
cd fraiseql_rs
cargo test
cargo clippy
cargo tarpaulin --out Stdout

# Python tests (should still pass - no changes made)
cd ..
pytest tests/unit/mutations/test_rust_executor.py -v
pytest tests/integration/graphql/mutations/test_mutation_patterns.py -v
```

## Notes

- Still no Python changes - purely additive Rust code
- Focus on correctness and test coverage
- Keep functions small and testable
- Document wrapper vs direct entity behavior clearly

## Next Phase

Once Phase 2 is complete, proceed to **Phase 3: Response Building** which will build GraphQL-compliant success and error responses.
