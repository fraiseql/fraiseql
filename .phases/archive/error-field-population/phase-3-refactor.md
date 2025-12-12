# Phase 3: REFACTOR - Polish Error Field Population Implementation

## Objective

Improve code quality, extract shared helpers, add validation, and optimize the error field population implementation. All tests must continue passing throughout refactoring.

## Context

**Current State**: All 6 tests passing, but code has technical debt from GREEN phase

**Target State**: Clean, maintainable code with shared utilities, better validation, and improved error messages

## Files to Modify

1. `fraiseql_rs/src/mutation/response_builder.rs` - Extract shared code, improve structure
2. `fraiseql_rs/src/mutation/field_extractor.rs` - NEW: Shared field extraction utilities
3. `fraiseql_rs/src/mutation/type_inference.rs` - NEW: Entity type inference logic

## Implementation Steps

### Step 1: Extract Shared Field Extraction Logic

**Create New File**: `fraiseql_rs/src/mutation/field_extractor.rs`

```rust
//! Shared utilities for extracting custom fields from entity/metadata.
//!
//! Used by both success and error response builders to populate
//! custom GraphQL fields from database mutation_response data.

use serde_json::{Map, Value};
use crate::mutation::type_inference::infer_entity_type_from_field_name;

/// Configuration for field extraction
pub struct FieldExtractionConfig<'a> {
    /// Fields to extract from the source
    pub expected_fields: &'a [String],
    /// Fields to skip (already populated or reserved)
    pub reserved_fields: &'a [&'a str],
    /// Whether to transform keys to camelCase
    pub auto_camel_case: bool,
}

/// Extract custom fields from a JSON object source
///
/// # Arguments
/// * `source` - The JSON object to extract from (entity or metadata)
/// * `target` - The map to insert extracted fields into
/// * `config` - Extraction configuration
///
/// # Returns
/// Number of fields successfully extracted
pub fn extract_fields_from_source(
    source: &Value,
    target: &mut Map<String, Value>,
    config: &FieldExtractionConfig,
) -> usize {
    let Value::Object(source_map) = source else {
        return 0;
    };

    let mut extracted_count = 0;

    for field_name in config.expected_fields {
        // Skip reserved fields
        if config.reserved_fields.contains(&field_name.as_str()) {
            continue;
        }

        // Skip if already populated
        let camel_key = if config.auto_camel_case {
            crate::utils::to_camel_case(field_name)
        } else {
            field_name.clone()
        };

        if target.contains_key(&camel_key) {
            continue;
        }

        // Check if source has this field
        if let Some(field_value) = source_map.get(field_name) {
            let transformed = transform_field_value(
                field_value,
                field_name,
                config.auto_camel_case,
            );

            target.insert(camel_key, transformed);
            extracted_count += 1;
        }
    }

    extracted_count
}

/// Transform field value: add __typename if entity, apply camelCase
fn transform_field_value(
    value: &Value,
    field_name: &str,
    auto_camel_case: bool,
) -> Value {
    match value {
        Value::Object(_) => {
            let entity_type = infer_entity_type_from_field_name(field_name);
            if !entity_type.is_empty() {
                crate::mutation::response_builder::transform_entity(
                    value,
                    &entity_type,
                    auto_camel_case,
                )
            } else {
                crate::mutation::response_builder::transform_value(value, auto_camel_case)
            }
        }
        Value::Array(arr) => transform_array_value(arr, field_name, auto_camel_case),
        _ => value.clone(),
    }
}

/// Transform array value (handle arrays of entities)
fn transform_array_value(
    arr: &[Value],
    field_name: &str,
    auto_camel_case: bool,
) -> Value {
    if let Some(first) = arr.first() {
        if first.is_object() {
            let entity_type = infer_entity_type_from_field_name(field_name);
            if !entity_type.is_empty() {
                return Value::Array(
                    arr.iter()
                        .map(|item| {
                            crate::mutation::response_builder::transform_entity(
                                item,
                                &entity_type,
                                auto_camel_case,
                            )
                        })
                        .collect()
                );
            }
        }
    }

    Value::Array(
        arr.iter()
            .map(|item| {
                crate::mutation::response_builder::transform_value(item, auto_camel_case)
            })
            .collect()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_fields_skips_reserved() {
        let source = json!({
            "message": "Should be skipped",
            "custom_field": "Should be extracted"
        });

        let mut target = Map::new();
        let config = FieldExtractionConfig {
            expected_fields: &["message".to_string(), "custom_field".to_string()],
            reserved_fields: &["message"],
            auto_camel_case: false,
        };

        let count = extract_fields_from_source(&source, &mut target, &config);

        assert_eq!(count, 1);
        assert!(!target.contains_key("message"));
        assert_eq!(target.get("custom_field").unwrap(), "Should be extracted");
    }

    #[test]
    fn test_extract_fields_applies_camel_case() {
        let source = json!({"snake_case_field": "value"});

        let mut target = Map::new();
        let config = FieldExtractionConfig {
            expected_fields: &["snake_case_field".to_string()],
            reserved_fields: &[],
            auto_camel_case: true,
        };

        extract_fields_from_source(&source, &mut target, &config);

        assert!(target.contains_key("snakeCaseField"));
        assert!(!target.contains_key("snake_case_field"));
    }
}
```

### Step 2: Extract Type Inference Logic

**Create New File**: `fraiseql_rs/src/mutation/type_inference.rs`

```rust
//! Entity type inference from field names.
//!
//! Provides heuristics to infer GraphQL entity type names from
//! database field names (e.g., conflict_dns_server -> DnsServer).

/// Infer entity type from field name using common patterns
///
/// # Examples
/// ```
/// use fraiseql_rs::mutation::type_inference::infer_entity_type_from_field_name;
///
/// assert_eq!(infer_entity_type_from_field_name("conflict_dns_server"), "DnsServer");
/// assert_eq!(infer_entity_type_from_field_name("validation_errors"), "ValidationError");
/// assert_eq!(infer_entity_type_from_field_name("dns_server"), "DnsServer");
/// ```
pub fn infer_entity_type_from_field_name(field_name: &str) -> String {
    // Remove common prefixes that don't indicate type
    let cleaned = field_name
        .trim_start_matches("conflict_")
        .trim_start_matches("existing_")
        .trim_start_matches("related_")
        .trim_start_matches("current_")
        .trim_start_matches("previous_");

    // Convert plural to singular for array fields
    let singular = singularize(cleaned);

    // Convert to PascalCase (GraphQL type name convention)
    to_pascal_case(&singular)
}

/// Convert plural field names to singular
fn singularize(name: &str) -> String {
    if name.ends_with("_errors") {
        // validation_errors -> validation_error
        name.trim_end_matches('s').to_string()
    } else if name.ends_with("ies") {
        // entities -> entity
        format!("{}y", name.trim_end_matches("ies"))
    } else if name.ends_with("ves") {
        // lives -> life
        format!("{}fe", name.trim_end_matches("ves"))
    } else if name.ends_with("s") && !name.ends_with("ss") && !name.ends_with("us") {
        // dns_servers -> dns_server (but not address -> addres, status -> statu)
        name.trim_end_matches('s').to_string()
    } else {
        name.to_string()
    }
}

/// Convert snake_case to PascalCase
fn to_pascal_case(snake: &str) -> String {
    snake
        .split('_')
        .map(capitalize_first)
        .collect::<String>()
}

/// Capitalize first character of a word
fn capitalize_first(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_simple_entity() {
        assert_eq!(infer_entity_type_from_field_name("dns_server"), "DnsServer");
        assert_eq!(infer_entity_type_from_field_name("user"), "User");
    }

    #[test]
    fn test_infer_with_prefix() {
        assert_eq!(
            infer_entity_type_from_field_name("conflict_dns_server"),
            "DnsServer"
        );
        assert_eq!(
            infer_entity_type_from_field_name("existing_user"),
            "User"
        );
    }

    #[test]
    fn test_infer_plural() {
        assert_eq!(
            infer_entity_type_from_field_name("validation_errors"),
            "ValidationError"
        );
        assert_eq!(infer_entity_type_from_field_name("dns_servers"), "DnsServer");
    }

    #[test]
    fn test_infer_special_plurals() {
        assert_eq!(infer_entity_type_from_field_name("entities"), "Entity");
    }

    #[test]
    fn test_does_not_singularize_ss_ending() {
        assert_eq!(infer_entity_type_from_field_name("address"), "Address");
        assert_eq!(infer_entity_type_from_field_name("status"), "Status");
    }
}
```

### Step 3: Refactor `build_error_response()` to Use Shared Code

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Replace the custom field extraction code** from Phase 2 with:

```rust
use crate::mutation::field_extractor::{extract_fields_from_source, FieldExtractionConfig};

pub fn build_error_response(
    result: &MutationResult,
    error_type: &str,
    auto_camel_case: bool,
    error_type_fields: Option<&Vec<String>>,
) -> Result<Value, String> {
    let mut obj = Map::new();

    // Add standard fields (__typename, message, status, code, errors)
    // ... existing code from Phase 2 ...

    // Extract custom fields using shared utilities
    if let Some(expected_fields) = error_type_fields {
        let reserved = RESERVED_ERROR_FIELDS;

        // First try entity
        if let Some(entity) = &result.entity {
            let config = FieldExtractionConfig {
                expected_fields,
                reserved_fields: &reserved,
                auto_camel_case,
            };

            extract_fields_from_source(entity, &mut obj, &config);
        }

        // Then try metadata as fallback
        if let Some(metadata) = &result.metadata {
            let config = FieldExtractionConfig {
                expected_fields,
                reserved_fields: &reserved,
                auto_camel_case,
            };

            extract_fields_from_source(metadata, &mut obj, &config);
        }
    }

    Ok(Value::Object(obj))
}

/// Reserved error response fields (cannot be overridden from entity/metadata)
const RESERVED_ERROR_FIELDS: &[&str] = &["message", "status", "code", "errors", "__typename"];
```

### Step 4: Refactor `build_success_response()` to Use Shared Code

**File**: `fraiseql_rs/src/mutation/response_builder.rs`

**Update success response** to also use the shared field extractor where applicable:

```rust
pub fn build_success_response(
    result: &MutationResult,
    success_type: &str,
    entity_field_name: Option<&str>,
    auto_camel_case: bool,
    success_type_fields: Option<&Vec<String>>,
) -> Result<Value, String> {
    // ... existing code for __typename, id, message, entity ...

    // For fields NOT covered by entity_field_name, use shared extractor
    if let Some(expected_fields) = success_type_fields {
        if let Some(entity) = &result.entity {
            let reserved = &["__typename", "id", "message", entity_field_name.unwrap_or("")];

            let config = FieldExtractionConfig {
                expected_fields,
                reserved_fields: reserved,
                auto_camel_case,
            };

            extract_fields_from_source(entity, &mut obj, &config);
        }
    }

    // ... rest of existing code (updatedFields, cascade, validation) ...

    Ok(Value::Object(obj))
}
```

### Step 5: Update Module Structure

**File**: `fraiseql_rs/src/mutation/mod.rs`

**Add new modules**:

```rust
pub mod response_builder;
pub mod field_extractor;  // NEW
pub mod type_inference;   // NEW
pub mod types;
pub mod executor;
```

### Step 6: Add Validation and Warnings

**File**: `fraiseql_rs/src/mutation/field_extractor.rs`

**Add warning for missing expected fields**:

```rust
pub fn extract_fields_from_source(
    source: &Value,
    target: &mut Map<String, Value>,
    config: &FieldExtractionConfig,
) -> usize {
    // ... existing code ...

    // Optional: Log missing fields for debugging
    #[cfg(feature = "field-validation-warnings")]
    {
        let missing: Vec<_> = config.expected_fields
            .iter()
            .filter(|f| !config.reserved_fields.contains(&f.as_str()))
            .filter(|f| !source_map.contains_key(*f))
            .collect();

        if !missing.is_empty() {
            eprintln!(
                "Field extraction: {} fields not found in source: {:?}",
                missing.len(),
                missing
            );
        }
    }

    extracted_count
}
```

### Step 7: Add Integration Tests for Edge Cases

**File**: `tests/integration/mutations/test_error_field_population.py`

**Add edge case tests**:

```python
    @pytest.mark.asyncio
    async def test_error_field_null_value_preserved(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that null values in custom fields are preserved."""
        # Modify database function to return null conflict_dns_server
        async with setup_error_field_test_db.cursor() as cur:
            await cur.execute("""
                CREATE OR REPLACE FUNCTION app.create_dns_server_null_conflict(input_data JSONB)
                RETURNS app.mutation_response AS $$
                BEGIN
                    RETURN ROW(
                        'failed:conflict',
                        'Conflict occurred',
                        NULL,
                        NULL,
                        jsonb_build_object('conflict_dns_server', NULL),
                        NULL,
                        NULL,
                        NULL
                    )::app.mutation_response;
                END;
                $$ LANGUAGE plpgsql;
            """)

        # Test that null is preserved, not omitted
        # ... mutation execution ...

        assert "conflictDnsServer" in error
        assert error["conflictDnsServer"] is None

    @pytest.mark.asyncio
    async def test_error_field_array_of_primitives(
        self, setup_error_field_test_db, graphql_schema
    ):
        """Test that arrays of primitives (not entities) work correctly."""
        # Test field like affected_ids: [str]
        # Should not try to add __typename to strings
        pass
```

## Verification Commands

### Run Full Test Suite

```bash
# All error field population tests must still pass
uv run pytest tests/integration/mutations/test_error_field_population.py -v

# Expected: All tests PASS
```

### Run Existing Mutation Tests

```bash
# Ensure refactoring didn't break anything
uv run pytest tests/integration/mutations/ -v

# Expected: All tests PASS
```

### Check Code Quality

```bash
# Run Rust tests
cd fraiseql_rs
cargo test

# Run clippy for lint warnings
cargo clippy -- -D warnings

# Check formatting
cargo fmt -- --check
```

### Benchmark Performance (Optional)

```bash
# Compare performance before/after refactoring
uv run pytest tests/integration/mutations/test_error_field_population.py --benchmark-only
```

## Acceptance Criteria

- [ ] Shared `field_extractor.rs` module created
- [ ] Shared `type_inference.rs` module created
- [ ] `build_error_response()` refactored to use shared code
- [ ] `build_success_response()` refactored to use shared code (optional)
- [ ] Reserved fields defined as constants
- [ ] Type inference has comprehensive unit tests
- [ ] Field extractor has comprehensive unit tests
- [ ] Edge case integration tests added
- [ ] **All existing tests still PASS**
- [ ] **No performance regression** (mutation responses still fast)
- [ ] **Rust clippy warnings addressed**
- [ ] **Code is DRY** (no significant duplication between success/error builders)

## Code Quality Checklist

- [ ] Functions have doc comments with examples
- [ ] Complex logic has inline comments explaining why
- [ ] Magic strings replaced with named constants
- [ ] Error messages are helpful and actionable
- [ ] Unit tests cover edge cases (empty strings, special characters, etc.)
- [ ] No unwrap() calls (use proper error handling)

## DO NOT

- ❌ Change test behavior (all tests must still pass)
- ❌ Add new features (only refactor existing GREEN implementation)
- ❌ Change external API (Python callers shouldn't need updates)
- ❌ Over-engineer (keep it simple and maintainable)

## Next Phase

After refactoring is complete and all tests pass, proceed to **Phase 4: QA** for cross-version testing, documentation, and changelog updates.
