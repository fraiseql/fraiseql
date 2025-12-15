# Rust Mutation Pipeline Architecture

## Overview

The Rust mutation pipeline provides ultra-fast GraphQL mutation response building from PostgreSQL JSON data. It supports two JSON formats and handles complex GraphQL response construction including cascade data, __typename injection, and camelCase conversion.

## Architecture

### Core Components

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   PostgreSQL    │───▶│  MutationResult  │───▶│ ResponseBuilder │
│     JSON        │    │    Parser        │    │                 │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                                                       │
                                                       ▼
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │◀───│  Entity         │◀───│   Cascade       │
│   Response      │    │  Processor      │    │   Processor     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

### Key Features

- **Zero-copy JSON parsing** where possible
- **SIMD-accelerated** string operations
- **Format auto-detection** between simple and full v2 formats
- **Cascade data handling** with proper placement
- **Type-safe status taxonomy** with HTTP code mapping
- **Comprehensive error handling** with detailed messages

## JSON Format Support

### Simple Format (Auto-detected)
```json
{"id": "123", "name": "John", "email": "john@example.com"}
```
- No `status` field or invalid status values
- Entire JSON becomes the entity
- Assumes success status
- Supports `_cascade` field extraction

### Full v2 Format
```json
{
  "status": "created",
  "message": "User created successfully",
  "entity_type": "User",
  "entity": {"id": "123", "name": "John"},
  "updated_fields": ["name", "email"],
  "cascade": {
    "updated": [{"id": "user-123", "post_count": 5}],
    "deleted": [],
    "invalidations": ["User:123"]
  },
  "metadata": {"errors": [...]}
}
```
- Complete mutation response structure
- Rich status taxonomy
- Cascade data support
- Error handling with metadata

## Status Taxonomy

### Success States
- `success`, `created`, `updated`, `deleted` → HTTP 200

### Error States (with HTTP codes)
- `failed:*` → HTTP 422 (validation) or 500 (generic)
- `unauthorized:*` → HTTP 401
- `forbidden:*` → HTTP 403
- `not_found:*` → HTTP 404
- `conflict:*` → HTTP 409
- `timeout:*` → HTTP 408

### Noop States
- `noop:*` → HTTP 200 (success with no changes)

## Cascade Data Handling

Cascade data represents side effects of mutations:

```json
{
  "updated": [{"id": "user-123", "post_count": 5}],
  "deleted": ["post-456"],
  "invalidations": ["User:123", "Post:456"],
  "metadata": {"operation": "create"}
}
```

**Important**: Cascade data is **never** placed inside entity objects. It always appears at the mutation response level.

## Response Structure

### Success Response
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserSuccess",
      "message": "User created successfully",
      "user": {
        "__typename": "User",
        "id": "123",
        "firstName": "John",
        "lastName": "Doe"
      },
      "cascade": {
        "updated": [...],
        "deleted": [...],
        "invalidations": [...]
      }
    }
  }
}
```

### Error Response
```json
{
  "data": {
    "createUser": {
      "__typename": "CreateUserError",
      "status": "validation:",
      "message": "Email already exists",
      "code": 422,
      "errors": [
        {
          "field": "email",
          "code": "duplicate",
          "message": "Email already exists"
        }
      ]
    }
  }
}
```

## Performance Characteristics

- **10-50x faster** than pure Python implementation
- **Zero-copy** JSON parsing for simple formats
- **SIMD acceleration** for string transformations
- **Arena-based memory management** for reduced allocations

## Integration Points

### Python API
```python
from fraiseql_rs import build_mutation_response

result = build_mutation_response(
    mutation_json='{"id": "123", "name": "John"}',
    field_name="createUser",
    success_type="CreateUserSuccess",
    error_type="CreateUserError",
    entity_field_name="user",
    entity_type="User",
    cascade_selections=None,
    auto_camel_case=True,
    success_type_fields=None
)
```

### GraphQL Schema Integration
The pipeline integrates with GraphQL union types:
```graphql
union CreateUserResult = CreateUserSuccess | CreateUserError

type CreateUserSuccess {
  message: String!
  user: User!
  cascade: CascadeData
}

type CreateUserError {
  status: String!
  message: String!
  code: Int!
  errors: [ValidationError!]
}
```

## Testing Strategy

### Unit Tests
- Format detection edge cases
- Status taxonomy validation
- Cascade placement invariants
- Entity processing correctness
- Error handling scenarios

### Property-Based Tests
- Invariant verification (cascade never in entity)
- Deterministic format detection
- Type safety guarantees

### Integration Tests
- End-to-end response building
- Python interoperability
- Performance regression detection

### Benchmarks
- Simple format processing
- Full format with cascade
- Error response handling
- Array entity processing

## Migration from Python

See [Migration Guide](./migration-guide.md) for details on transitioning from the Python mutation pipeline to the Rust implementation.
