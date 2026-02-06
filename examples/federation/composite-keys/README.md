# Composite Keys Federation Example

Multi-tenant SaaS federation with composite key entity resolution.

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    Apollo Router/Gateway                     │
│                      (Port 4000)                             │
└────────┬───────────────────────────────────────┬─────────────┘
         │                                       │
    ┌────▼─────────┐                      ┌──────▼────────┐
    │TenantUsers   │                      │TenantOrders   │
    │Service       │                      │Service        │
    │(Port 4001)   │                      │(Port 4002)    │
    └────┬─────────┘                      └──────┬────────┘
         │                                       │
    ┌────▼──────────────────────┐         ┌──────▼────────────────┐
    │PostgreSQL: tenants_db     │         │PostgreSQL: orders_db  │
    │(Port 5432)               │         │(Port 5433)           │
    │                          │         │                       │
    │ organizations            │         │ tenant_orders         │
    │ ├─ id (PK)              │         │ ├─ organization_id    │
    │ └─ name                 │         │ ├─ order_id (PK)      │
    │                          │         │ ├─ user_id           │
    │ organization_users       │         │ ├─ status            │
    │ ├─ organization_id (PK)  │         │ └─ amount            │
    │ ├─ user_id (PK)         │         │                       │
    │ ├─ name                 │         │ Composite Key:        │
    │ ├─ email                │         │ (organization_id,     │
    │ └─ role                 │         │  order_id)            │
    └────────────────────────┘         └───────────────────────┘
```

## Key Features

- **Composite Key Federation**: Entities identified by multiple fields (organizationId, userId)
- **Multi-Tenant Data Isolation**: Complete separation by organizationId
- **Cross-Tenant Safety**: Mutations automatically filtered by tenant context
- **SaaS Architecture**: Scalable multi-tenant pattern

## Setup

### Prerequisites

```bash
docker --version      # Docker 20.10+
docker-compose --version  # Docker Compose 1.29+
```

### Start Services

```bash
# Build images
docker-compose build

# Start all services
docker-compose up -d

# Wait for services to be ready
sleep 10

# Check status
docker-compose ps
```

All services should show "healthy" status.

### Expected Output

```
NAME                    STATUS
tenants-postgres1       running (healthy)
tenants-postgres2       running (healthy)
tenants-users-service   running (healthy)
tenants-orders-service  running (healthy)
```

## Database Schema

### TenantUsers Service (PostgreSQL 1)

```sql
CREATE TABLE organizations (
  id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE organization_users (
  organization_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) NOT NULL,
  role VARCHAR(50) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (organization_id, user_id),
  FOREIGN KEY (organization_id) REFERENCES organizations(id)
);

CREATE INDEX idx_org_id ON organization_users(organization_id);
```

### TenantOrders Service (PostgreSQL 2)

```sql
CREATE TABLE tenant_orders (
  organization_id VARCHAR(50) NOT NULL,
  order_id VARCHAR(50) NOT NULL,
  user_id VARCHAR(50) NOT NULL,
  status VARCHAR(50) NOT NULL DEFAULT 'pending',
  amount DECIMAL(10, 2) NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (organization_id, order_id)
);

CREATE INDEX idx_org_order ON tenant_orders(organization_id, order_id);
CREATE INDEX idx_org_user ON tenant_orders(organization_id, user_id);
```

## Example Queries

### Get User with Orders (Same Tenant)

```graphql
query GetUserOrders($orgId: ID!, $userId: ID!) {
  user(organizationId: $orgId, userId: $userId) {
    organizationId
    userId
    name
    email
    orders {
      organizationId
      orderId
      status
      amount
    }
  }
}
```

Variables:

```json
{
  "orgId": "org1",
  "userId": "user1"
}
```

Expected Response:

```json
{
  "data": {
    "user": {
      "organizationId": "org1",
      "userId": "user1",
      "name": "Alice Johnson",
      "email": "alice@example.com",
      "orders": [
        {
          "organizationId": "org1",
          "orderId": "order1",
          "status": "completed",
          "amount": "149.99"
        },
        {
          "organizationId": "org1",
          "orderId": "order2",
          "status": "pending",
          "amount": "299.99"
        }
      ]
    }
  }
}
```

### Get All Users in Organization

```graphql
query GetOrgUsers($orgId: ID!) {
  organization(id: $orgId) {
    id
    name
    users {
      userId
      name
      email
      role
    }
  }
}
```

Variables:

```json
{
  "orgId": "org1"
}
```

### Create Order for User

```graphql
mutation CreateOrder($orgId: ID!, $userId: ID!, $amount: Float!) {
  createOrder(organizationId: $orgId, userId: $userId, amount: $amount) {
    organizationId
    orderId
    status
    amount
  }
}
```

Variables:

```json
{
  "orgId": "org1",
  "userId": "user1",
  "amount": 199.99
}
```

## Test API

### Test Users Service

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { organizationId userId name } }"
  }'
```

### Test Orders Service

```bash
curl -X POST http://localhost:4002/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ orders { organizationId orderId status } }"
  }'
```

### Test Federated Query

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query { user(organizationId: \"org1\", userId: \"user1\") { userId name orders { orderId status } } }"
  }'
```

## Performance Expectations

| Scenario | Latency | Notes |
|----------|---------|-------|
| Get single user | <5ms | Direct local DB |
| Get user with orders | 10-20ms | Cross-subgraph federation |
| List all users | <10ms | Local batch query |
| List all orders | <10ms | Local batch query |
| Create order | 15-25ms | Local write + federation sync |
| Batch 100 users | ~20ms | Batched local queries |

## Troubleshooting

### Services won't start

```bash
# Check logs
docker-compose logs tenants-users-service
docker-compose logs tenants-orders-service

# Verify databases are healthy
docker-compose logs tenants-postgres1
docker-compose logs tenants-postgres2
```

### Connection errors

```bash
# Verify services can communicate
docker-compose exec tenants-users-service curl http://tenants-orders-service:4000/health

# Check network
docker-compose exec tenants-users-service ping tenants-postgres1
```

### Slow queries

```bash
# Check database connection pools
curl http://localhost:4001/metrics | grep pool

# Check query logs
docker-compose logs tenants-users-service | grep "query"
```

## Cleanup

```bash
# Stop services
docker-compose down

# Remove volumes (delete data)
docker-compose down -v
```

## Next Steps

1. **Modify Schemas**: Update `schema.py` files to add your own types
2. **Add Mutations**: Create custom mutations in schema definitions
3. **Deploy to Cloud**: Use `../multi-cloud/` as reference for cloud deployment
4. **Monitor Performance**: Use metrics endpoint at `/metrics` for monitoring

## Multi-Tenant Best Practices

- Always include `organizationId` in query filters
- Validate tenant context in mutations
- Use composite keys consistently across services
- Monitor per-tenant latency and error rates
- Implement backup/recovery per tenant
