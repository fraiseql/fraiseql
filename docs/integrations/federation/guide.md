# FraiseQL Federation v2 Guide

**Status:** ✅ Production Ready
**Audience:** Architects, Developers, DevOps
**Reading Time:** 20-30 minutes
**Last Updated:** 2026-02-05

FraiseQL implements **Apollo Federation v2**, enabling multi-subgraph GraphQL composition with sub-5ms latency for local entity resolution and sub-20ms for direct database federation.

## Prerequisites

**Required Knowledge:**

- GraphQL basics (types, fields, queries, mutations)
- Apollo Federation v2 concepts (@key, @external, @extends)
- Multi-database architecture understanding
- REST API design (for HTTP federation fallback)
- Basic networking and service communication

**Required Software:**

- FraiseQL v2.0.0-alpha.1 or later
- Apollo Router or compatible gateway
- Python 3.10+, TypeScript 4.5+, or other supported SDK
- PostgreSQL 14+ or MySQL 8.0+ or SQL Server 2019+
- Docker (for local testing)

**Required Infrastructure:**

- 2+ FraiseQL instances (one per subgraph)
- Apollo Router instance or Apollo Gateway
- Database instances (one per subgraph or shared)
- Network connectivity between services
- For local DB federation: direct database access between subgraphs

**Optional but Recommended:**

- Load balancer (for HA)
- Service mesh (Istio, Linkerd for observability)
- Distributed tracing (Jaeger, Zipkin)

**Time Estimate:** 1-3 hours to set up multi-database federation

---

## Table of Contents

1. [Quick Start](#quick-start)
2. [Core Concepts](#core-concepts)
3. [Federation Directives](#federation-directives)
4. [Entity Resolution Strategies](#entity-resolution-strategies)
5. [Common Patterns](#common-patterns)
6. [Troubleshooting](#troubleshooting)
7. [Performance Optimization](#performance-optimization)

---

## Quick Start

### 1. Define a Federated Entity (Python)

```python
from FraiseQL import type, key

@type
@key("id")
class User:
    id: str
    name: str
    email: str
```

### 2. Extend the Entity in Another Subgraph

```python
from FraiseQL import type, key, extends, external

@type
@extends
@key("id")
class User:
    id: str = external()
    email: str = external()
    orders: list["Order"]
```

### 3. Deploy Subgraphs to Federation Gateway

```bash
# Subgraph 1: Users service
FraiseQL deploy users-service --port 4001 --federation

# Subgraph 2: Orders service
FraiseQL deploy orders-service --port 4002 --federation
```

### 4. Query Through Federation Gateway

```graphql
query {
  user(id: "user123") {
    id
    name
    email
    orders {
      id
      total
    }
  }
}
```

The gateway automatically:

- Queries User from users-service (owns entity)
- Resolves Order from orders-service (extends User)
- Returns complete response

---

## Core Concepts

### Subgraphs

A **subgraph** is a FraiseQL instance that:

- Owns a subset of entity types
- May extend types from other subgraphs
- Participates in federated composition

### Entity Ownership

Each entity type is owned by exactly one subgraph:

- The owner defines the complete schema
- Other subgraphs can extend it with additional fields
- Only the owner can create/update/delete entities

### Entity Resolution

When a query needs an entity, FraiseQL uses one of three strategies:

1. **Local** (<5ms): Entity owned by this subgraph, query local database
2. **Direct DB** (<20ms): FraiseQL-to-FraiseQL, direct database connection
3. **HTTP** (<200ms): External subgraph, HTTP GraphQL query

### Federation Metadata

Federation is enabled via `@key` directives:

```python
@type
@key("id")           # Single key field
@key("org_id id")    # Composite key
class User:
    id: str
    org_id: str
```

---

## Federation Directives

### @key

Declares the primary key for entity identification in federation.

**Usage:**

```python
@type
@key("id")
class User:
    id: str

@type
@key("organizationId id")  # Composite key
class OrgUser:
    organizationId: str
    id: str
```

**Features:**

- Single or composite keys
- Multiple @key directives supported
- Marks entity as resolvable in _entities query

**Performance:** <500ps key field extraction

---

### @extends

Marks a type as extending an entity from another subgraph.

**Usage:**

```python
@type
@extends
@key("id")
class User:
    id: str = external()
    email: str = external()
    orders: list["Order"]
```

**When to use:**

- Adding fields to entities owned elsewhere
- Creating relationships from extended entities
- Sharing computed fields

---

### @external

Marks fields as owned by another subgraph.

**Usage:**

```python
@type
@extends
@key("id")
class User:
    id: str = external()      # Owned by users service
    email: str = external()   # Owned by users service
    reputation: int           # Owned by this service
```

**Important:** External fields can only be used in:

- `@requires` clauses
- Entity identification
- Reference resolution

Cannot be mutated in this subgraph.

---

### @shareable

Marks fields that can be resolved by multiple subgraphs.

**Usage:**

```python
@type
@shareable
class Product:
    id: str
    name: str
    description: str  # Shareable - can be computed anywhere
```

**Use case:** Fields that are expensive to compute and can be cached/replicated across subgraphs.

---

## Entity Resolution Strategies

### Local Resolution (<5ms)

**When:** Entity is owned by this subgraph

**Query Pattern:**

```sql
SELECT id, name, email FROM users WHERE id IN (?, ?, ...)
```

**Performance:** ~5ms for 100 entities (local database)

**Example:**

```python
# Users Subgraph
@type
@key("id")
class User:
    id: str
    name: str
    email: str

# Resolves locally, never queries external service
```

---

### Direct Database Resolution (<20ms)

**When:** Entity is in another FraiseQL instance's database

**Query Pattern:**
Direct connection to remote database, same as local resolution

**Performance:** ~20ms for 100 entities across networks

**Setup:**

```toml
# config.toml
[federation.subgraphs]
name = "Order"
strategy = "direct-database"
database_url = "postgresql://orders-db:5432/orders"
```

**Benefits:**

- No HTTP overhead
- Lower latency than HTTP federation
- Works with same schema.compiled.json

---

### HTTP Resolution (<200ms)

**When:** Entity is in external GraphQL service

**Query Pattern:**

```graphql
query($representations: [_Any!]!) {
  _entities(representations: $representations) {
    __typename
    ... on Order { id status }
  }
}
```

**Performance:** ~200ms for typical subgraph

**Setup:**

```toml
# config.toml
[federation.subgraphs]
name = "Order"
strategy = "http"
url = "http://orders-service:4000/graphql"
```

**Benefits:**

- Works with any GraphQL service
- Easy integration with Apollo Server
- Good for multi-vendor setups

---

## Common Patterns

### Pattern 1: Simple Two-Subgraph Federation

**Scenario:** Users service + Orders service

```python
# users-service/schema.py
@type
@key("id")
class User:
    id: str
    name: str
    email: str

# orders-service/schema.py
@type
@extends
@key("id")
class User:
    id: str = external()
    email: str = external()

@type
@key("id")
class Order:
    id: str
    user_id: str
    total: float
```

**Query:**

```graphql
query {
  user(id: "user123") {
    id
    name
    email
    orders {
      id
      total
    }
  }
}
```

---

### Pattern 2: Multi-Tenant Composite Keys

**Scenario:** SaaS with organization isolation

```python
@type
@key("organization_id id")
class OrgUser:
    organization_id: str
    id: str
    name: str
    email: str

@type
@extends
@key("organization_id id")
class OrgUser:
    organization_id: str = external()
    id: str = external()
    email: str = external()

@type
@key("organization_id id")
class OrgOrder:
    organization_id: str
    id: str
    user_id: str
    total: float
```

**Benefits:**

- Complete data isolation by organization
- Single schema definition
- Same query pattern as simple case

---

### Pattern 3: Three-Tier Federation

**Scenario:** Products → Orders → Users

```python
# users-service: owns User
@type
@key("id")
class User:
    id: str
    name: str

# orders-service: extends User, owns Order
@type
@extends
@key("id")
class User:
    id: str = external()

@type
@key("id")
class Order:
    id: str
    user_id: str

# products-service: extends Order, owns Product
@type
@extends
@key("id")
class Order:
    id: str = external()

@type
@key("id")
class Product:
    id: str
    order_id: str
    price: float
```

**Query Resolution:**

1. products-service queries local Product
2. Fetches Order from orders-service
3. orders-service fetches User from users-service
4. Response bubbles back through all layers

---

### Pattern 4: Multi-Cloud Deployment

**Scenario:** Users in AWS, Orders in GCP, Products in Azure

```python
# Same schema.py deployed to all three clouds
@type
@key("id")
class User:
    id: str
    name: str

@type
@key("id")
class Order:
    id: str
    user_id: str

@type
@key("id")
class Product:
    id: str
    order_id: str
```

**Deployment:**

```bash
# AWS us-east-1: Users
FraiseQL deploy users-subgraph \
  --cloud aws \
  --region us-east-1 \
  --database postgresql://aws-db:5432/users

# GCP europe-west1: Orders
FraiseQL deploy orders-subgraph \
  --cloud gcp \
  --region europe-west1 \
  --database postgresql://gcp-db:5432/orders

# Azure southeast-asia: Products
FraiseQL deploy products-subgraph \
  --cloud azure \
  --region southeast-asia \
  --database postgresql://azure-db:5432/products
```

**Key Benefits:**

- Single schema definition
- Data locality (EU data stays in EU)
- Cost transparency (pay cloud providers directly)
- No vendor lock-in

---

## Troubleshooting

### Issue: "Entity not found" errors

**Symptom:** `_entities query returns null for valid entities`

**Cause:** Entity ownership mismatch

**Solution:**

1. Verify `@key` directives match across subgraphs
2. Check database contains the requested entity
3. Verify entity IDs are correctly passed

```python
# ✅ Correct: Same key definition
# users-service
@key("id")
class User: id: str

# orders-service
@extends
@key("id")
class User: id: str = external()
```

---

### Issue: "Field is external" errors

**Symptom:** Cannot query/mutate external fields in extended type

**Cause:** Attempting to write external fields

**Solution:** Only write fields owned by this subgraph

```python
# ❌ Wrong: Cannot mutate external field
mutation {
  updateUser(id: "123", email: "new@example.com") { id }
}

# ✅ Correct: Only mutate owned fields
mutation {
  updateUserReputation(id: "123", reputation: 5) { id }
}
```

---

### Issue: Circular dependencies

**Symptom:** A extends B, B extends A

**Solution:** Break the cycle by introducing a new type or restructuring

```python
# ❌ Wrong: Circular
# A extends B
# B extends A

# ✅ Correct: Linear hierarchy
# A (owns User)
# B (extends User, owns Order)
# C (extends Order, owns Product)
```

---

### Issue: Slow federation queries

**Symptom:** Federation queries >200ms latency

**Causes and Solutions:**

1. **HTTP Overhead:** Switch to DirectDB strategy

   ```toml
   strategy = "direct-database"
   database_url = "postgresql://..."
   ```

2. **Network Latency:** Use local or DirectDB resolution
   - Local: <5ms
   - DirectDB: <20ms
   - HTTP: <200ms expected

3. **Database Slow:** Add indexes to key fields

   ```sql
   CREATE INDEX idx_user_id ON users(id);
   ```

4. **Batching Issues:** Ensure representations are batched
   - Should fetch 100+ entities in single query
   - Not individual queries per entity

---

## Performance Optimization

### 1. Index Key Fields

All fields in `@key` directives must be indexed for performance:

```sql
-- Create indexes for key fields
CREATE INDEX idx_id ON users(id);
CREATE INDEX idx_org_id_user_id ON users(organization_id, id);
```

**Performance impact:**

- Without index: 50-100ms per entity
- With index: <5ms per entity

---

### 2. Batch Entity Resolution

FraiseQL automatically batches entity representations:

```python
# ✅ Single query for all entities
# Input: 100 entity representations
# Database query: WHERE id IN (id1, id2, ..., id100)
# Executes: 1 query for 100 entities
```

This is automatic; no configuration needed.

---

### 3. Choose Resolution Strategy

| Strategy | Latency | Use Case |
|----------|---------|----------|
| Local | <5ms | Same subgraph |
| DirectDB | <20ms | FraiseQL-to-FraiseQL |
| HTTP | <200ms | External services |

**Recommendation:**

- Use Local for entities you own
- Use DirectDB for FraiseQL subgraphs
- Use HTTP only for non-FraiseQL services

---

### 4. Monitor Entity Resolution

Add logging to track resolution:

```python
import logging

logging.basicConfig(level=logging.DEBUG)
logger = logging.getLogger('FraiseQL.federation')

# Shows resolution strategy per query
# Example: Resolved 100 users via local (5ms)
```

---

### 5. Connection Pooling

FraiseQL automatically pools connections:

```toml
[federation.connection_manager]
pool_size = 10          # Default: 5
timeout_seconds = 5     # Default: 5
```

**Performance:** Connection reuse reduces setup overhead ~50%

---

## API Reference

### Python Decorators

```python
from FraiseQL import type, key, extends, external, shareable

# Define federated entity
@type
@key("id")
class User:
    id: str

# Extend entity from another subgraph
@extends
@key("id")
class User:
    id: str = external()

# Mark fields as external (owned elsewhere)
@external()

# Mark fields as shareable (can be resolved elsewhere)
@shareable()
```

### TypeScript Decorators

```typescript
import { Key, Type, Extends, External, Shareable } from 'FraiseQL';

@Type()
@Key("id")
class User {
  id: string;
}

@Extends()
@Key("id")
class User {
  @External() id: string;
}
```

### Rust Runtime

```rust
use fraiseql_core::federation::FederationMetadata;

let metadata = FederationMetadata {
    enabled: true,
    version: "v2".to_string(),
    types: vec![
        FederatedType {
            name: "User".to_string(),
            keys: vec![KeyDirective {
                fields: vec!["id".to_string()],
                resolvable: true,
            }],
            is_extends: false,
            external_fields: vec![],
            shareable_fields: vec![],
        },
    ],
};
```

---

## Next Steps

1. **Setup Examples:** See `examples/federation/basic/` for complete working example
2. **Deploy:** See [deployment.md](./deployment.md) for multi-cloud setup
3. **Performance:** See [Performance Optimization](#performance-optimization) above
4. **Troubleshoot:** See [Troubleshooting](#troubleshooting) above

---

## Additional Resources

- [Apollo Federation v2 Specification](https://www.apollographql.com/docs/apollo-server/federation/federation-2/)
- [Deployment Guide](./deployment.md)
- [Readiness Checklist](./readiness-checklist.md)

---

## See Also

- **[Consistency Model](../../guides/consistency-model.md)** - Understanding consistency in federation
- **[Production Deployment](../../guides/production-deployment.md)** - Deploying federation in production
- **[Federation Architecture](../../architecture/integration/federation.md)** - Technical architecture details
- **[SAGA Pattern](./sagas.md)** - Distributed transaction coordination
- **[Enterprise RBAC](../../enterprise/rbac.md)** - Row-level security and multi-tenant isolation
