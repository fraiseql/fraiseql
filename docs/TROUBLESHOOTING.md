# FraiseQL Troubleshooting Guide

**Last Updated**: 2026-01-29

---

## Table of Contents

1. [Installation & Setup](#installation--setup)
2. [Schema & Federation](#schema--federation)
3. [Saga Execution](#saga-execution)
4. [Performance & Optimization](#performance--optimization)
5. [Production Issues](#production-issues)

---

## Installation & Setup

### Problem: Cargo build fails

**Solution**:
```bash
# Install build tools
sudo apt-get install build-essential
cargo clean && cargo build --release
```

---

## Schema & Federation

### Problem: "Unknown directive @key"

**Solution**:
Ensure schema includes federation import:
```graphql
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")
```

---

### Problem: Entity resolution fails

**Solution**:
Verify @requires fields are included in entity representation and database queries return all needed fields.

---

### Problem: "Cannot compose supergraph"

**Solution**:
Check that @key directives match across services:
```graphql
# Both must match
type User @key(fields: "id") { id: ID! }
extend type User @key(fields: "id") { id: ID! }
```

---

## Saga Execution

### Problem: Saga stuck in EXECUTING

**Solution**:
1. Check subgraph service health: `curl http://service:4000/graphql`
2. Restart if needed: `docker-compose restart service`
3. Force recovery: `recovery_manager.recover_saga().await?`

---

### Problem: Saga compensation fails

**Solution**:
Verify compensation mutations exist in schema:
```graphql
type Mutation {
  cancelOrder(orderId: ID!): Order!
  refundCharge(chargeId: String!): Charge!
}
```

---

### Problem: Saga exceeds timeout

**Solution**:
Increase timeout: `export FRAISEQL_SAGA_TIMEOUT_SECONDS=600`

---

## Performance & Optimization

### Problem: Slow entity resolution

**Solution**:
Add database indexes:
```sql
CREATE INDEX idx_users_id ON users(id);
```

---

### Problem: High memory usage

**Solution**:
Reduce connection pool: `export DATABASE_POOL_SIZE=10`

---

## Production Issues

### Problem: Database connection lost

**Solution**:
1. Check health: `docker-compose exec postgres pg_isready -U fraiseql`
2. Restart: `docker-compose restart postgres`

---

### Problem: Router cannot load subgraph

**Solution**:
Test service: `curl http://users-service:4000/graphql`

---

### Problem: Saga recovery not working

**Solution**:
Enable recovery: `export FRAISEQL_SAGA_RECOVERY_ENABLED=true`

---

## Debugging

### Enable Debug Logs
```bash
export RUST_LOG=fraiseql=debug
RUST_LOG=debug cargo run
```

### Query Saga State
```bash
docker-compose exec postgres psql -U fraiseql -d fraiseql
SELECT * FROM sagas WHERE id = 'YOUR_SAGA_ID';
SELECT * FROM saga_steps WHERE saga_id = 'YOUR_SAGA_ID';
```

### Test GraphQL
```bash
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "query { users { id } }"}'
```

---

**Maintainer**: FraiseQL Federation Team
