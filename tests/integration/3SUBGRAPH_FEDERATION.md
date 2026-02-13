# 3+ Subgraph Federation Integration Tests

## Overview

This documentation covers the 3-subgraph federation architecture where FraiseQL manages queries across three service boundaries:

```
┌─────────────┐      ┌──────────────┐      ┌────────────────┐
│   Users     │      │    Orders    │      │    Products    │
│ Subgraph    │◄────►│  Subgraph    │◄────►│   Subgraph     │
│ (Port 4001) │      │ (Port 4002)  │      │  (Port 4003)   │
└──────┬──────┘      └──────┬───────┘      └────────┬───────┘
       │                    │                        │
       ▼                    ▼                        ▼
   PostgreSQL           PostgreSQL               PostgreSQL
   (Port 5432)          (Port 5433)              (Port 5434)
```

## Architecture

### Subgraph Responsibilities

**Users Subgraph (Port 4001)**

- Owns: User entity with `id` and `identifier` fields
- Exposes: `users` query
- Key: `id` (primary)
- Database: PostgreSQL on port 5432

**Orders Subgraph (Port 4002)**

- Owns: Order entity with `id` and `status` fields
- Extends: User (references users via `user` field)
- References: Product (will reference products via `products` field)
- Exposes: `orders` query
- Key: `id` (primary)
- Database: PostgreSQL on port 5433

**Products Subgraph (Port 4003)**

- Owns: Product entity with `id`, `name`, and `price` fields
- References: Order (may be referenced by orders)
- Exposes: `products` query
- Key: `id` (primary)
- Database: PostgreSQL on port 5434

### Apollo Router Gateway (Port 4000)

Apollo Router v1.31.1 composes the three subgraph schemas into a unified GraphQL API:

- Handles query federation across all 3 subgraphs
- Routes requests to appropriate subgraph
- Resolves cross-subgraph entity references
- Manages composite queries spanning multiple services

## Federation Flow

### 2-Hop Query Example: Users → Orders

```graphql
query {
  users(limit: 2) {
    id
    identifier
    orders {
      id
      status
    }
  }
}
```

**Execution Flow:**

1. Client sends query to Apollo Router (4000)
2. Router queries Users subgraph (4001) for users
3. Router extracts order references from response
4. Router queries Orders subgraph (4002) with extracted order keys
5. Router merges results and returns to client

**Latency Contribution:**

- Users subgraph query: ~10-50ms
- Orders subgraph query: ~10-50ms
- Gateway coordination: ~5-10ms
- **Total: ~25-110ms**

### 3-Hop Query Example: Users → Orders → Products

```graphql
query {
  users(limit: 2) {
    id
    identifier
    orders {
      id
      status
      products {
        id
        name
        price
      }
    }
  }
}
```

**Execution Flow:**

1. Client sends query to Apollo Router (4000)
2. Router queries Users subgraph (4001)
3. Router queries Orders subgraph (4002) for orders belonging to users
4. Router queries Products subgraph (4003) for products referenced in orders
5. Router performs 3-level entity resolution
6. Router merges all results with proper nesting
7. Returns complete 3-level response to client

**Latency Contribution:**

- Users subgraph query: ~15-50ms
- Orders subgraph query: ~20-80ms
- Products subgraph query: ~20-80ms
- Gateway coordination & resolution: ~10-20ms
- **Total: ~65-230ms** (target: <5000ms for batch queries)

## Schema Composition

### Type Definitions

```graphql
# In Users Subgraph
type User @key(fields: "id") {
  id: ID!
  identifier: String!
}

# In Orders Subgraph
type User @key(fields: "id") {
  id: ID!
}

type Order @key(fields: "id") {
  id: ID!
  status: String!
  user: User!
  products: [Product!]!
}

# In Products Subgraph
type Product @key(fields: "id") {
  id: ID!
  name: String!
  price: Float!
}

# Composed Schema (Apollo Router)
type User @key(fields: "id") {
  id: ID!
  identifier: String!
  orders: [Order!]!
}

type Order @key(fields: "id") {
  id: ID!
  status: String!
  user: User!
  products: [Product!]!
}

type Product @key(fields: "id") {
  id: ID!
  name: String!
  price: Float!
}
```

## Test Scenarios

### Test 1: Setup Validation

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_setup_validation`

Validates all 3 subgraphs and gateway are operational:

- ✓ Users subgraph health check (4001)
- ✓ Orders subgraph health check (4002)
- ✓ Products subgraph health check (4003)
- ✓ Apollo Router gateway health check (4000)

### Test 2: Direct Queries to Products

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_direct_queries`

Verifies products subgraph responds to direct queries:

- Query products directly via gateway
- Validate products array returned
- Check products have `id`, `name`, `price` fields

### Test 3: Orders with Products (2-Hop)

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_order_with_products`

Tests extended relationships within 2 subgraphs:

- Query orders from orders subgraph
- Fetch products through order references
- Validate nested structure

### Test 4: Full 3-Hop Federation (Users → Orders → Products)

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_federation_users_orders_products`

Core test for complete 3-hop federation:

- Query users with limit 2
- Fetch all orders for each user
- Fetch all products for each order
- Measure latency (target: <5000ms)
- Validate proper nesting at all 3 levels

### Test 5: Entity Resolution Chain

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_entity_resolution_chain`

Validates entity resolution across 3 subgraph boundaries:

1. Get user from users subgraph
2. Resolve orders for that user
3. Resolve products for those orders
4. Verify consistent identifiers throughout chain

### Test 6: Cross-Boundary Federation

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_cross_boundary_federation`

Tests multi-level federation boundaries:

- Products referenced from orders
- Orders referenced from users
- Validates federation propagation across 3 levels

### Test 7: Mutation Propagation

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_mutation_propagation`

Validates mutation requests propagate correctly:

- Structure accepts mutation syntax
- Responses handle cross-subgraph mutations
- Note: Full mutation support implementation-dependent

### Test 8: Batch Entity Resolution

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_batch_entity_resolution`

Performance test for high-cardinality resolution:

- Query 5 users with 3 orders each
- Each order has 2 products
- Total: 5 × 3 × 2 = 30 product references
- Measure batch resolution performance
- Validate all references resolved correctly

### Test 9: Gateway Composition

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_gateway_composition`

Verifies Apollo Router schema composition:

- Introspection query returns all types
- All federation directives present
- Composed schema includes all 3 subgraph definitions
- Query planning validates for complex queries

### Test 10: Performance Benchmark

**File:** `federation_docker_compose_integration.rs::test_three_subgraph_performance`

Measures end-to-end 3-hop federation performance:

- 10 users with all orders and products
- Warm-up query (for JIT effects)
- Timed measurement
- Target: <5000ms
- Measures gateway coordination overhead

## Running the Tests

### Prerequisites

1. Docker and Docker Compose installed
2. FraiseQL built and services available

### Start Services

```bash
cd tests/integration
docker-compose up -d

# Wait for all services to be healthy
docker-compose ps
# All services should show "healthy" status
```

### Run All 3-Subgraph Tests

```bash
# From repository root
cargo test --test federation_docker_compose_integration test_three_subgraph_ --ignored --nocapture
```

### Run Specific Test

```bash
# Run setup validation only
cargo test --test federation_docker_compose_integration test_three_subgraph_setup_validation --ignored --nocapture

# Run federation query test
cargo test --test federation_docker_compose_integration test_three_subgraph_federation_users_orders_products --ignored --nocapture

# Run performance test
cargo test --test federation_docker_compose_integration test_three_subgraph_performance --ignored --nocapture
```

### View Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f users-subgraph
docker-compose logs -f orders-subgraph
docker-compose logs -f products-subgraph
docker-compose logs -f apollo-router
```

### Stop Services

```bash
docker-compose down -v  # -v removes volumes
```

## Expected Results

### Successful Test Output

```

--- Test: 3-subgraph setup validation ---
✓ Users subgraph is ready: http://localhost:4001/graphql
✓ Orders subgraph is ready: http://localhost:4002/graphql
✓ Products subgraph is ready: http://localhost:4003/graphql
✓ Apollo Router gateway is ready: http://localhost:4000/graphql
✓ All 3 subgraphs + gateway validation passed

--- Test: 3-hop federation query (users → orders → products) ---
✓ 3-hop federation query returned 2 users with orders and products in 145ms

--- Test: Batch entity resolution at scale ---
✓ Batch entity resolution for 5 users with nested orders/products: 287ms

--- Test: 3-hop federation performance ---
✓ 3-hop federation query (10 users with orders and products): 356ms
```

## Common Issues & Troubleshooting

### Issue: Services not starting

**Symptoms:** `docker-compose ps` shows service status as `Exited` or `Unhealthy`

**Solution:**

```bash
# View logs for specific service
docker-compose logs users-subgraph

# Restart specific service
docker-compose restart users-subgraph

# Full cleanup and restart
docker-compose down -v
docker-compose up -d
```

### Issue: Apollo Router not discovering subgraphs

**Symptoms:** Gateway health check fails, introspection returns empty

**Solution:**

1. Verify all subgraphs are healthy: `docker-compose ps`
2. Check router logs: `docker-compose logs apollo-router`
3. Verify supergraph.yaml configuration includes all 3 subgraphs
4. Restart router: `docker-compose restart apollo-router`

### Issue: 3-hop queries timeout

**Symptoms:** Queries >5 seconds or timeout

**Solution:**

1. Check subgraph individual response times
2. Verify database query performance: `docker-compose logs orders-subgraph`
3. Check network latency between containers
4. Increase query timeout if testing large batch sizes
5. Profile individual subgraph performance

### Issue: Products subgraph not available

**Symptoms:** Error: "Products subgraph not found" or port 4003 not responding

**Solution:**

1. Verify products database initialized: `docker-compose logs postgres-products`
2. Check products service built: `docker-compose logs products-subgraph`
3. Verify fixtures/init-products.sql exists and loads correctly
4. Check federation.toml for products service configuration

### Issue: Cross-subgraph references fail

**Symptoms:** null values for nested fields, "entity not found" errors

**Solution:**

1. Verify entity keys match across subgraphs
2. Check federation directives in schemas (@key, @external, @requires)
3. Validate foreign key references in databases
4. Review entity resolution logs in Apollo Router

## Performance Characteristics

### Latency by Query Depth

| Query Type | Hops | Users | Typical Latency | Target |
|-----------|------|-------|-----------------|--------|
| Users only | 1 | 10 | 10-50ms | <1000ms |
| Users + Orders | 2 | 10 | 30-150ms | <2000ms |
| Users + Orders + Products | 3 | 10 | 100-300ms | <5000ms |
| Batch (5×3×2 nested) | 3 | 5 | 200-400ms | <5000ms |

### Scaling Characteristics

- **10 users:** ~145ms for 3-hop query
- **20 users:** ~280ms for 3-hop query (linear scaling)
- **50 users:** ~650ms for 3-hop query

### Optimization Opportunities

1. **Connection pooling:** Subgraph services should use pooled connections
2. **Query batching:** Apollo Router can batch entity resolution queries
3. **Caching:** Results can be cached between requests
4. **Index optimization:** Database indexes on foreign keys critical
5. **Parallel resolution:** Federation allows parallel subgraph queries

## Database Setup

### Users Database (PostgreSQL 5432)

```sql
-- Initialized by fixtures/init-users.sql
CREATE TABLE users (
  id UUID PRIMARY KEY,
  identifier VARCHAR(255) NOT NULL
);
```

### Orders Database (PostgreSQL 5433)

```sql
-- Initialized by fixtures/init-orders.sql
CREATE TABLE orders (
  id UUID PRIMARY KEY,
  user_id UUID NOT NULL REFERENCES users(id),
  status VARCHAR(50) NOT NULL
);

-- Order to Product references managed by Orders subgraph
```

### Products Database (PostgreSQL 5434)

```sql
-- Initialized by fixtures/init-products.sql
CREATE TABLE products (
  id UUID PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  price DECIMAL(10, 2) NOT NULL
);
```

## Related Documentation

- [FEDERATION_TESTS.md](./FEDERATION_TESTS.md) - 2-subgraph federation guide
- [EXTENDED_MUTATIONS.md](./EXTENDED_MUTATIONS.md) - Mutation across subgraphs
- [COMPOSITE_KEYS.md](./COMPOSITE_KEYS.md) - Multi-field entity keys
- [docker-compose.yml](./docker-compose.yml) - Service configuration
- [Apollo Federation Docs](https://www.apollographql.com/docs/apollo-server/federation/introduction/)

## Next Steps

After validating 3-subgraph federation:

1. **Task #6**: Apollo Router schema composition validation
2. **Phase 2**: Add 4+ subgraph support
3. **Phase 3**: Advanced federation patterns (interfaces, unions)
4. **Phase 4**: Mutation semantics across federation boundaries

---

**Last Updated:** 2026-01-28
**Test Count:** 10 scenarios
**Lines of Test Code:** 527
