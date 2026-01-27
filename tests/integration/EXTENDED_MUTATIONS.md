# Extended Mutations Integration Tests

This document describes the extended mutations integration tests for Apollo Federation v2 with FraiseQL.

## Overview

Extended mutations are mutations on entities that are extended in a subgraph but owned by another subgraph. In federation, mutations must be executed by the authoritative subgraph (the one that owns the entity).

### Architecture

```
Orders Subgraph (extended User)
       ↓
    Mutation request: updateUser(...)
       ↓
    Orders detects User is extended (not owned)
       ↓
    HTTP propagation to Users Subgraph
       ↓
    Users Subgraph executes mutation
       ↓
    Result returned to Orders Subgraph
       ↓
    Response to client
```

## Test Scenarios

### 1. Direct User Mutation (Authoritative)
**Test**: `test_extended_mutation_user_from_authoritative_subgraph`

Tests creating a user directly in the users subgraph (the authoritative owner):
- Users subgraph is authoritative for User entity
- Direct mutations should execute locally without federation

**Example**:
```graphql
mutation {
    createUser(
        identifier: "test@example.com"
        name: "Test User"
        email: "test@example.com"
    ) {
        id
        identifier
    }
}
```

**Expected Outcome**: User created successfully in users subgraph

**Validates**: Authoritative mutations work correctly

---

### 2. Extended User Mutation (HTTP Propagation)
**Test**: `test_extended_mutation_update_user_from_extended_subgraph`

Tests updating a user from the orders subgraph (which extends User):
- Orders subgraph extends User from users subgraph
- Update request must propagate via HTTP to users subgraph
- Orders subgraph acts as a proxy

**Example**:
```graphql
mutation {
    updateUser(
        id: "<user-id>"
        name: "Updated Name"
    ) {
        id
        name
    }
}
```

**Expected Outcome**:
- Either mutation succeeds with HTTP propagation, or
- Error message indicates extended mutations not configured (acceptable in MVP)

**Validates**: HTTP federation mutation propagation or graceful error handling

---

### 3. Order Creation with User Reference
**Test**: `test_extended_mutation_create_order_with_user_reference`

Tests creating an order that references an existing user:
- Orders subgraph owns Order entity
- Order has foreign key reference to User (from extended type)
- Tests entity linking through federation

**Example**:
```graphql
mutation {
    createOrder(
        userId: "<user-id>"
        status: "pending"
        total: 99.99
    ) {
        id
        status
        user {
            id
            identifier
        }
    }
}
```

**Expected Outcome**: Order created successfully with resolved user reference

**Validates**: Entity references work across federation boundaries

---

### 4. Mutation Error Handling
**Test**: `test_extended_mutation_error_handling`

Tests error scenarios:
- Updating non-existent user
- Invalid input validation
- Error message propagation

**Expected Outcome**: Errors are returned with meaningful messages

**Validates**: Error handling is robust and informative

---

### 5. Data Consistency After Mutation
**Test**: `test_extended_mutation_data_consistency_after_mutation`

Tests data consistency after mutations:
1. Get user before mutation
2. Update user
3. Query user again and verify update persisted

**Expected Outcome**: Mutation changes persist across queries

**Validates**: Data integrity after mutations

---

### 6. Mutation Through Gateway
**Test**: `test_extended_mutation_through_gateway`

Tests executing mutations through Apollo Router gateway:
- Gateway receives mutation request
- Routes to appropriate subgraph (or handles directly)
- Returns result

**Example**:
```graphql
mutation {
    updateUser(
        id: "<user-id>"
        name: "Updated via Gateway"
    ) {
        id
        name
    }
}
```

**Expected Outcome**:
- Either mutation succeeds through gateway, or
- Error indicates gateway mutations not yet implemented (acceptable in MVP)

**Validates**: Gateway mutation routing or planned feature

---

### 7. Mutation Performance
**Test**: `test_extended_mutation_performance`

Tests mutation performance:
- Create 5 orders in sequence
- Measure total time
- Verify reasonable latency

**Expected Outcome**: Mutations complete in < 10 seconds total

**Validates**: Mutation performance is acceptable

---

## Running the Tests

### Quick Start (Recommended)

```bash
cd tests/integration
docker-compose up -d

# Wait for services
sleep 30

# Run extended mutation tests
cargo test test_extended_mutation_ --ignored --nocapture

# View results
docker-compose logs -f orders-subgraph

# Cleanup
docker-compose down -v
```

### Individual Test

```bash
cargo test test_extended_mutation_create_order_with_user_reference --ignored --nocapture
```

### With Detailed Logging

```bash
RUST_LOG=debug cargo test test_extended_mutation_ --ignored --nocapture
```

## Test Results Interpretation

### ✓ All Tests Pass
Extended mutations are fully implemented:
- Mutations on extended entities propagate via HTTP
- Data consistency is maintained
- Performance is acceptable
- Gateway can route mutations

### ⚠ HTTP Propagation Fails
This is expected in early implementations:
- Extended mutations may not yet be implemented
- Check federation.toml configuration
- Verify HTTP federation is enabled
- May be a future enhancement

### ✗ Data Consistency Fails
Indicates data integrity issue:
- Mutation executed but didn't persist
- Check database constraints
- Verify entity references are correct

### ✗ Performance Issues
Investigate if:
- Network latency is high
- Database queries are slow
- Connection pooling is needed

## Debugging Extended Mutations

### View Mutation Requests

Monitor orders subgraph logs during mutation:
```bash
docker-compose logs -f orders-subgraph
```

Look for:
- Mutation parsing errors
- Entity resolution attempts
- HTTP federation requests to users subgraph

### Check Database State

Verify mutations actually modified data:
```bash
# Users database
psql postgresql://postgres:fraiseql@localhost:5432/users
SELECT * FROM tb_user WHERE identifier = 'test@example.com';

# Orders database
psql postgresql://postgres:fraiseql@localhost:5433/orders
SELECT * FROM tb_order LIMIT 5;
```

### Manual Mutation Testing

Test a mutation manually:
```bash
curl -X POST http://localhost:4002/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createOrder(userId: \"550e8400-e29b-41d4-a716-446655440001\", status: \"pending\", total: 49.99) { id status } }"
  }'
```

### Check Federation Configuration

Verify federation is enabled:
```bash
cat tests/integration/services/orders/federation.toml
```

Should show:
- federation.enabled = true
- [[federation.subgraphs]] with User entry
- HTTP URL pointing to users subgraph

## Mutation Ownership Rules

| Entity | Subgraph | Ownership | Mutation Execution |
|--------|----------|-----------|-------------------|
| User | Users | Owned | Local (no federation) |
| User | Orders | Extended | HTTP to Users (federation) |
| Order | Orders | Owned | Local (no federation) |
| Order | Users | N/A | Not extended |

## Implementation Status

### ✅ Implemented
- Mutation tests with various scenarios
- Error handling test cases
- Data consistency validation
- Performance measurement
- Test helpers and setup

### 🚧 In Progress / Future
- HTTP mutation propagation in Rust runtime
- Extended mutation handler in orders subgraph
- Gateway mutation routing
- Composite mutation scenarios

### 📋 Future Enhancements
- Distributed transaction support
- Compensation-based saga pattern for failed mutations
- Mutation authorization checks
- Rate limiting for mutations
- Audit logging for mutations

## Query Examples

### Create Order with Validation

Check if all orders have valid user references:
```graphql
query {
    orders {
        id
        status
        user {
            id
            identifier
        }
    }
}
```

### Update Order Status

Update order status and verify user still accessible:
```graphql
mutation {
    updateOrderStatus(id: "<order-id>", status: "shipped") {
        id
        status
        total
        user {
            id
            email
        }
    }
}
```

## Performance Targets

| Operation | Target Latency | Notes |
|-----------|-----------------|-------|
| Local mutation | <50ms | Owns entity, direct DB write |
| HTTP mutation | <200ms | Extended entity, HTTP propagation |
| Mutation batch (5) | <1s | Multiple sequential mutations |
| Gateway mutation | <500ms | Through federation gateway |

## Troubleshooting

### "updateUser mutation returns error"

Possible causes:
1. User ID doesn't exist - verify with query first
2. Extended mutations not implemented yet - check Rust implementation
3. HTTP federation not configured - check federation.toml
4. User type not marked as @extends - check schema.py

### "Mutation succeeds but data doesn't change"

Possible causes:
1. Query targets wrong database/subgraph
2. Transaction didn't commit
3. Caching returning stale data
4. Multiple services have different data

### "HTTP propagation timeout"

Possible causes:
1. Users subgraph not accessible from orders subgraph
2. Firewall blocking inter-service communication
3. Users subgraph is slow or crashed
4. HTTP timeout too short

Check Docker Compose networking:
```bash
docker-compose exec orders-subgraph curl http://users-subgraph:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __typename }"}'
```

## Next Steps

After extended mutation tests pass:

1. **Task #4**: Composite key integration tests
   - Multi-tenant data isolation
   - Composite key resolution

2. **Task #5**: 3+ subgraph federation tests
   - Products subgraph (already setup)
   - 3-hop queries and mutations

3. **Task #6**: Apollo Router verification
   - Schema composition
   - Query planning
   - Mutation routing

## Contact & Support

For issues with extended mutations:
1. Check test output for specific errors
2. Review service logs: `docker-compose logs [service]`
3. Verify federation configuration: `federation.toml`
4. Check database state manually with psql
5. Try manual mutation with curl
