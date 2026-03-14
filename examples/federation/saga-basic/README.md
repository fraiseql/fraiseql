# FraiseQL Saga Example: Basic Order Processing

This example demonstrates how to use **sagas** in FraiseQL to orchestrate distributed transactions across Apollo Federation subgraphs.

## Scenario

An **e-commerce order processing saga** that coordinates between three microservices:

- **Users Service** (PostgreSQL): Manages user accounts
- **Orders Service** (MySQL): Manages order creation and fulfillment
- **Inventory Service** (MySQL): Manages product inventory and reservations

### The Order Saga Flow

When a customer places an order, the saga orchestrates these steps:

```

1. Verify User Exists (Users Service)
   ↓
2. Charge Payment (Payment Service - simulated)
   ↓
3. Reserve Inventory (Inventory Service)
   ↓
4. Create Order (Orders Service)
   ↓
✅ Order Confirmed

❌ If any step fails:
   Compensation runs in reverse:
   - Release inventory reservation
   - Refund charge (if step 2 completed)
   - Keep order in "failed" state for manual review
```

## Architecture

```
┌─────────────────────────────────────────┐
│         Apollo Router (Gateway)         │
│         localhost:4000/graphql          │
└────────────┬────────────────────────────┘
             │
     ┌───────┴────────┬──────────────┐
     │                │              │
┌────▼────────┐ ┌────▼────────┐ ┌──▼────────┐
│Users Service│ │Orders       │ │Inventory  │
│(Flask)      │ │Service      │ │Service    │
│Port: 4001   │ │(Flask)      │ │(Flask)    │
└────────┬────┘ │Port: 4002   │ │Port: 4003 │
         │      └────┬────────┘ └──┬────────┘
    ┌────▼──┐        │             │
    │Postgres┐       │    ┌────────▼───┐
    │(5432)  │       │    │    MySQL    │
    └────────┘       │    │ (3306)      │
                     │    │             │
                ┌────▼───┐│ ┌───────────┘
                │Orders  ││ │
                │Inventory│
                └────────┘
```

## Files

- **docker-compose.yml** - Multi-container setup with all services
- **fixtures/postgres-init.sql** - Users database schema
- **fixtures/mysql-init.sql** - Orders and inventory database schemas
- **fixtures/supergraph.graphql** - Apollo Federation schema composition
- **fixtures/router.yaml** - Apollo Router configuration
- **users-service/schema.graphql** - Users subgraph schema
- **users-service/server.py** - Users service implementation
- **orders-service/schema.graphql** - Orders subgraph schema
- **orders-service/server.py** - Orders service implementation
- **inventory-service/schema.graphql** - Inventory subgraph schema
- **inventory-service/server.py** - Inventory service implementation
- **test-saga.sh** - Integration test script

## Prerequisites

- Docker & Docker Compose (v1.29+)
- curl (for testing)
- jq (for JSON parsing in tests)

## Quick Start

### 1. Start the Example

```bash
cd examples/federation/saga-basic
docker-compose up -d
```

Wait for all services to become healthy (check Docker Desktop or run `docker-compose ps`).

### 2. Test the Saga

```bash
./test-saga.sh
```

This runs an automated test that:

- Verifies all services are healthy
- Executes a complete order saga (4 steps)
- Validates data persistence
- Tests the compensation path
- Cleans up

### 3. Manual Testing with GraphQL

```bash
# Get all users
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { users { id name email } }"
  }'

# Verify a user exists
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { verifyUserExists(userId: \"550e8400-e29b-41d4-a716-446655440001\") { id name email } }"
  }'

# Create an order
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createOrder(userId: \"550e8400-e29b-41d4-a716-446655440001\", items: [{productId: \"prod-001\", quantity: 1, price: 999.99}], chargeId: \"ch-123\", reservationId: \"res-456\") { id status total } }"
  }'
```

### 4. View Logs

```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f users-service
docker-compose logs -f orders-service
docker-compose logs -f inventory-service
```

### 5. Stop the Example

```bash
docker-compose down
```

## Understanding the Saga Pattern

### Forward Path (Success)

Each step executes in sequence. If all succeed:

```
Step 1: Verify User
  Query: verifyUserExists(userId)
  Response: User data
  ↓
Step 2: Charge Payment
  Mutation: chargeCard(userId, amount)
  Response: chargeId
  ↓
Step 3: Reserve Inventory
  Mutation: reserveItems(items, orderId)
  Response: reservationId
  ↓
Step 4: Create Order
  Mutation: createOrder(userId, items, chargeId, reservationId)
  Response: Order with status="confirmed"
```

### Compensation Path (Failure)

If step 3 (reserve inventory) fails, compensation runs in reverse:

```
Step 3 failed: Insufficient inventory

Compensation:
  (Step 2 compensation): refundCharge(chargeId)
  (Step 1 compensation): No compensation needed (verify only)

Result: Order not created, inventory unchanged, charge refunded
```

## Key Saga Concepts

### 1. **Idempotency**

Every saga step must be **idempotent** - running it twice has the same effect as running it once.

In this example:

- Payment charging uses a `chargeId` to prevent duplicate charges
- Inventory reservation uses `reservationId` for consistency
- Order creation is idempotent if the same `orderId` is used

### 2. **Compensation**

Each forward step has an optional compensation step that undoes it:

```
Forward Step          Compensation Step
─────────────────     ─────────────────
reserveItems()        releaseReservation()
chargeCard()          refundCharge()
verifyUserExists()    (no compensation needed)
createOrder()         cancelOrder()
```

### 3. **State Tracking**

The saga coordinator tracks state in PostgreSQL:

- **sagas** table: Overall saga status (PENDING, EXECUTING, COMPLETED, COMPENSATING, ROLLED_BACK)
- **saga_steps** table: Individual step status and input/output

### 4. **Recovery**

If a service crashes mid-saga:

1. Restart the service
2. Coordinator detects incomplete sagas
3. Resumes from the last completed step
4. Reruns compensation or forward steps as needed

## Saga Configuration

The saga coordinator is configured via environment variables (see docker-compose.yml):

```yaml
environment:
  # Saga settings
  FRAISEQL_SAGA_ENABLED: "true"
  FRAISEQL_SAGA_STORE_TYPE: "postgres"
  FRAISEQL_SAGA_MAX_RETRIES: "3"
  FRAISEQL_SAGA_STEP_TIMEOUT_SECONDS: "30"
  FRAISEQL_SAGA_TIMEOUT_SECONDS: "300"
  FRAISEQL_SAGA_RECOVERY_ENABLED: "true"
  FRAISEQL_SAGA_RECOVERY_POLL_INTERVAL_SECONDS: "60"
```

## Troubleshooting

### Services not starting

```bash
# Check Docker logs
docker-compose logs postgres
docker-compose logs mysql
docker-compose logs users-service

# Restart services
docker-compose restart
```

### Database connection errors

The services wait for databases to be healthy before starting. If you see connection errors:

```bash
# Check database status
docker-compose exec postgres pg_isready -U fraiseql
docker-compose exec mysql mysqladmin ping -u fraiseql -pfraise ql123

# Restart databases
docker-compose restart postgres mysql
```

### Router not composing schemas

```bash
# Check router logs
docker-compose logs apollo-router

# Verify subgraph health
curl http://localhost:4001/graphql  # Users service
curl http://localhost:4002/graphql  # Orders service
curl http://localhost:4003/graphql  # Inventory service
```

### Test failures

If `test-saga.sh` fails:

1. Check service logs: `docker-compose logs`
2. Verify all services are healthy: `docker-compose ps`
3. Wait longer for services to initialize: `sleep 10`
4. Check network connectivity: `docker network ls`

## Next Steps

### 1. Modify the Saga

Edit the saga definition in `/saga-coordination/` to add more steps or change the order.

### 2. Add More Subgraphs

- Create a new service directory
- Define schema (schema.graphql)
- Implement server (server.py)
- Add Dockerfile
- Update docker-compose.yml
- Update supergraph.graphql

### 3. Test Failure Paths

The example shows the success path. Test failure by:

- Setting product stock to 0 (inventory failure)
- Stopping a service mid-saga (crash recovery)
- Injecting errors in service responses

### 4. Monitor Sagas

Query saga state in PostgreSQL:

```bash
docker-compose exec postgres psql -U fraiseql -d fraiseql

# View all sagas
SELECT id, saga_type, status, created_at FROM sagas;

# View saga steps
SELECT saga_id, step_index, name, status FROM saga_steps ORDER BY step_index;

# View failures
SELECT * FROM sagas WHERE status = 'FAILED';
```

### 5. Production Deployment

For production, see `/docs/FEDERATION_DEPLOYMENT.md` and `/docs/SAGA_PATTERNS.md` for:

- Kubernetes deployment manifests
- High-availability saga coordination
- Distributed tracing setup
- Monitoring and alerting
- Disaster recovery procedures

## Related Documentation

- **[SAGA_GETTING_STARTED.md](../../docs/SAGA_GETTING_STARTED.md)** - Introduction to sagas
- **[SAGA_PATTERNS.md](../../docs/SAGA_PATTERNS.md)** - Common saga patterns
- **[FEDERATION_SAGAS.md](../../docs/FEDERATION_SAGAS.md)** - Sagas in Apollo Federation
- **[SAGA_API.md](../../docs/reference/SAGA_API.md)** - Complete API reference

## Performance Characteristics

On typical hardware:

| Operation | Time | Notes |
|-----------|------|-------|
| User verification | ~50ms | Local DB query |
| Inventory reserve | ~100ms | Stock update + insert |
| Order creation | ~120ms | Order + items insert |
| **Total saga** | **~270ms** | 4 steps sequential |
| **Saga + compensation** | **~350ms** | If failure in step 3 |
| **Recovery** | **~500ms** | Crash recovery, worst case |

## Example Output

When `test-saga.sh` succeeds:

```
🚀 FraiseQL Saga Example - Integration Test
============================================
ℹ Waiting for services to become healthy...
✓ postgres is healthy
✓ mysql is healthy
✓ users-service is healthy
✓ orders-service is healthy
✓ inventory-service is healthy
✓ apollo-router is healthy
✓ All services are healthy!
ℹ Test 1: Verifying test users exist...
✓ Found test users. Using user ID: 550e8400-e29b-41d4-a716-446655440001
ℹ Test 2: Executing order saga (success path)...
✓ Step 1/4: Verified user exists
✓ Step 2/4: Payment charged (ID: charge-1706490234)
✓ Step 3/4: Inventory reserved (ID: res-550e8400-e29b-41d4-a716-446655440002)
✓ Step 4/4: Order created (ID: order-1706490235, Total: $1059.97)
✓ Order saga completed successfully!
ℹ Test 3: Verifying order data...
✓ Order data verified successfully
ℹ Test 4: Testing compensation path (release reservation)...
✓ Reservation released successfully (compensation works)

✅ All tests passed!

📊 Test Summary:
  ✓ Services started and became healthy
  ✓ Users verified
  ✓ Order saga executed (4 steps)
  ✓ Order data persisted correctly
  ✓ Compensation path works

🎉 FraiseQL Saga Example is working correctly!
```

---

**Last Updated:** 2026-01-29

**Maintainer:** FraiseQL Federation Team

## License

This example is part of FraiseQL and uses the same license. See root LICENSE file.
