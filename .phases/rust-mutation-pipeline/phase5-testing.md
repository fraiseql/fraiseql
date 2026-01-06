# Phase 5: Testing & Validation

**Duration**: 3-4 days
**Objective**: Comprehensive testing, edge cases, property-based tests
**Status**: NOT STARTED

**Prerequisites**: Phase 4 complete (Python integration done, basic tests passing)

## Overview

Add comprehensive test coverage:
1. Edge cases in Rust
2. Property-based tests (invariants)
3. Integration tests with real PostgreSQL
4. Performance benchmarks (optional)

## Tasks

### Task 5.1: Rust Unit Tests - Edge Cases

**File**: `fraiseql_rs/src/mutation/tests.rs` (NEW)

**Objective**: Comprehensive edge case testing

**Test categories**:

```rust
// fraiseql_rs/src/mutation/tests.rs

#[cfg(test)]
mod edge_cases {
    use super::*;
    use serde_json::json;

    // ===== CASCADE PLACEMENT =====

    #[test]
    fn test_cascade_never_nested_in_entity() {
        let json = r#"{
            "status": "created",
            "entity_type": "Post",
            "entity": {"id": "123", "title": "Test"},
            "cascade": {"updated": []}
        }"#;

        let result = build_mutation_response(
            json, "createPost", "CreatePostSuccess", "CreatePostError",
            Some("post"), Some("Post"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
        let success = &response["data"]["createPost"];

        // CASCADE at success level
        assert!(success["cascade"].is_object());
        // NOT in entity
        assert!(success["post"]["cascade"].is_null());
    }

    // ===== __typename CORRECTNESS =====

    #[test]
    fn test_typename_always_present() {
        let json = r#"{"id": "123"}"#;
        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            Some("entity"), Some("Entity"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Success type has __typename
        assert_eq!(response["data"]["test"]["__typename"], "TestSuccess");
        // Entity has __typename
        assert_eq!(response["data"]["test"]["entity"]["__typename"], "Entity");
    }

    #[test]
    fn test_typename_matches_entity_type() {
        let json = r#"{
            "status": "success",
            "entity_type": "CustomType",
            "entity": {"id": "123"}
        }"#;

        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            Some("entity"), Some("CustomType"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // __typename must match entity_type from JSON
        assert_eq!(
            response["data"]["test"]["entity"]["__typename"],
            "CustomType"
        );
    }

    // ===== FORMAT DETECTION =====

    #[test]
    fn test_ambiguous_status_treated_as_simple() {
        // Has "status" field but value is not a valid mutation status
        let json = r#"{"status": "active", "name": "User"}"#;
        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            Some("entity"), Some("Entity"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should be treated as simple format (entity only)
        // The entire object becomes the entity
        assert_eq!(response["data"]["test"]["entity"]["status"], "active");
    }

    // ===== NULL HANDLING =====

    #[test]
    fn test_null_entity() {
        let json = r#"{
            "status": "success",
            "message": "OK",
            "entity": null
        }"#;

        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            None, None, true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should have message but no entity field
        assert_eq!(response["data"]["test"]["message"], "OK");
        assert!(response["data"]["test"].get("entity").is_none());
    }

    // ===== ARRAY ENTITIES =====

    #[test]
    fn test_array_of_entities() {
        let json = r#"[
            {"id": "1", "name": "Alice"},
            {"id": "2", "name": "Bob"}
        ]"#;

        let result = build_mutation_response(
            json, "listUsers", "ListUsersSuccess", "ListUsersError",
            Some("users"), Some("User"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Each array element should have __typename
        let users = response["data"]["listUsers"]["users"].as_array().unwrap();
        assert_eq!(users[0]["__typename"], "User");
        assert_eq!(users[1]["__typename"], "User");
    }

    // ===== DEEP NESTING =====

    #[test]
    fn test_deeply_nested_objects() {
        let json = r#"{
            "id": "1",
            "level1": {
                "level2": {
                    "level3": {
                        "value": "deep"
                    }
                }
            }
        }"#;

        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            Some("entity"), Some("Entity"), true,
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should handle deep nesting
        assert_eq!(
            response["data"]["test"]["entity"]["level1"]["level2"]["level3"]["value"],
            "deep"
        );
    }

    // ===== SPECIAL CHARACTERS =====

    #[test]
    fn test_special_characters_in_fields() {
        let json = r#"{
            "id": "123",
            "field_with_unicode": "Hello 世界",
            "field_with_quotes": "He said \"hello\""
        }"#;

        let result = build_mutation_response(
            json, "test", "TestSuccess", "TestError",
            Some("entity"), Some("Entity"), false,  // No camelCase
        ).unwrap();

        let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

        // Should preserve special characters
        assert_eq!(
            response["data"]["test"]["entity"]["field_with_unicode"],
            "Hello 世界"
        );
    }
}
```

**Acceptance Criteria**:
- [ ] CASCADE placement tested extensively
- [ ] __typename correctness verified
- [ ] Format detection edge cases covered
- [ ] Null handling tested
- [ ] Arrays handled correctly
- [ ] Deep nesting works
- [ ] Special characters preserved

---

### Task 5.2: Property-Based Tests

**File**: `fraiseql_rs/src/mutation/tests.rs` (UPDATE)

**Objective**: Test invariants that should always hold

**Add to Cargo.toml**:
```toml
[dev-dependencies]
proptest = "1.0"
```

**Implementation**:

```rust
// Add to fraiseql_rs/src/mutation/tests.rs

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn cascade_never_in_entity(
            entity_id in ".*",
            cascade_data in prop::bool::ANY,
        ) {
            let json = if cascade_data {
                format!(r#"{{
                    "status": "success",
                    "entity_type": "Test",
                    "entity": {{"id": "{}"}},
                    "cascade": {{"updated": []}}
                }}"#, entity_id)
            } else {
                format!(r#"{{
                    "status": "success",
                    "entity_type": "Test",
                    "entity": {{"id": "{}"}}
                }}"#, entity_id)
            };

            let result = build_mutation_response(
                &json, "test", "TestSuccess", "TestError",
                Some("entity"), Some("Test"), true,
            ).unwrap();

            let response: serde_json::Value = serde_json::from_slice(&result).unwrap();
            let entity = &response["data"]["test"]["entity"];

            // INVARIANT: CASCADE must NEVER be in entity
            prop_assert!(entity.get("cascade").is_none());
        }

        #[test]
        fn typename_always_present_in_success(
            entity_id in ".*",
        ) {
            let json = format!(r#"{{"id": "{}"}}"#, entity_id);

            let result = build_mutation_response(
                &json, "test", "TestSuccess", "TestError",
                Some("entity"), Some("Entity"), true,
            ).unwrap();

            let response: serde_json::Value = serde_json::from_slice(&result).unwrap();

            // INVARIANT: __typename always present
            prop_assert_eq!(
                response["data"]["test"]["__typename"].as_str(),
                Some("TestSuccess")
            );
            prop_assert_eq!(
                response["data"]["test"]["entity"]["__typename"].as_str(),
                Some("Entity")
            );
        }

        #[test]
        fn format_detection_deterministic(
            has_status in prop::bool::ANY,
            entity_data in ".*",
        ) {
            let json = if has_status {
                format!(r#"{{"status": "success", "data": "{}"}}"#, entity_data)
            } else {
                format!(r#"{{"data": "{}"}}"#, entity_data)
            };

            // Parse twice - should get same format
            let response1 = parse_mutation_response(&json, None);
            let response2 = parse_mutation_response(&json, None);

            // INVARIANT: Format detection is deterministic
            prop_assert_eq!(
                std::mem::discriminant(&response1.unwrap()),
                std::mem::discriminant(&response2.unwrap())
            );
        }
    }
}
```

**Run property tests**:
```bash
cd fraiseql_rs
cargo test property_tests
```

**Acceptance Criteria**:
- [ ] CASCADE placement invariant tested
- [ ] __typename presence invariant tested
- [ ] Format detection deterministic
- [ ] Tests pass with 100+ random inputs
- [ ] No panics or crashes

---

### Task 5.3: Python Integration Tests

**File**: `tests/integration/graphql/mutations/test_mutation_rust_pipeline.py` (NEW)

**Objective**: End-to-end tests with real PostgreSQL

**Implementation**:

```python
"""End-to-end tests for Rust mutation pipeline."""

import pytest
from graphql import execute


pytestmark = pytest.mark.integration


@pytest.mark.asyncio
async def test_simple_format_with_database(db_pool):
    """Test simple format through complete pipeline with real DB."""
    async with db_pool.connection() as conn:
        # Create test function
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_simple_mutation(input jsonb)
            RETURNS jsonb AS $$
            BEGIN
                RETURN jsonb_build_object(
                    'id', '123',
                    'name', input->>'name',
                    'email', input->>'email'
                );
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Execute via GraphQL
        # ... setup schema with mutation pointing to test_simple_mutation ...

        result = await execute(schema, mutation, variable_values={...})

        # Verify result is dict
        assert isinstance(result.data["createUser"], dict)
        assert result.data["createUser"]["user"]["__typename"] == "User"
        assert result.data["createUser"]["user"]["id"] == "123"


@pytest.mark.asyncio
async def test_full_format_with_cascade(db_pool):
    """Test full format with CASCADE through complete pipeline."""
    async with db_pool.connection() as conn:
        # Create test function with CASCADE
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_cascade_mutation(input jsonb)
            RETURNS jsonb AS $$
            BEGIN
                RETURN jsonb_build_object(
                    'status', 'created',
                    'message', 'User created',
                    'entity_type', 'User',
                    'entity', jsonb_build_object('id', '123'),
                    'cascade', jsonb_build_object(
                        'updated', jsonb_build_array(
                            jsonb_build_object(
                                'type_name', 'User',
                                'id', '123',
                                'operation', 'CREATED'
                            )
                        ),
                        'deleted', '[]'::jsonb,
                        'invalidations', '[]'::jsonb
                    )
                );
            END;
            $$ LANGUAGE plpgsql;
        """)

        # Execute via GraphQL
        result = await execute(schema, mutation, variable_values={...})

        # Verify CASCADE structure
        success = result.data["createUser"]
        assert "cascade" in success
        assert success["cascade"]["__typename"] == "Cascade"

        # CASCADE NOT in entity
        assert "cascade" not in success["user"]


@pytest.mark.asyncio
async def test_error_response_with_database(db_pool):
    """Test error response through complete pipeline."""
    async with db_pool.connection() as conn:
        # Create test function that returns error
        await conn.execute("""
            CREATE OR REPLACE FUNCTION test_error_mutation(input jsonb)
            RETURNS jsonb AS $$
            BEGIN
                RETURN jsonb_build_object(
                    'status', 'failed:validation',
                    'message', 'Email already exists'
                );
            END;
            $$ LANGUAGE plpgsql;
        """)

        result = await execute(schema, mutation, variable_values={...})

        # Verify error structure
        error = result.data["createUser"]
        assert error["__typename"] == "CreateUserError"
        assert error["status"] == "failed:validation"
        assert error["code"] == 422
        assert len(error["errors"]) > 0
```

**Acceptance Criteria**:
- [ ] Simple format tested with real DB
- [ ] Full format tested with real DB
- [ ] CASCADE tested with real DB
- [ ] Error responses tested
- [ ] All tests pass

---

### Task 5.4: Performance Benchmarks (OPTIONAL)

**File**: `fraiseql_rs/benches/mutation_benchmark.rs` (NEW)

**Only if time permits** - basic performance validation:

```rust
// fraiseql_rs/benches/mutation_benchmark.rs

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use fraiseql_rs::mutation::build_mutation_response;

fn benchmark_simple_format(c: &mut Criterion) {
    let json = r#"{"id": "123", "name": "Test"}"#;

    c.bench_function("simple_format", |b| {
        b.iter(|| {
            build_mutation_response(
                black_box(json),
                "test",
                "TestSuccess",
                "TestError",
                Some("entity"),
                Some("Entity"),
                true,
            )
        })
    });
}

fn benchmark_full_format_with_cascade(c: &mut Criterion) {
    let json = r#"{
        "status": "success",
        "entity_type": "User",
        "entity": {"id": "123", "name": "Test"},
        "cascade": {"updated": [], "deleted": []}
    }"#;

    c.bench_function("full_format_cascade", |b| {
        b.iter(|| {
            build_mutation_response(
                black_box(json),
                "test",
                "TestSuccess",
                "TestError",
                Some("user"),
                Some("User"),
                true,
            )
        })
    });
}

criterion_group!(benches, benchmark_simple_format, benchmark_full_format_with_cascade);
criterion_main!(benches);
```

**Add to Cargo.toml**:
```toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "mutation_benchmark"
harness = false
```

**Run benchmarks**:
```bash
cd fraiseql_rs
cargo bench
```

**Acceptance Criteria**:
- [ ] Simple format <1ms
- [ ] Full format with CASCADE <2ms
- [ ] No performance regression vs baseline

---

## Phase 5 Completion Checklist

- [ ] Task 5.1: Edge cases tested comprehensively
- [ ] Task 5.2: Property-based tests added
- [ ] Task 5.3: Integration tests with PostgreSQL
- [ ] Task 5.4: Performance benchmarks (optional)
- [ ] All Rust tests pass: `cargo test`
- [ ] Property tests pass: `cargo test property_tests`
- [ ] Integration tests pass: `pytest tests/integration/`
- [ ] Code coverage >95% Rust, >85% Python
- [ ] No known bugs
- [ ] Performance acceptable

**Verification**:
```bash
# Rust tests
cd fraiseql_rs
cargo test
cargo test property_tests
cargo clippy
cargo tarpaulin --out Stdout

# Python tests
cd ..
pytest tests/ -v --cov=fraiseql --cov-report=term-missing

# Optional: benchmarks
cd fraiseql_rs
cargo bench
```

## Next Phase

Once Phase 5 is complete and all tests pass, proceed to **Phase 6: Documentation** for architecture docs, migration guide, and examples.
