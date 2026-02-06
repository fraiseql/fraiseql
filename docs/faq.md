<!-- Skip to main content -->
---

title: FraiseQL Frequently Asked Questions (FAQ)
description: 1. [General Questions](#general-questions)
keywords: []
tags: ["documentation", "reference"]
---

# FraiseQL Frequently Asked Questions (FAQ)

**Last Updated**: 2026-01-29

---

## Table of Contents

1. [General Questions](#general-questions)
2. [Federation Questions](#federation-questions)
3. [Saga Questions](#saga-questions)
4. [Performance & Optimization](#performance--optimization)
5. [Deployment & Operations](#deployment--operations)
6. [Troubleshooting](#troubleshooting)

---

## General Questions

### Q: What is FraiseQL?

A: FraiseQL is a compiled GraphQL execution engine that transforms schema definitions into optimized SQL at build time. It eliminates runtime overhead by pre-compiling queries, enabling high-performance, deterministic GraphQL execution.

**Key Features**:

- Compiled GraphQL execution (no runtime interpretation)
- Apollo Federation v2 support
- Saga orchestration for distributed transactions
- Multi-database support (PostgreSQL, MySQL, SQLite)
- Python/TypeScript schema authoring
- Zero-cost abstractions in Rust

---

### Q: Is FraiseQL production-ready?

A: **Yes**, FraiseQL v2 is fully production-ready.

Current status:

- ✅ All 6 development phases complete
- ✅ 572 commits, 2,293+ tests passing
- ✅ All core federation features implemented and tested
- ✅ Comprehensive documentation (250+ files, 60,000+ lines)
- ✅ Security audit completed with hardening applied
- ✅ Performance optimized and benchmarked

See [RELEASE_NOTES.md](../RELEASE_NOTES.md) for release details.

---

### Q: What databases does FraiseQL support?

A: FraiseQL currently supports:

- **PostgreSQL** (primary, all features)
- **MySQL** (secondary, core features)
- **SQLite** (local development, testing)
- **SQL Server** (enterprise)

All databases can be used together in federation. See [multi-database consistency](integrations/federation/sagas.md#multi-database-consistency).

---

### Q: Can I use FraiseQL with existing databases?

A: **Yes**. FraiseQL works with existing schema by mapping GraphQL types to database tables via the `@key` directive. Always use UUID v4 for entity identifiers (see [Naming Patterns: id: UUID v4](reference/naming-patterns.md#1-id-uuid-v4-graphql-primary-identifier)).

```python
<!-- Code example in Python -->
from uuid import UUID
from FraiseQL.federation import federated_type, key

@federated_type
@key(fields="id")
class User:
    id: UUID                 # ✅ UUID v4 for public GraphQL ID
    name: str
    email: str
```text
<!-- Code example in TEXT -->

This maps to your existing `tb_user` table (singular, following [Naming Patterns](reference/naming-patterns.md)).

---

## Federation Questions

### Q: What is Apollo Federation v2?

A: Apollo Federation v2 is a specification for composing multiple GraphQL services into a single gateway. It allows:

- **Type extensions** across services
- **Reference resolution** by key fields
- **Field requirements** (@requires directive)
- **Field availability** (@provides directive)
- **Service independence** while maintaining a unified API

See [federation sagas](integrations/federation/sagas.md) for FraiseQL implementation.

---

### Q: How does entity resolution work?

A: Entity resolution maps keys from the gateway to actual database rows in each service:

```text
<!-- Code example in TEXT -->
Gateway Query: { user(id: "123") { name orders { total } } }
                                              ↓
User Service:   SELECT * FROM users WHERE id = '123'
                Returns: { id: "123", name: "Alice", ... }
                                              ↓
Orders Service: SELECT * FROM orders WHERE user_id = '123'
                Returns: [{ id: "1", user_id: "123", total: 100 }]
```text
<!-- Code example in TEXT -->

Performance: <5ms local, <20ms direct DB, <200ms HTTP.

---

### Q: Can I use @requires and @provides directives?

A: **Yes**. Both are fully implemented and runtime-enforced:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!
  email: String!
  profile: String! @requires(fields: "email")  # Profile needs email
}

type Order @key(fields: "id") {
  id: ID!
  userId: ID! @provides(fields: "User.id")  # Provides User reference
}
```text
<!-- Code example in TEXT -->

See [runtime directive enforcement](integrations/federation/sagas.md#runtime-directive-enforcement).

---

### Q: How do I compose multiple services?

A: Use Apollo Router with a supergraph:

```yaml
<!-- Code example in YAML -->
# docker-compose.yml
services:
  users-service:
    ports: ["4001:4000"]

  orders-service:
    ports: ["4002:4000"]

  apollo-router:
    volumes:
      - ./supergraph.graphql:/etc/router/supergraph.graphql
    ports: ["4000:4000"]
```text
<!-- Code example in TEXT -->

See [saga-basic example](../examples/federation/saga-basic/).

---

## Saga Questions

### Q: What are sagas?

A: Sagas are distributed transactions that coordinate operations across multiple services. They handle failure by automatically reversing (compensating) completed steps.

```text
<!-- Code example in TEXT -->
Step 1: Verify User ✓
Step 2: Charge Payment ✓
Step 3: Reserve Inventory ✗ Out of stock
        ↓ Compensation
Step 2 Reverse: Refund ✓
Step 1 Reverse: N/A (verify only)
Result: No order, payment refunded
```text
<!-- Code example in TEXT -->

See [federation guide](integrations/federation/guide.md).

---

### Q: When should I use sagas?

A: Use sagas when:

- ✅ A business operation spans multiple services
- ✅ All steps must succeed together (ACID-like semantics)
- ✅ You need automatic rollback on failure
- ✅ Each step has a compensating action

Don't use sagas when:

- ❌ Operations are independent (no coordination needed)
- ❌ Manual intervention is always acceptable
- ❌ Eventual consistency is sufficient

---

### Q: What's the difference between automatic and manual compensation?

A: **Automatic Compensation**: System-driven reversal

```rust
<!-- Code example in RUST -->
SagaStep {
    forward: Mutation { operation: "chargeCard" },
    compensation: Some(Mutation { operation: "refundCharge" })
}
// Compensation runs automatically if later step fails
```text
<!-- Code example in TEXT -->

**Manual Compensation**: Logic-driven reversal

```rust
<!-- Code example in RUST -->
match coordinator.execute(steps).await {
    Ok(result) => { /* success */ },
    Err(e) => {
        // Decide what to do based on business logic
        if e.failed_step == "payment" {
            recovery_manager.recover_saga(&saga).await?;
        }
    }
}
```text
<!-- Code example in TEXT -->

See [compensation strategies](integrations/federation/sagas.md#best-practices-for-federation-sagas).

---

### Q: How do sagas handle failures?

A: Sagas have built-in failure handling:

```text
<!-- Code example in TEXT -->
Transient Failures (network, timeouts):
  → Automatic retry with exponential backoff
  → Max 3 retries by default

Permanent Failures (validation, business rule):
  → Trigger compensation immediately
  → Return error to caller

Stuck Sagas (no progress >1 hour):
  → Automatic detection
  → Recovery manager attempts retry
  → Alert operations team
```text
<!-- Code example in TEXT -->

Configure with:

```bash
<!-- Code example in BASH -->
export FRAISEQL_SAGA_MAX_RETRIES=3
export FRAISEQL_SAGA_STEP_TIMEOUT_SECONDS=30
export FRAISEQL_SAGA_RECOVERY_ENABLED=true
```text
<!-- Code example in TEXT -->

---

### Q: What's idempotency and why does it matter?

A: **Idempotency** means running an operation twice produces the same result as running it once.

**Example**: Transfer $100 twice with same `transactionId`

```text
<!-- Code example in TEXT -->
First attempt:  Account A: $1000 → $900, Account B: $500 → $600
Second attempt: Returns cached result, no double charge ✓
```text
<!-- Code example in TEXT -->

Use `request_id` or `transactionId`:

```rust
<!-- Code example in RUST -->
SagaStep {
    forward: Mutation {
        request_id: Some("txn-123"),  // Unique per request
        ..
    }
}
```text
<!-- Code example in TEXT -->

See [idempotency best practices](integrations/federation/sagas.md#best-practices-for-federation-sagas).

---

## Performance & Optimization

### Q: How fast is entity resolution?

A: Performance varies by distance:

| Scenario | Time | Notes |
|----------|------|-------|
| Local (same DB, indexed key) | <5ms | Sub-5ms guaranteed |
| Direct DB (different service, same DB) | <20ms | Network + query |
| HTTP subgraph | <200ms | Full round-trip |

Optimize with:

- ✅ Database indexes on @key fields
- ✅ Connection pooling (PgBouncer, ProxySQL)
- ✅ Result caching
- ✅ Batch operations

See [performance characteristics](integrations/federation/sagas.md#observability).

---

### Q: How do I optimize saga performance?

A: Use parallel execution for independent steps:

```rust
<!-- Code example in RUST -->
// Sequential (600ms total)
coordinator.execute(vec![
    create_account,
    init_payment,
    setup_shipping,
]).await

// Parallel (200ms total, 3x faster!)
coordinator.execute_parallel(
    vec![create_account, init_payment, setup_shipping],
    ParallelConfig { max_concurrent: 3, fail_fast: true }
).await
```text
<!-- Code example in TEXT -->

See [saga-complex example](../examples/federation/saga-complex/).

---

### Q: What's the saga timeout default?

A: **5 minutes** for entire saga, **30 seconds** per step.

Increase if needed:

```bash
<!-- Code example in BASH -->
export FRAISEQL_SAGA_TIMEOUT_SECONDS=600  # 10 minutes
export FRAISEQL_SAGA_STEP_TIMEOUT_SECONDS=60  # 1 minute per step
```text
<!-- Code example in TEXT -->

Or programmatically:

```rust
<!-- Code example in RUST -->
let coordinator = SagaCoordinator::new(metadata, store)
    .with_timeout(Duration::from_secs(600))
    .with_step_timeout(Duration::from_secs(60));
```text
<!-- Code example in TEXT -->

---

## Deployment & Operations

### Q: How do I deploy FraiseQL?

A: **Docker + Docker Compose** (recommended):

```bash
<!-- Code example in BASH -->
cd examples/federation/saga-basic
docker-compose up -d
docker-compose ps  # Verify all services healthy
```text
<!-- Code example in TEXT -->

**Kubernetes**:

```bash
<!-- Code example in BASH -->
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
kubectl get pods  # Verify running
```text
<!-- Code example in TEXT -->

**Manual** (advanced):

1. Build binary: `cargo build --release`
2. Set environment variables
3. Run: `./target/release/FraiseQL-server`

---

### Q: How do I monitor sagas in production?

A: Check saga state directly:

```sql
<!-- Code example in SQL -->
-- PostgreSQL
SELECT id, saga_type, status, created_at
FROM sagas
WHERE status IN ('PENDING', 'EXECUTING', 'FAILED')
ORDER BY created_at DESC;

-- Check stuck sagas
SELECT * FROM sagas
WHERE status = 'EXECUTING' AND created_at < NOW() - INTERVAL '1 hour';
```text
<!-- Code example in TEXT -->

Or use metrics:

```bash
<!-- Code example in BASH -->
# Prometheus metrics available
curl http://localhost:9090/metrics | grep saga
```text
<!-- Code example in TEXT -->

---

### Q: What should I backup?

A: Backup these databases:

```bash
<!-- Code example in BASH -->
# PostgreSQL (saga state)
pg_dump -U FraiseQL FraiseQL > saga_backup.sql

# MySQL (orders/inventory)
mysqldump -u root -p FraiseQL > orders_backup.sql

# All application data
docker-compose exec postgres pg_dump -U FraiseQL FraiseQL > backup.sql
```text
<!-- Code example in TEXT -->

See [deployment guide](deployment/guide.md) for production-deployment steps.

---

### Q: How do I scale FraiseQL?

A: Scale horizontally:

1. **Add more instances** (load balancer needed):

```yaml
<!-- Code example in YAML -->
FraiseQL-server-1:
  image: FraiseQL:latest

FraiseQL-server-2:
  image: FraiseQL:latest

load-balancer:
  image: nginx:latest
  ports: ["80:80"]
```text
<!-- Code example in TEXT -->

1. **Increase connection pool**:

```bash
<!-- Code example in BASH -->
export DATABASE_POOL_SIZE=50  # From 20
```text
<!-- Code example in TEXT -->

1. **Cache results**:

```bash
<!-- Code example in BASH -->
export CACHE_TTL_SECONDS=300  # 5 minutes
```text
<!-- Code example in TEXT -->

---

## Troubleshooting

### Q: "Entity resolution failed: field X required but missing"

A: The @requires directive field wasn't included. Ensure all required fields are present:

```graphql
<!-- Code example in GraphQL -->
# Schema
type User {
  email: String! @requires(fields: "phone")
  phone: String!
}

# Database query must include phone
SELECT id, email, phone FROM users WHERE id = $1
```text
<!-- Code example in TEXT -->

See [entity resolution troubleshooting](troubleshooting.md#problem-entity-resolution-fails).

---

### Q: "Saga stuck in EXECUTING for 30 minutes"

A: Check subgraph health:

```bash
<!-- Code example in BASH -->
curl http://orders-service:4000/graphql -d '{"query":"query{__typename}"}'
docker-compose restart orders-service
```text
<!-- Code example in TEXT -->

Then force recovery:

```rust
<!-- Code example in RUST -->
recovery_manager.recover_saga(&stuck_saga).await?;
```text
<!-- Code example in TEXT -->

See [saga troubleshooting](troubleshooting.md#problem-saga-stuck-in-executing).

---

### Q: "Cannot compose supergraph"

A: Verify @key directives match:

```graphql
<!-- Code example in GraphQL -->
# ✅ Correct
type User @key(fields: "id") { ... }      // users-service
extend type User @key(fields: "id") { ... }  // orders-service

# ❌ Wrong
extend type User @key(fields: "userId") { ... }  // Different key!
```text
<!-- Code example in TEXT -->

See [supergraph troubleshooting](troubleshooting.md#problem-cannot-compose-supergraph).

---

### Q: How do I enable debug logging?

A: Set log level:

```bash
<!-- Code example in BASH -->
export RUST_LOG=FraiseQL=debug,tracing=trace
RUST_LOG=debug cargo run --bin FraiseQL-server
```text
<!-- Code example in TEXT -->

Watch for logs like:

```text
<!-- Code example in TEXT -->
[DEBUG fraiseql_core::federation::entity_resolver] Resolving entity User:123
[TRACE fraiseql_core::database::query_executor] Executing: SELECT * FROM users
```text
<!-- Code example in TEXT -->

---

### Q: Where can I get help?

A: Check these resources in order:

1. **[troubleshooting.md](troubleshooting.md)** - Common issues & solutions
2. **[Federation Guide](integrations/federation/guide.md)** - Saga basics
3. **[Federation Sagas](integrations/federation/sagas.md)** - Federation patterns
4. **[Examples](../examples/federation/)** - Working code
5. **[GitHub Issues](https://github.com/anthropics/FraiseQL/issues)** - Bug reports

---

## Contributing

### Q: Can I contribute to FraiseQL?

A: **Yes!** We welcome contributions. See [CONTRIBUTING.md](../CONTRIBUTING.md) for:

- Development setup
- Code style guidelines
- Testing requirements
- Pull request process

---

## Licensing

### Q: What license is FraiseQL under?

A: FraiseQL is released under the **Apache 2.0 License**.

---

**Last Updated**: 2026-01-29
**Maintainer**: FraiseQL Federation Team
