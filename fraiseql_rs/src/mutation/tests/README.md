# Mutation Module Tests

This directory contains organized tests for the mutation module.

## Test Organization

### format_tests.rs (15 tests)
Format parsing and response building tests:
- Simple format (entity JSONB only, no status)
- Full format (mutation_response with status field)
- Response building for both formats
- CASCADE integration
- Format detection

### validation_tests.rs (6 tests)
v1.8.0 validation as error type tests:
- NOOP returns error type (not success)
- NOT_FOUND returns error type with 404
- CONFLICT returns error type with 409
- Success with null entity returns error
- Error responses include CASCADE data

### status_tests.rs (15 tests)
Status taxonomy tests:
- Status string parsing (new, updated, deleted, noop, failed)
- Status code mapping (201, 200, 204, 422, 400, 404, 409)
- Success/Error classification

### integration_tests.rs (13 tests)
Full integration tests:
- Complete mutation response flow (parse → build → validate)
- CASCADE placement and structure
- __typename correctness for success/error types
- Format detection (simple vs full)
- Null handling, arrays, deep nesting
- Special characters in field names

### edge_case_tests.rs (9 tests)
Edge cases and corner cases:
- CASCADE never copied from entity wrapper
- __typename always present and matches entity_type
- Ambiguous status treated as simple format
- Null entities, arrays, deeply nested objects
- Special characters

### composite_tests.rs (2 tests)
PostgreSQL composite type tests:
- Parsing mutation_response as 8-field composite
- CASCADE extraction from position 7

### property_tests.rs (3 proptests)
Property-based tests:
- CASCADE never appears in entity wrapper
- Entity structure validation
- Status parsing edge cases

## Running Tests

```bash
# Run all mutation tests
cargo test mutation --lib

# Run specific test file
cargo test format_tests --lib
cargo test validation_tests --lib
cargo test status_tests --lib
cargo test integration_tests --lib
cargo test edge_case_tests --lib
cargo test composite_tests --lib
cargo test property_tests --lib

# Run single test
cargo test test_parse_simple_format --lib
```

## Adding New Tests

1. Identify the category for your test
2. Add to the appropriate `*_tests.rs` file
3. Follow existing naming conventions: `test_<feature>_<scenario>`
4. Include descriptive comments
5. Run tests to verify: `cargo test mutation --lib`

## Test Statistics

- **Total tests**: 63 tests (60 unit tests + 3 property tests)
- **Total lines**: ~1,500 lines (split across 7 files)
- **Max file size**: ~444 lines (integration_tests.rs)
- **All files**: < 600 lines ✅

---

**Last Updated**: 2024-12-09
**Related**: WP-035 Phase 2 - Test Organization
