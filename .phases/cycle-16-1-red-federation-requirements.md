# Cycle 16-1: RED Phase - Federation Requirements & Test Definition

**Cycle**: 1 of 8
**Phase**: RED (Write failing tests first)
**Duration**: ~3-4 days
**Focus**: Define federation requirements through comprehensive failing tests

---

## Objective

Write comprehensive failing tests that define:
1. Federation query handler (`_entities` query)
2. Entity representation parsing (`_Any` scalar)
3. Resolution strategy selection
4. SDL generation with federation directives

All tests must fail initially, proving they test new functionality.

---

## Requirements Definition

### Requirement 1: `_entities` Query Handler

**Description**: FraiseQL must handle the standard federation `_entities` query

**Query Structure**:
```graphql
query {
  _entities(representations: [
    { __typename: "User", id: "123" },
    { __typename: "User", id: "456" }
  ]) {
    ... on User {
      id
      email
      createdAt
    }
  }
}
```

**Expected Response**:
```json
{
  "data": {
    "_entities": [
      {
        "__typename": "User",
        "id": "123",
        "email": "user123@example.com",
        "createdAt": "2024-01-01"
      },
      {
        "__typename": "User",
        "id": "456",
        "email": "user456@example.com",
        "createdAt": "2024-01-02"
      }
    ]
  }
}
```

**Acceptance Criteria**:
- [ ] `_entities` query is recognized by GraphQL executor
- [ ] Representations array is parsed correctly
- [ ] Each representation resolves to correct entity
- [ ] Response includes requested fields
- [ ] Null entities handled gracefully
- [ ] Batching supported (100+ entities in one request)

---

### Requirement 2: `_service` Query & SDL Generation

**Description**: FraiseQL must return federation-compliant SDL via `_service` query

**Query Structure**:
```graphql
query {
  _service {
    sdl
  }
}
```

**Expected SDL (excerpt)**:
```graphql
schema {
  query: Query
}

directive @key(fields: String!, resolvable: Boolean = true) repeatable on OBJECT
directive @extends on OBJECT
directive @external on FIELD_DEFINITION
directive @requires(fields: String!) on FIELD_DEFINITION
directive @provides(fields: String!) on FIELD_DEFINITION

type Query {
  user(id: ID!): User
  _entities(representations: [_Any!]!): [_Entity]!
  _service: _Service!
}

type User @key(fields: "id", resolvable: true) {
  id: ID!
  email: String!
  createdAt: String!
}

union _Entity = User

type _Service {
  sdl: String!
}

scalar _Any
```

**Acceptance Criteria**:
- [ ] `_service` query is recognized
- [ ] SDL includes federation directives
- [ ] SDL includes `@key` directives with correct fields
- [ ] SDL includes `_Entity` union
- [ ] SDL includes `_Any` scalar
- [ ] SDL is valid GraphQL

---

### Requirement 3: Entity Representation Parsing

**Description**: Parse `_Any` scalar input correctly

**Input Examples**:
```json
{
  "__typename": "User",
  "id": "123",
  "email": "user@example.com"
}
```

**Acceptance Criteria**:
- [ ] `__typename` field extracted
- [ ] Key fields extracted (defined by `@key` directive)
- [ ] Non-key fields ignored during resolution
- [ ] Null values handled
- [ ] Type coercion works (string "123" → ID "123")
- [ ] Complex types supported (objects, arrays)

---

### Requirement 4: Resolution Strategy Selection

**Description**: Select correct resolution strategy based on entity ownership

**Strategies**:
1. **Local** - Entity defined in this subgraph
2. **DirectDatabase** - Resolved via direct database connection to another FraiseQL subgraph's database
3. **Http** - Resolved via HTTP to external subgraph

**Decision Tree**:
```
Is entity @extend?
├─ YES (not owned by this subgraph)
│  └─ Direct DB available?
│     ├─ YES → DirectDatabase
│     └─ NO  → Http
└─ NO (owned by this subgraph)
   └─ Local
```

**Acceptance Criteria**:
- [ ] Local strategy selected for non-extended entities
- [ ] Direct DB strategy selected when connection available
- [ ] HTTP strategy selected as fallback
- [ ] Strategy cached (subsequent requests reuse decision)
- [ ] Strategy errors handled gracefully

---

### Requirement 5: Performance & Batching

**Description**: Support efficient batching of entity resolutions

**Requirements**:
```
Single Query: 100 entities
├─ Latency: <8ms
├─ Query: SELECT * FROM user_view WHERE id IN (123, 456, ..., 100)
└─ Deduplication: If same key appears multiple times, query once

Parallel Requests: 10 batches of 100 entities
├─ Connection Pool Size: 10
├─ Latency: <8ms per batch
└─ Throughput: >100 batches/second
```

**Acceptance Criteria**:
- [ ] Batch query construction efficient
- [ ] Duplicate keys deduplicated
- [ ] Connection pooling implemented
- [ ] Performance meets latency targets
- [ ] No memory leaks under load

---

## Test Files to Create

### 1. Unit Tests: `tests/federation/test_entity_resolver.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Requirement 1: _entities Query Handler
    #[test]
    fn test_entities_query_recognized() {
        // Parse GraphQL query containing _entities
        // Assert: Query recognized as federation query
    }

    #[test]
    fn test_entities_representations_parsed() {
        // Parse representations array with multiple entities
        // Assert: Each representation parsed correctly
    }

    #[test]
    fn test_entities_response_format() {
        // Execute _entities query
        // Assert: Response matches federation spec
    }

    #[test]
    fn test_entities_null_handling() {
        // Request entity that doesn't exist
        // Assert: Null returned (not error)
    }

    #[test]
    fn test_entities_batch_100() {
        // Request 100 entities
        // Assert: All resolved in single batch
    }

    // Requirement 2: _service Query & SDL
    #[test]
    fn test_service_query_recognized() {
        // Parse _service query
        // Assert: Query recognized
    }

    #[test]
    fn test_sdl_includes_federation_directives() {
        // Execute _service query
        // Assert: SDL includes @key, @extends, etc.
    }

    #[test]
    fn test_sdl_includes_entity_union() {
        // Execute _service query
        // Assert: SDL includes _Entity union with correct types
    }

    #[test]
    fn test_sdl_valid_graphql() {
        // Parse SDL output
        // Assert: SDL is valid GraphQL schema
    }

    // Requirement 3: Entity Representation Parsing
    #[test]
    fn test_entity_representation_parse_typename() {
        // Parse representation with __typename
        // Assert: __typename extracted correctly
    }

    #[test]
    fn test_entity_representation_key_fields() {
        // Parse representation with key fields
        // Assert: Key fields extracted
    }

    #[test]
    fn test_entity_representation_null_values() {
        // Parse representation with null field
        // Assert: Null handled gracefully
    }

    // Requirement 4: Resolution Strategy Selection
    #[test]
    fn test_strategy_local_for_owned_entity() {
        // Query owned entity (no @extends)
        // Assert: Local strategy selected
    }

    #[test]
    fn test_strategy_caching() {
        // Select strategy for entity
        // Request same entity again
        // Assert: Cached decision reused
    }

    // Requirement 5: Performance & Batching
    #[test]
    fn test_batch_deduplication() {
        // Request same entity key twice
        // Assert: Database query executed once
    }

    #[test]
    fn test_batch_latency() {
        // Batch 100 entities
        // Assert: Latency < 8ms
    }
}
```

### 2. Integration Tests: `tests/federation/test_federation_e2e.rs`

```rust
#[cfg(test)]
mod federation_e2e_tests {
    use super::*;

    #[tokio::test]
    async fn test_federation_query_single_entity() {
        // Setup: Create FraiseQL instance with User type
        // Execute: _entities query for single user
        // Assert: User resolved correctly
    }

    #[tokio::test]
    async fn test_federation_query_batch_entities() {
        // Setup: Create FraiseQL instance
        // Execute: _entities query for 50 users
        // Assert: All users resolved, latency < 8ms
    }

    #[tokio::test]
    async fn test_federation_service_sdl() {
        // Setup: Create FraiseQL instance
        // Execute: _service query
        // Assert: SDL valid and includes federation directives
    }

    #[tokio::test]
    async fn test_federation_partial_failure() {
        // Setup: Create FraiseQL instance
        // Execute: _entities query, some entities don't exist
        // Assert: Existing entities resolved, missing are null
    }
}
```

### 3. Compliance Tests: `tests/federation/test_apollo_federation_compliance.rs`

```rust
#[cfg(test)]
mod apollo_federation_compliance_tests {
    use super::*;

    #[test]
    fn test_federation_spec_version_2() {
        // Verify: Implementation matches Apollo Federation v2 spec
        // Check: All required fields present
        // Check: All directive formats correct
    }

    #[test]
    fn test_service_query_required_fields() {
        // Execute: _service query
        // Assert: Returns exactly: { _service { sdl } }
        // Assert: No additional fields
    }

    #[test]
    fn test_entities_query_required_signature() {
        // Check: _entities signature matches spec
        // input: [_Any!]!
        // output: [_Entity]!
    }

    #[test]
    fn test_any_scalar_required() {
        // Assert: _Any scalar is defined
        // Assert: _Any accepts any JSON value
    }

    #[test]
    fn test_entity_union_required() {
        // Assert: _Entity union includes all types with @key
        // Assert: Union is ordered consistently
    }
}
```

---

## Test Execution & Verification

### Running Tests

```bash
# All federation tests
cargo test --test federation

# Specific test
cargo test --test federation test_entities_query_recognized

# With output
cargo test --test federation -- --nocapture

# Check they fail (pre-implementation)
cargo test --test federation 2>&1 | grep "FAILED"
```

### Expected Results (All Should Fail)

```
test test_entities_query_recognized ... FAILED
test test_entities_representations_parsed ... FAILED
test test_entities_response_format ... FAILED
test test_entities_null_handling ... FAILED
test test_entities_batch_100 ... FAILED
test test_service_query_recognized ... FAILED
test test_sdl_includes_federation_directives ... FAILED
test test_sdl_includes_entity_union ... FAILED
test test_sdl_valid_graphql ... FAILED
test test_entity_representation_parse_typename ... FAILED
test test_entity_representation_key_fields ... FAILED
test test_entity_representation_null_values ... FAILED
test test_strategy_local_for_owned_entity ... FAILED
test test_strategy_caching ... FAILED
test test_batch_deduplication ... FAILED
test test_batch_latency ... FAILED
test test_federation_query_single_entity ... FAILED
test test_federation_query_batch_entities ... FAILED
test test_federation_service_sdl ... FAILED
test test_federation_partial_failure ... FAILED
test test_federation_spec_version_2 ... FAILED
test test_service_query_required_fields ... FAILED
test test_entities_query_required_signature ... FAILED
test test_any_scalar_required ... FAILED
test test_entity_union_required ... FAILED

test result: FAILED. 0 passed; 25 failed
```

---

## File Structure Created

```
crates/fraiseql-core/tests/federation/
├── mod.rs                                    (module entry)
├── test_entity_resolver.rs                   (unit tests)
├── test_federation_e2e.rs                    (integration tests)
├── test_apollo_federation_compliance.rs      (compliance tests)
└── fixtures/
    ├── schema_with_key.json                  (test schema)
    └── schema_with_extends.json              (extended type schema)
```

---

## Next Phase: GREEN

Once all tests are failing (and failing for the right reasons):
1. Commit this RED phase
2. Move to GREEN phase to implement minimal code to pass tests
3. Verify tests pass
4. Continue with REFACTOR phase

---

## Validation Checklist

- [ ] All 25+ tests written
- [ ] All tests fail with clear error messages
- [ ] Each test is focused on single requirement
- [ ] Tests are not interdependent
- [ ] Test names clearly describe expected behavior
- [ ] Comments explain complex test setup
- [ ] Fixtures organized in `fixtures/` directory

---

**Status**: [~] In Progress (Writing tests)
**Next**: GREEN Phase - Implement Federation Core
