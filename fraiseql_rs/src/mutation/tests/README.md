# Mutation Module Tests

This directory contains comprehensive tests for the mutation module, organized by pipeline stage to reflect the actual data flow through the mutation system.

## Pipeline-Based Test Organization

The test suite is organized around the **4-stage mutation pipeline**:

```
JSON Response → Parse → Classify → Build Response → GraphQL Response
     ↓         ↓        ↓           ↓              ↓
  Database   Stage 1  Stage 2    Stage 3       Stage 4
             Parsing  Classification Response   Integration
                       Status       Building    End-to-end
                       Taxonomy     JSON
```

### Stage 1: Parsing (`parsing.rs` - 11 tests)
**JSON → MutationResult**: Parse database responses into structured MutationResult objects.

- **Simple format parsing**: Entity JSONB only, no status wrapper
- **Full format parsing**: Complete mutation_response with status field
- **Format detection**: Automatic simple vs full format recognition
- **Error handling**: Invalid JSON, missing fields, malformed data
- **CASCADE integration**: Relationship data extraction during parsing
- **PostgreSQL composite types**: 8-field mutation_response parsing

### Stage 2: Classification (`classification.rs` - 15 tests)
**Status Taxonomy**: Classify mutation results by status and determine response types.

- **Status string parsing**: `new`, `updated`, `deleted`, `noop`, `failed:*`
- **Status code mapping**: HTTP codes (201, 200, 204, 422, 400, 404, 409)
- **Success/Error/Noop classification**: Automatic type determination
- **Case insensitivity**: Robust parsing regardless of case
- **Edge cases**: Multiple colons, empty status, unknown statuses

### Stage 3: Response Building (`response_building.rs` - 33 tests)
**MutationResult → JSON**: Build GraphQL responses from classified mutation results.

- **Simple format responses**: Direct entity JSON without wrapper
- **Full format responses**: Complete mutation_response structure
- **Auto-populated fields**: `status`, `message`, `errors` fields
- **Error array generation**: Structured error information
- **Response routing**: v1.8.0 validation as error type behavior
- **CASCADE placement**: Correct relationship data positioning
- **__typename correctness**: Proper GraphQL type names
- **Format edge cases**: Ambiguous status, null entities, arrays
- **Deep nesting**: Complex object structures
- **Special characters**: Unicode and escaped content handling

### Stage 4: Integration (`integration.rs` - 13 tests)
**End-to-End**: Complete mutation flows from database to GraphQL response.

- **Full pipeline validation**: Parse → Classify → Build → Response
- **CASCADE structure**: Relationship data integrity across pipeline
- **__typename validation**: Correct type names for all scenarios
- **Format detection**: Simple vs full format in complete flows
- **Null handling**: Missing entities, empty responses
- **Array entities**: Bulk operations and list responses
- **Deep nesting**: Complex hierarchical data structures
- **Special characters**: Unicode, quotes, and formatting

### Property-Based Tests (`properties.rs` - 3 tests)
**Invariant Testing**: Proptest-based validation of system invariants.

- **CASCADE invariants**: Relationship data never incorrectly placed
- **Entity structure**: Consistent object formatting
- **Status determinism**: Consistent parsing behavior
- **Format detection**: Reliable simple vs full classification

## Running Tests

```bash
# Run all mutation tests
cargo test mutation --lib

# Run specific pipeline stage
cargo test parsing --lib          # Stage 1: JSON parsing
cargo test classification --lib   # Stage 2: Status taxonomy
cargo test response_building --lib # Stage 3: Response building
cargo test integration --lib      # Stage 4: End-to-end
cargo test properties --lib       # Property-based tests

# Run single test
cargo test test_parse_simple_format --lib
cargo test test_success_keywords --lib
cargo test test_build_full_success_response --lib
cargo test test_build_error_response_validation --lib
```

## Adding New Tests

### 1. Identify the Pipeline Stage
- **Parsing**: Raw JSON → MutationResult conversion
- **Classification**: Status parsing and type determination
- **Response Building**: GraphQL response construction
- **Integration**: Full pipeline validation
- **Properties**: System invariant testing

### 2. Add to Appropriate File
```rust
// parsing.rs - for JSON parsing tests
#[test]
fn test_your_new_parsing_feature() {
    // Test implementation
}

// response_building.rs - for response construction tests
#[test]
fn test_your_new_response_feature() {
    // Test implementation
}
```

### 3. Follow Naming Conventions
- `test_<action>_<subject>_<condition>`: `test_parse_simple_format_with_cascade`
- `test_<feature>_<scenario>`: `test_cascade_never_nested_in_entity`
- Descriptive and specific names

### 4. Include Documentation
```rust
/// Test that CASCADE data is correctly extracted during parsing
/// when using PostgreSQL composite type responses.
#[test]
fn test_cascade_extraction_from_composite_type() {
    // Implementation
}
```

### 5. Verify Tests Pass
```bash
cargo test mutation --lib
```

## Test Statistics

- **Total tests**: 75 tests (72 unit tests + 3 property tests)
- **Total lines**: ~2,000 lines (organized across 5 files)
- **Pipeline stages**: 4-stage architecture with clear boundaries
- **File sizes**: Optimized for maintainability (< 1,000 lines each)
- **Coverage**: Comprehensive testing of all mutation pathways

## Architecture Benefits

### ✅ Clear Responsibility Boundaries
Each test file corresponds to exactly one pipeline stage, making it obvious where to add new tests and find existing ones.

### ✅ Reduced Cognitive Load
No more mixed concerns - parsing tests are separate from response building tests, eliminating confusion about test placement.

### ✅ Improved Maintainability
Changes to one pipeline stage don't affect tests for other stages, reducing merge conflicts and test maintenance overhead.

### ✅ Enhanced Developer Experience
New developers can quickly understand the mutation system by following the test organization, which mirrors the actual code architecture.

### ✅ Future-Proof Structure
As new mutation features are added, they naturally fit into the existing pipeline stages, maintaining consistent organization.

---

**Last Updated**: 2025-12-11
**Architecture**: 4-Stage Mutation Pipeline
**Test Reorganization**: Complete (Phase 4/4)
