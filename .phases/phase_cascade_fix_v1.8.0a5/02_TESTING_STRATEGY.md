# Testing Strategy: CASCADE Fix v1.8.0-alpha.5

**Phase:** QA & Verification
**Coverage Target:** 100% of new code
**Test Pyramid:** Unit (70%) → Integration (25%) → E2E (5%)

---

## Test Categories

### 1. Unit Tests (Rust)

**Location:** `fraiseql_rs/src/mutation/tests.rs` + `postgres_composite.rs`

#### 1.1 Composite Type Parsing

```rust
#[test]
fn test_parse_8field_all_fields() {
    let json = r#"{
        "status": "created",
        "message": "Allocation created",
        "entity_id": "uuid-123",
        "entity_type": "Allocation",
        "entity": {"id": "uuid-123", "name": "Test"},
        "updated_fields": ["location_id", "machine_id"],
        "cascade": {
            "updated": [{"id": "1", "operation": "UPDATED"}],
            "deleted": [],
            "invalidations": [{"queryName": "allocations"}]
        },
        "metadata": {"extra": "data"}
    }"#;

    let result = PostgresMutationResponse::from_json(json).unwrap();

    assert_eq!(result.status, "created");
    assert_eq!(result.message, "Allocation created");
    assert_eq!(result.entity_id, Some("uuid-123".to_string()));
    assert_eq!(result.entity_type, Some("Allocation".to_string()));
    assert!(result.entity.is_object());
    assert_eq!(result.updated_fields.unwrap().len(), 2);
    assert!(result.cascade.is_some());
    assert!(result.metadata.is_some());
}

#[test]
fn test_parse_8field_minimal() {
    // Only required fields: status, message, entity
    let json = r#"{
        "status": "success",
        "message": "OK",
        "entity": {}
    }"#;

    let result = PostgresMutationResponse::from_json(json).unwrap();
    assert!(result.is_ok());
}

#[test]
fn test_parse_8field_null_optionals() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity_id": null,
        "entity_type": null,
        "entity": {},
        "updated_fields": null,
        "cascade": null,
        "metadata": null
    }"#;

    let result = PostgresMutationResponse::from_json(json).unwrap();
    assert_eq!(result.entity_id, None);
    assert_eq!(result.entity_type, None);
    assert_eq!(result.cascade, None);
}
```

#### 1.2 CASCADE Extraction

```rust
#[test]
fn test_cascade_from_position_7() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity": {},
        "cascade": {"updated": [{"id": "1"}]}
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(None);

    // CASCADE should be from Position 7
    assert!(result.cascade.is_some());
    let cascade = result.cascade.unwrap();
    assert_eq!(cascade["updated"][0]["id"], "1");
}

#[test]
fn test_cascade_null_filtered() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity": {},
        "cascade": null
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(None);

    // Null CASCADE should be filtered out (not Some(null))
    assert!(result.cascade.is_none());
}

#[test]
fn test_cascade_empty_object_preserved() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity": {},
        "cascade": {}
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(None);

    // Empty object should be preserved (valid CASCADE)
    assert!(result.cascade.is_some());
    assert!(result.cascade.unwrap().is_object());
}
```

#### 1.3 Entity Type Resolution

```rust
#[test]
fn test_entity_type_from_position_4() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity_type": "Allocation",
        "entity": {}
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(Some("Fallback"));

    // Should use Position 4, not fallback
    assert_eq!(result.entity_type, Some("Allocation".to_string()));
}

#[test]
fn test_entity_type_null_uses_none() {
    let json = r#"{
        "status": "created",
        "message": "OK",
        "entity_type": null,
        "entity": {}
    }"#;

    let pg_response = PostgresMutationResponse::from_json(json).unwrap();
    let result = pg_response.to_mutation_result(Some("Fallback"));

    // Null entity_type → None (fallback is ignored in 8-field format)
    assert_eq!(result.entity_type, None);
}
```

#### 1.4 Error Handling

```rust
#[test]
fn test_parse_invalid_json() {
    let json = "not valid json";
    let result = PostgresMutationResponse::from_json(json);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Failed to parse"));
}

#[test]
fn test_parse_wrong_structure() {
    // Missing required field 'status'
    let json = r#"{"message": "OK", "entity": {}}"#;
    let result = PostgresMutationResponse::from_json(json);
    assert!(result.is_err());
}

#[test]
fn test_parse_extra_fields_rejected() {
    // Extra field 'unknown_field' should be rejected
    let json = r#"{
        "status": "ok",
        "message": "OK",
        "entity": {},
        "unknown_field": "value"
    }"#;

    let result = PostgresMutationResponse::from_json(json);
    assert!(result.is_err());
    // serde should reject due to deny_unknown_fields
}
```

#### 1.5 Backward Compatibility

```rust
#[test]
fn test_fallback_to_simple_format() {
    // Simple format: just entity JSONB, no status field
    let simple_json = r#"{"id": "123", "name": "John"}"#;

    // Should fail 8-field parsing
    let composite_result = PostgresMutationResponse::from_json(simple_json);
    assert!(composite_result.is_err());

    // Should succeed with simple format
    let simple_result = MutationResult::from_json(simple_json, Some("User"));
    assert!(simple_result.is_ok());
    assert!(simple_result.unwrap().is_simple_format);
}

#[test]
fn test_entry_point_tries_composite_first() {
    // This tests the actual entry point logic
    let composite_json = r#"{
        "status": "created",
        "message": "OK",
        "entity": {"id": "1"}
    }"#;

    let result = build_mutation_response(
        composite_json,
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        Some("user"),
        Some("User"),
        None,
        true,
        None,
    );

    assert!(result.is_ok());
}
```

**Test Metrics:**

- Total unit tests: ~20-25
- Coverage: 100% of new code
- Run time: < 1 second

---

### 2. Integration Tests (Python)

**Location:** `tests/test_mutations_cascade.py`

#### 2.1 CASCADE Location Verification

```python
import pytest
from fraiseql import build_mutation_response

def test_cascade_at_success_level():
    """Verify CASCADE appears at success wrapper, not in entity"""
    json_data = {
        "status": "created",
        "message": "Success",
        "entity_id": "uuid-123",
        "entity_type": "Allocation",
        "entity": {"id": "uuid-123", "identifier": "test"},
        "updated_fields": ["location_id"],
        "cascade": {
            "updated": [{"id": "1", "operation": "UPDATED"}],
            "deleted": [],
            "invalidations": [{"queryName": "allocations"}]
        },
        "metadata": {}
    }

    import json
    json_str = json.dumps(json_data)

    result = build_mutation_response(
        json_str,
        "createAllocation",
        "CreateAllocationSuccess",
        "CreateAllocationError",
        "allocation",
        "Allocation",
        None,
        True,
        None
    )

    import json
    response = json.loads(result)

    mutation_data = response["data"]["createAllocation"]

    # CASCADE at success level ✅
    assert "cascade" in mutation_data
    assert mutation_data["cascade"]["__typename"] == "Cascade"
    assert "updated" in mutation_data["cascade"]

    # CASCADE NOT in entity ✅
    allocation = mutation_data["allocation"]
    assert "cascade" not in allocation
    assert allocation["__typename"] == "Allocation"


def test_cascade_null_handled():
    """Verify null CASCADE is handled correctly"""
    json_data = {
        "status": "created",
        "message": "Success",
        "entity": {"id": "1"},
        "cascade": None  # Null CASCADE
    }

    import json
    result = build_mutation_response(
        json.dumps(json_data),
        "createUser",
        "CreateUserSuccess",
        "CreateUserError",
        "user",
        "User",
        None,
        True,
        None
    )

    response = json.loads(result)
    mutation_data = response["data"]["createUser"]

    # Null CASCADE should not create cascade field
    assert "cascade" not in mutation_data or mutation_data.get("cascade") is None
```

#### 2.2 PrintOptim Integration

```python
@pytest.mark.asyncio
async def test_printoptim_allocation_cascade(db_session):
    """Test with real PrintOptim createAllocation mutation"""
    query = """
    mutation CreateAllocation($input: CreateAllocationInput!) {
        createAllocation(input: $input) {
            __typename
            ... on CreateAllocationSuccess {
                allocation {
                    __typename
                    id
                    identifier
                }
                cascade {
                    __typename
                    updated {
                        __typename
                        id
                        operation
                    }
                    invalidations {
                        queryName
                        strategy
                    }
                }
            }
        }
    }
    """

    variables = {
        "input": {
            "machineId": "test-machine-id",
            "locationId": "test-location-id",
            "startDate": "2025-01-01"
        }
    }

    result = await execute_graphql(query, variables, db_session)

    # Verify structure
    assert result["createAllocation"]["__typename"] == "CreateAllocationSuccess"

    # CASCADE at success level
    assert "cascade" in result["createAllocation"]
    cascade = result["createAllocation"]["cascade"]
    assert cascade["__typename"] == "Cascade"
    assert "updated" in cascade
    assert "invalidations" in cascade

    # CASCADE NOT in allocation
    allocation = result["createAllocation"]["allocation"]
    assert "cascade" not in allocation
```

**Test Metrics:**

- Total integration tests: ~5-10
- Requires: PostgreSQL database
- Run time: 5-10 seconds

---

### 3. End-to-End Tests

#### 3.1 Full Mutation Flow

**Test:** Run complete PrintOptim mutation with CASCADE

```python
@pytest.mark.e2e
@pytest.mark.asyncio
async def test_full_allocation_cascade_flow(db_session):
    """E2E test: Create allocation, verify CASCADE, check cache hints"""

    # Step 1: Create allocation
    result = await create_allocation_mutation(...)

    # Step 2: Verify CASCADE structure
    assert_cascade_at_correct_level(result)

    # Step 3: Verify CASCADE content
    cascade = result["cascade"]
    assert len(cascade["updated"]) > 0  # Allocation was updated
    assert len(cascade["invalidations"]) > 0  # Queries to invalidate

    # Step 4: Verify entity integrity
    allocation = result["allocation"]
    assert allocation["id"]
    assert allocation["identifier"]
    assert "cascade" not in allocation  # No nesting bug
```

**Test Metrics:**

- Total E2E tests: 2-3
- Requires: Full PrintOptim environment
- Run time: 10-30 seconds

---

## Test Execution Plan

### Local Development

```bash
# 1. Rust unit tests
cd fraiseql_rs
cargo test --all-features

# 2. Python unit tests
cd ..
pytest tests/test_mutations_cascade.py -v

# 3. Integration tests (requires PostgreSQL)
pytest tests/ -v -m "not e2e"

# 4. E2E tests (full environment)
pytest tests/ -v -m e2e
```

### CI/CD Pipeline

```yaml
# .github/workflows/test.yml
name: Test CASCADE Fix

on: [push, pull_request]

jobs:
  test-rust:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions-rs/toolchain@v1
      - run: cargo test --all-features

  test-python:
    runs-on: ubuntu-latest
    services:
      postgres:
        image: postgres:15
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-python@v4
      - run: pip install -e .
      - run: pytest tests/
```

---

## Test Coverage Requirements

### Minimum Coverage

- **Rust code:** 100% of new lines in `postgres_composite.rs`
- **Python code:** 90% of mutation handling code
- **Integration:** All major mutation types (CREATE, UPDATE, DELETE)

### Coverage Report

```bash
# Rust coverage
cargo tarpaulin --out Html

# Python coverage
pytest --cov=fraiseql --cov-report=html

# View reports
open tarpaulin-report.html
open htmlcov/index.html
```

---

## Acceptance Criteria

### Functional

- [ ] All unit tests pass
- [ ] All integration tests pass
- [ ] CASCADE at correct location in all mutations
- [ ] No CASCADE in entity objects
- [ ] Backward compatibility maintained

### Non-Functional

- [ ] Test coverage > 90%
- [ ] No flaky tests
- [ ] Test run time < 30 seconds (unit + integration)
- [ ] CI/CD pipeline green

---

## Test Data

### Valid 8-Field JSON

```json
{
  "status": "created",
  "message": "Allocation created successfully",
  "entity_id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
  "entity_type": "Allocation",
  "entity": {
    "id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
    "identifier": "test-allocation"
  },
  "updated_fields": ["location_id", "machine_id"],
  "cascade": {
    "updated": [
      {
        "__typename": "Allocation",
        "id": "4d16b78b-7d9b-495f-9094-a65b57b33916",
        "operation": "CREATED"
      }
    ],
    "deleted": [],
    "invalidations": [
      {
        "queryName": "allocations",
        "strategy": "INVALIDATE",
        "scope": "PREFIX"
      }
    ]
  },
  "metadata": {}
}
```

### Edge Cases

```json
// Null CASCADE
{"status": "ok", "message": "OK", "entity": {}, "cascade": null}

// Empty CASCADE
{"status": "ok", "message": "OK", "entity": {}, "cascade": {}}

// Minimal (only required fields)
{"status": "ok", "message": "OK", "entity": {}}

// All nulls
{"status": "ok", "message": "OK", "entity": {}, "entity_id": null, "entity_type": null, "updated_fields": null, "cascade": null, "metadata": null}
```

---

## Test Metrics Dashboard

| Category | Count | Status |
|----------|-------|--------|
| Rust Unit Tests | 20-25 | ⏳ |
| Python Integration | 5-10 | ⏳ |
| E2E Tests | 2-3 | ⏳ |
| **Total** | **~30** | ⏳ |

| Coverage | Target | Actual |
|----------|--------|--------|
| Rust | 100% | ⏳ |
| Python | 90% | ⏳ |
| Overall | 95% | ⏳ |

---

## Debugging Failed Tests

### Common Issues

**Issue 1: Compilation Error**

```
error[E0432]: unresolved import `crate::mutation::postgres_composite`
```

**Fix:** Ensure `mod postgres_composite;` added to `mod.rs`

**Issue 2: Test Failure - Wrong CASCADE Location**

```
AssertionError: 'cascade' found in allocation object
```

**Fix:** Verify `to_mutation_result()` extracts CASCADE from Position 7

**Issue 3: Parse Error**

```
Failed to parse PostgreSQL mutation_response composite type
```

**Fix:** Check JSON structure matches 8-field format exactly

---

## Next Phase: Deployment

After all tests pass, proceed to deployment phase.
