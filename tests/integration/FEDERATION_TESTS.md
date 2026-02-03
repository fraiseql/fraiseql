# Federation Integration Tests - 2-Subgraph Scenarios

This document describes the 2-subgraph federation integration tests for Apollo Federation v2 with FraiseQL.

## Overview

The 2-subgraph federation tests validate the core federation patterns:

- **Users Subgraph** (Port 4001): Owns User entity
- **Orders Subgraph** (Port 4002): Owns Order, extends User via HTTP federation
- **Apollo Router Gateway** (Port 4000): Composes subgraphs and executes federated queries

## Test Scenarios

### 1. Setup Validation

**Test**: `test_two_subgraph_setup_validation`

Validates that the Docker Compose environment is ready:

- Users subgraph is accessible
- Orders subgraph is accessible
- Apollo Router gateway is accessible

**Expected Outcome**: All services respond to health checks within 30 seconds

---

### 2. Direct Subgraph Queries

**Test**: `test_two_subgraph_direct_subgraph_queries`

Queries each subgraph directly (without federation):

- Query users from users subgraph
- Query orders from orders subgraph

**Expected Outcome**:

- Users subgraph returns user entities
- Orders subgraph returns order entities

**Validates**: Individual subgraphs work independently

---

### 3. HTTP Federation from Orders

**Test**: `test_two_subgraph_http_federation_from_orders`

Tests if orders subgraph can resolve User information via HTTP federation:

- Orders subgraph extends User type
- Query orders with user information
- User data comes from users subgraph via HTTP

**Expected Outcome**:

- Orders can be queried
- User information is either resolved or error message indicates configuration state

**Validates**: HTTP federation strategy configuration

---

### 4. Federation Through Gateway

**Test**: `test_two_subgraph_federation_through_gateway`

Executes a federated query through Apollo Router gateway:

- Query users from gateway (routed to users subgraph)
- Query orders for each user (routed to orders subgraph)
- Orders subgraph resolves User references via HTTP federation

**Example Query**:
```graphql
query {
    users(limit: 3) {
        id
        identifier
        email
        orders {
            id
            status
            total
        }
    }
}
```

**Expected Outcome**:

- Query succeeds without errors
- Returns users with their orders
- Federation compositing works correctly

**Validates**: Core federation functionality

---

### 5. Entity Resolution Consistency

**Test**: `test_two_subgraph_entity_resolution_consistency`

Validates that entities are consistently resolved across subgraphs:

1. Get a user ID from users subgraph
2. Query the same user through orders subgraph
3. Verify the user data matches

**Expected Outcome**:

- User can be resolved from both subgraphs
- User ID and identifier are consistent

**Validates**: Entity resolution correctness

---

### 6. Data Consistency

**Test**: `test_two_subgraph_data_consistency`

Compares data retrieved via direct and federated queries:

1. Query users directly from users subgraph
2. Query users through gateway
3. Verify data matches

**Expected Outcome**:

- User counts are the same
- User IDs are the same
- Data is consistent across query paths

**Validates**: Federation doesn't modify data

---

### 7. Performance Benchmark

**Test**: `test_two_subgraph_federation_performance`

Measures federation query latency:

- Query 10 users with their orders through gateway
- Measure total execution time
- Verify reasonable latency

**Expected Outcome**:

- Query completes in < 5 seconds
- Performance is acceptable for production use

**Validates**: Federation performance characteristics

---

## Running the Tests

### Quick Start (Recommended)

Run the automated test script:

```bash
cd tests/integration
./run_2subgraph_tests.sh
```

This script will:

1. Start Docker Compose environment
2. Wait for services to be healthy
3. Run all 2-subgraph tests
4. Display results
5. Clean up environment

Options:
```bash
./run_2subgraph_tests.sh --no-cleanup    # Keep Docker Compose running for debugging
```

### Manual Testing

If you prefer to run tests manually:

```bash
# Step 1: Start Docker Compose
cd tests/integration
docker-compose up -d

# Wait for services (30-60 seconds)
docker-compose ps

# Step 2: Run tests from project root
cd ../..
cargo test --test federation_docker_compose_integration \
    test_two_subgraph_ \
    --ignored \
    --nocapture

# Step 3: Stop services
cd tests/integration
docker-compose down -v
```

### Running Individual Tests

```bash
# Run a specific test
cargo test --test federation_docker_compose_integration \
    test_two_subgraph_federation_through_gateway \
    --ignored \
    --nocapture

# Run with detailed logging
RUST_LOG=debug cargo test --test federation_docker_compose_integration \
    test_two_subgraph_ \
    --ignored \
    --nocapture
```

### Running in CI

For CI environments, use the script:

```bash
./run_2subgraph_tests.sh
```

The script will:

- Exit with code 0 on success
- Exit with non-zero code on failure
- Provide detailed output for troubleshooting

## Test Results Interpretation

### ✓ All Tests Pass
Federation is working correctly. Subgraphs can:

- Respond independently
- Compose through gateway
- Resolve extended entities via HTTP
- Maintain data consistency

### ⚠ Some Tests Fail

Check the error messages:

**Error: "Service not ready"**
- Docker Compose services didn't start
- Verify Docker is running: `docker ps`
- Check logs: `docker-compose logs`

**Error: "Gateway federation query failed"**
- Apollo Router couldn't compose schema
- Check subgraph SDL: `curl http://localhost:4001/graphql?query={_service{sdl}}`
- Verify federation directives in schemas

**Error: "Entity resolution failed"**
- HTTP federation not configured correctly
- Check federation.toml in orders service
- Verify users subgraph URL is correct

**Error: "Query timed out"**
- Services are slow or unreachable
- Check Docker resource limits
- Verify network connectivity

## Debugging

### View Service Logs

```bash
# Users subgraph
docker-compose logs -f users-subgraph

# Orders subgraph
docker-compose logs -f orders-subgraph

# Apollo Router
docker-compose logs -f apollo-router

# All services
docker-compose logs -f
```

### Manual GraphQL Queries

Test users subgraph:
```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 1) { id identifier } }"}'
```

Test orders subgraph:
```bash
curl -X POST http://localhost:4002/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ orders(limit: 1) { id status } }"}'
```

Test gateway (federated query):
```bash
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 1) { id orders { id } } }"}'
```

### Check Schema Composition

View Apollo Router's composed schema:
```bash
curl http://localhost:4000/graphql?query={_service{sdl}} | jq '.data._service.sdl'
```

### Database Access

Access PostgreSQL databases directly:
```bash
# Users database
psql postgresql://postgres:fraiseql@localhost:5432/users

# Orders database
psql postgresql://postgres:fraiseql@localhost:5433/orders

# Query seed data
SELECT * FROM tb_user;
SELECT * FROM tb_order;
```

## Troubleshooting

### "docker-compose: command not found"
Install Docker Compose v2:
```bash
# macOS
brew install docker-compose

# Linux (as root or with sudo)
curl -L https://github.com/docker/compose/releases/download/v2.20.0/docker-compose-Linux-x86_64 \
  -o /usr/local/bin/docker-compose && chmod +x /usr/local/bin/docker-compose
```

### Ports Already in Use
If ports 4000-4003 or 5432-5434 are already in use:
```bash
# Kill existing containers
docker-compose down -v --remove-orphans

# Or use different ports (modify docker-compose.yml)
```

### Out of Disk Space
```bash
# Clean up Docker volumes and images
docker system prune -a --volumes
```

### Services Won't Start
```bash
# Check Docker daemon
docker ps

# Check logs
docker-compose logs

# Rebuild images
docker-compose build --no-cache

# Start fresh
docker-compose down -v
docker-compose up -d
```

## Next Steps

After 2-subgraph tests pass:

1. **Task #3**: Implement extended mutations integration tests
   - Test that mutations on extended entities propagate correctly
   - Validate HTTP mutation execution

2. **Task #5**: Implement 3+ subgraph federation integration tests
   - Add products subgraph
   - Test 3-hop federation (users → orders → products)

3. **Task #6**: Verify Apollo Router schema composition
   - Validate SDL contains all types and directives
   - Test gateway introspection

4. **Task #8**: Benchmark federation performance
   - Measure latency for various query patterns
   - Identify performance bottlenecks

## Architecture Reference

### 2-Subgraph Setup

```
┌─────────────────────────────────────────────┐
│         Apollo Router Gateway               │
│            (Port 4000)                      │
└──────────────┬──────────────────────────────┘
               │
        ┌──────┴─────────┐
        │                │
    ┌───▼────────┐   ┌──▼──────────┐
    │ Users      │   │ Orders      │
    │ Subgraph   │   │ Subgraph    │
    │ Port 4001  │   │ Port 4002   │
    └───┬────────┘   └──┬──────────┘
        │                │
    ┌───▼────────┐   ┌──▼──────────┐
    │ PostgreSQL │   │ PostgreSQL  │
    │ users db   │   │ orders db   │
    │ Port 5432  │   │ Port 5433   │
    └────────────┘   └─────────────┘

Federation Flow:
Users in gateway → Users subgraph (local)
Orders in gateway → Orders subgraph → User refs → Users subgraph (HTTP)
```

### Entity Resolution Pattern

```
Gateway Query: { users { orders { } } }
      ↓
Users subgraph returns: [User with id, identifier, ...]
      ↓
Orders subgraph extends User:
  - Sends back: [Order with user refs via id]
  - On field selection for user fields:
    - HTTP federation to users subgraph: { user(id: "...") { ... } }
    - Returns extended User data
```

## Contact & Support

For issues or questions about the integration tests:

1. Check the troubleshooting section above
2. Review test output and logs
3. Check Docker Compose status: `docker-compose ps`
4. Verify federation configuration: `tests/integration/services/*/federation.toml`
