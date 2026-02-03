# FraiseQL Federation Examples

Comprehensive examples for Apollo Federation v2 with FraiseQL.

## Examples

### 1. [Basic Federation](./basic/)

Two-subgraph federation with PostgreSQL.

**Architecture:** Users Service + Orders Service

**Key Features:**
- Simple entity ownership
- Cross-subgraph references
- Local database resolution
- Docker Compose setup

**Start:**
```bash
cd basic
docker-compose up -d
curl http://localhost:4001/graphql
```

**Expected:** <20ms federation queries

---

### 2. [Composite Keys](./composite-keys/)

Multi-tenant federation with composite keys.

**Architecture:** TenantUsers Service + TenantOrders Service

**Key Features:**
- Composite key federation (organizationId, userId)
- Multi-tenant data isolation
- Cross-tenant safety
- Scalable SaaS architecture

**Start:**
```bash
cd composite-keys
docker-compose up -d
```

**Query:**
```graphql
query {
  user(organizationId: "org1", userId: "user1") {
    organizationId
    userId
    name
    orders {
      id
      total
    }
  }
}
```

---

### 3. [Multi-Cloud](./multi-cloud/)

Three-cloud deployment with AWS, GCP, and Azure.

**Architecture:**
- AWS us-east-1: Users Service
- GCP europe-west1: Orders Service
- Azure southeast-asia: Products Service

**Key Features:**
- Data locality (EU, US, APAC)
- Single schema definition
- Cost transparency
- No vendor lock-in

**Deployment:**
```bash
./deploy.sh aws us-east-1 users-subgraph
./deploy.sh gcp europe-west1 orders-subgraph
./deploy.sh azure southeast-asia products-subgraph
```

---

### 4. [Advanced Patterns](./advanced/)

Complex federation scenarios.

**Includes:**
- Circular references (A ↔ B ↔ C)
- Shared fields (@shareable)
- Requires directives (@requires)
- Multi-tier federation (4+ subgraphs)

**Key Features:**
- 4-tier entity hierarchy
- Field-level federation
- Advanced optimization patterns
- Real-world scenarios

---

## Quick Comparison

| Example | Subgraphs | Features | Complexity | Latency |
|---------|-----------|----------|-----------|---------|
| Basic | 2 | Simple ownership, local DB | Low | <5ms |
| Composite | 2 | Multi-tenant, composite keys | Medium | <10ms |
| Multi-Cloud | 3 | Cross-cloud, data locality | High | <50ms |
| Advanced | 4+ | Complex patterns, sharing | High | Variable |

---

## Running All Examples

Start all examples in sequence:

```bash
# Basic
cd basic && docker-compose up -d && sleep 5

# Composite
cd ../composite-keys && docker-compose up -d && sleep 5

# Advanced
cd ../advanced && docker-compose up -d && sleep 5

# Check all are running
docker-compose ps
```

---

## Testing Federation

### Test Single Subgraph

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'
```

### Test Federation

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ user(id: \"user1\") { id name orders { id total } } }"
  }'
```

### Measure Latency

```bash
time curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user(id: \"user1\") { id orders { id } } }"}'
```

---

## Performance Expectations

### Baseline (Single Subgraph)

- Query: <5ms
- Batch 100: ~10ms

### Federation (Two Subgraphs)

- Query: 5-20ms
- Batch 100: ~20-30ms

### Multi-Cloud (Three Subgraphs)

- Query: 20-50ms
- Batch 100: ~50-100ms

### Advanced (Four+ Subgraphs)

- Query: 50-200ms (depends on depth)
- Batch 100: ~100-300ms

---

## Common Queries

### Get User with Orders

```graphql
query GetUserOrders($userId: ID!) {
  user(id: $userId) {
    id
    name
    email
    orders {
      id
      status
      total
    }
  }
}
```

Variables:
```json
{"userId": "user1"}
```

---

### Get Orders for Organization

```graphql
query GetOrgOrders($orgId: ID!) {
  organization(id: $orgId) {
    id
    name
    users {
      id
      orders {
        id
        total
      }
    }
  }
}
```

---

### Batch Query (Multiple Users)

```graphql
query {
  users {
    id
    name
    orders {
      id
      total
    }
  }
}
```

---

## Troubleshooting

### Service won't start

Check logs:
```bash
docker-compose logs users-service
docker-compose logs orders-service
```

### Slow queries

Check latency of individual services:
```bash
time curl http://localhost:4001/graphql -d '{"query": "{users{id}}"}'
time curl http://localhost:4002/graphql -d '{"query": "{orders{id}}"}'
```

### Database connection errors

Check PostgreSQL is ready:
```bash
docker-compose ps postgres1 postgres2
```

Verify connection string:
```bash
docker exec <service> env | grep DATABASE_URL
```

---

## Next Steps

1. **Run Basic Example:** Start with `basic/` for simple setup
2. **Try Composite Keys:** Move to `composite-keys/` for multi-tenant
3. **Multi-Cloud:** See `multi-cloud/` for cloud-native deployment
4. **Advanced Patterns:** Explore `advanced/` for complex scenarios

---

## Documentation

- [Federation Guide](../../docs/FEDERATION.md) - Complete guide
- [Deployment Guide](../../docs/FEDERATION_DEPLOYMENT.md) - Production setup
- [Performance Guide](../../docs/PERFORMANCE.md) - Optimization tips

