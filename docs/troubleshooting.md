<!-- Skip to main content -->
---

title: FraiseQL Troubleshooting Guide
description: 1. [Installation & Setup](#installation--setup)
keywords: []
tags: ["documentation", "reference"]
---

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
<!-- Code example in BASH -->
# Install build tools
sudo apt-get install build-essential
cargo clean && cargo build --release
```text
<!-- Code example in TEXT -->

---

## Schema & Federation

### Problem: "Unknown directive @key"

**Solution**:
Ensure schema includes federation import:

```graphql
<!-- Code example in GraphQL -->
extend schema @link(url: "https://specs.apollo.dev/federation/v2.0")
```text
<!-- Code example in TEXT -->

---

### Problem: Entity resolution fails

**Solution**:
Verify @requires fields are included in entity representation and database queries return all needed fields.

---

### Problem: "Cannot compose supergraph"

**Solution**:
Check that @key directives match across services:

```graphql
<!-- Code example in GraphQL -->
# Both must match
type User @key(fields: "id") { id: ID! }
extend type User @key(fields: "id") { id: ID! }
```text
<!-- Code example in TEXT -->

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
<!-- Code example in GraphQL -->
type Mutation {
  cancelOrder(orderId: ID!): Order!
  refundCharge(chargeId: String!): Charge!
}
```text
<!-- Code example in TEXT -->

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
<!-- Code example in SQL -->
CREATE INDEX idx_user_id ON users(id);
```text
<!-- Code example in TEXT -->

---

### Problem: High memory usage

**Solution**:
Reduce connection pool: `export DATABASE_POOL_SIZE=10`

---

## Production Issues

### Problem: Database connection lost

**Solution**:

1. Check health: `docker-compose exec postgres pg_isready -U FraiseQL`
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
<!-- Code example in BASH -->
export RUST_LOG=FraiseQL=debug
RUST_LOG=debug cargo run
```text
<!-- Code example in TEXT -->

### Query Saga State

```bash
<!-- Code example in BASH -->
docker-compose exec postgres psql -U FraiseQL -d FraiseQL
SELECT * FROM sagas WHERE id = 'YOUR_SAGA_ID';
SELECT * FROM saga_steps WHERE saga_id = 'YOUR_SAGA_ID';
```text
<!-- Code example in TEXT -->

### Test GraphQL

```bash
<!-- Code example in BASH -->
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "query { users { id } }"}'
```text
<!-- Code example in TEXT -->

---

**Maintainer**: FraiseQL Federation Team
