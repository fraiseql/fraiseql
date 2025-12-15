# Mutation Tests

This directory contains unit and integration tests for FraiseQL mutation functionality.

## Test Organization

### Field Selection Tests

Field selection/filtering ensures that auto-injected mutation response fields are only returned when explicitly requested by the client.

**Files**:
- `test_rust_field_selection.py` - Rust layer field filtering (Success/Error types)
- `test_named_fragments.py` - Named fragment support
- `test_field_selection_performance.py` - Performance benchmarks
- `../test_mutation_field_selection_integration.py` - E2E integration tests
- `../../integration/graphql/mutations/test_selection_filter.py` - Python layer selection utilities

### Coverage Matrix

| Scenario | Test File | Status |
|----------|-----------|--------|
| Success field filtering | `test_rust_field_selection.py` | ✅ Complete |
| Error field filtering | `test_rust_field_selection.py` | ✅ Complete |
| Named fragments | `test_named_fragments.py` | ✅ Complete |
| Cascade filtering | `test_rust_field_selection.py` | ✅ Complete |
| Multiple entities | `test_rust_field_selection.py` | ✅ Complete |
| E2E integration | `test_mutation_field_selection_integration.py` | ✅ Complete |
| Performance | `test_field_selection_performance.py` | ✅ Complete |

## Auto-Injected Fields (v1.8.1)

### Success Types
- `status: String!` - Operation status (e.g., "created", "updated")
- `message: String` - Human-readable message
- `id: UUID` - ID of created/updated entity (if entity field present)
- `updatedFields: [String!]` - List of fields that were updated

### Error Types
- `status: String!` - Error status (e.g., "failed:validation")
- `message: String` - Human-readable error message
- `code: Int!` - HTTP-like error code (computed from status)
- `errors: [Error!]` - Detailed error array

**Breaking Changes (v1.8.1)**:
- ❌ Success types do NOT have `errors` field (removed for semantic correctness)
- ❌ Error types do NOT have `id` or `updatedFields` fields (errors = no entity created)

## Field Selection Examples

### GraphQL Query
```graphql
mutation CreateMachine($input: CreateMachineInput!) {
    createMachine(input: $input) {
        # Only request specific fields
        status
        machine { id name }
        # Do NOT request: message, id, updatedFields
    }
}
```

### Response (with field selection)
```json
{
    "data": {
        "createMachine": {
            "__typename": "CreateMachineSuccess",
            "status": "created",
            "machine": { "id": "123", "name": "Machine X" }
            // message, id, updatedFields are NOT included
        }
    }
}
```

### Benefits
- ✅ Reduced bandwidth (only requested fields)
- ✅ GraphQL spec compliance
- ✅ Better performance (less serialization)
- ✅ Cleaner API responses

## Running Tests

```bash
# All mutation tests
uv run pytest tests/unit/mutations/ -v

# Field selection only
uv run pytest tests/unit/mutations/test_rust_field_selection.py -v
uv run pytest tests/unit/mutations/test_named_fragments.py -v

# Performance benchmarks
uv run pytest tests/unit/mutations/test_field_selection_performance.py -v -s

# E2E integration
uv run pytest tests/test_mutation_field_selection_integration.py -v
```

## Debugging Field Selection Issues

If field selection isn't working:

1. **Check FraiseQL version**: Must be v1.8.1+ (commit eaa1f78f or later)
2. **Enable debug logging**:
    ```bash
    export FRAISEQL_DEBUG_FIELD_EXTRACTION=1
    uv run pytest tests/unit/mutations/test_rust_field_selection.py -xvs
    ```
3. **Verify field extraction**: Check that `_extract_selected_fields()` returns correct set
4. **Verify Rust API**: Ensure using `build_mutation_response()` not old `build_graphql_response()`

## Performance Expectations

| Response Size | Filtering Overhead | Single Call Latency |
|---------------|-------------------|---------------------|
| Small (5 fields) | < 10% | < 1ms |
| Medium (20 fields) | < 15% | < 2ms |
| Large (100+ cascade) | < 20% | < 5ms |

If performance degrades beyond these thresholds, check `test_field_selection_performance.py` canary tests.

## Related Documentation

- FraiseQL v1.8.1 CHANGELOG: `/home/lionel/code/fraiseql/CHANGELOG.md`
- Implementation plan: `.phases/fraiseql-auto-injection-redesign/IMPLEMENTATION_PLAN.md`
- Python field extraction: `src/fraiseql/mutations/mutation_decorator.py`
- Rust field filtering: `fraiseql_rs/src/mutations/response_builder.rs`
