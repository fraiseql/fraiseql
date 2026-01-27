# FraiseQL Basic Federation Example

Two-subgraph federation with Users service and Orders service.

## Architecture

```
        Federation Gateway
              |
      +-------+-------+
      |               |
  users-service   orders-service
      |               |
  PostgreSQL      PostgreSQL
```

## Setup

### Prerequisites

- Docker and Docker Compose
- FraiseQL CLI (`fraiseql` command)

### Start Services

```bash
cd examples/federation/basic
docker-compose up -d
```

This starts:
- **PostgreSQL 1** (port 5432): users database
- **PostgreSQL 2** (port 5433): orders database
- **Users Service** (port 4001): owns User entity
- **Orders Service** (port 4002): owns Order, extends User

### Verify Setup

```bash
# Check all services are running
docker-compose ps

# Should show 4 services running:
# - postgres1
# - postgres2
# - users-service
# - orders-service
```

## Test Queries

### Single Subgraph Query

Query users from users-service:

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id name email } }"
  }'
```

Response:
```json
{
  "data": {
    "users": [
      {
        "id": "user1",
        "name": "Alice",
        "email": "alice@example.com"
      },
      {
        "id": "user2",
        "name": "Bob",
        "email": "bob@example.com"
      }
    ]
  }
}
```

### Federation Query (Multi-Subgraph)

Query user with orders (federation):

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ user(id: \"user1\") { id name email orders { id total } } }"
  }'
```

Response:
```json
{
  "data": {
    "user": {
      "id": "user1",
      "name": "Alice",
      "email": "alice@example.com",
      "orders": [
        {
          "id": "order1",
          "total": 99.99
        },
        {
          "id": "order2",
          "total": 149.99
        }
      ]
    }
  }
}
```

Federation automatically:
1. Queries User from users-service (owns entity)
2. Uses user ID to resolve Order from orders-service
3. Returns complete response

### Batch Query

Get multiple users with their orders:

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "{ users { id name orders { id total } } }"
  }'
```

### Mutation

Create a new order:

```bash
curl -X POST http://localhost:4002/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation { createOrder(userId: \"user1\", total: 199.99) { id total } }"
  }'
```

## Schema

### Users Service

```python
# Owns User entity
@type
@key("id")
class User:
    id: str
    name: str
    email: str
```

### Orders Service

```python
# Extends User and owns Order
@type
@extends
@key("id")
class User:
    id: str = external()
    email: str = external()
    orders: list["Order"]

@type
@key("id")
class Order:
    id: str
    user_id: str
    total: float
```

## Databases

### Users Database

```sql
CREATE TABLE users (
  id VARCHAR(50) PRIMARY KEY,
  name VARCHAR(255),
  email VARCHAR(255)
);

INSERT INTO users VALUES
  ('user1', 'Alice', 'alice@example.com'),
  ('user2', 'Bob', 'bob@example.com');
```

### Orders Database

```sql
CREATE TABLE orders (
  id VARCHAR(50) PRIMARY KEY,
  user_id VARCHAR(50),
  total DECIMAL(10, 2)
);

INSERT INTO orders VALUES
  ('order1', 'user1', 99.99),
  ('order2', 'user1', 149.99),
  ('order3', 'user2', 249.99);
```

## Performance

### Expected Latency

- Single User query: <5ms (local resolution)
- User + Orders: ~15-20ms (federation resolution)
- Batch 100 users: ~10ms (batched local)
- Batch 100 users with orders: ~30-50ms (batched federation)

### Measuring Latency

Use `-w` flag with curl to see timing:

```bash
curl -w "Time: %{time_total}s\n" \
  -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name orders { id } } }"}'
```

## Cleanup

Stop services:

```bash
docker-compose down
```

Remove volumes:

```bash
docker-compose down -v
```

## Troubleshooting

### Services won't start

Check Docker Compose logs:

```bash
docker-compose logs users-service
docker-compose logs orders-service
```

### Queries return errors

Check that both services are running:

```bash
docker-compose ps
```

### Federation queries are slow

1. Verify both services are responding quickly
   ```bash
   time curl http://localhost:4001/graphql -d '{"query": "{users{id}}"}'
   time curl http://localhost:4002/graphql -d '{"query": "{orders{id}}"}'
   ```

2. Check network latency between services
   ```bash
   docker exec federation_users-service_1 ping orders-service
   ```

3. Verify databases have indexes on key fields
   ```bash
   docker exec federation_postgres1_1 psql -U postgres -d users -c "\\d users"
   ```

## Next Steps

1. See [Multi-Cloud Example](../multi-cloud/) for AWS/GCP/Azure deployment
2. See [Composite Keys Example](../composite-keys/) for multi-tenant setup
3. See [Advanced Example](../advanced/) for complex patterns
