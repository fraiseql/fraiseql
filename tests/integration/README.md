# Integration Testing - Federation Multi-Subgraph Harness

This directory contains a Docker Compose environment for testing Apollo Federation v2 with FraiseQL across multiple subgraphs.

## Architecture

```
Apollo Router (Gateway)       [Port 4000]
├─ Users Subgraph            [Port 4001]
│  └─ PostgreSQL              [Port 5432]
├─ Orders Subgraph           [Port 4002]
│  └─ PostgreSQL              [Port 5433]
└─ Products Subgraph         [Port 4003]
   └─ PostgreSQL              [Port 5434]
```

## Entity Federation Pattern

**Trinity Pattern** - Each entity table uses:

- `pk_{entity}`: BIGINT surrogate key (GENERATED ALWAYS AS IDENTITY)
- `id`: UUID federation key (globally unique, federation primary key)
- `identifier`: TEXT semantic key (human-readable)

Example:

```sql
CREATE TABLE tb_user (
    pk_user BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    id UUID NOT NULL UNIQUE DEFAULT uuid_generate_v4(),
    identifier TEXT NOT NULL UNIQUE,  -- email
    name TEXT NOT NULL,
    ...
);
```

## Services

### Users Subgraph (Port 4001)

- **Owns**: `User` entity
- **Key**: `@key(fields=["id"])` with UUID
- **Database**: PostgreSQL (users database)
- **Mutations**: create_user, update_user, delete_user

### Orders Subgraph (Port 4002)

- **Owns**: `Order` entity
- **Key**: `@key(fields=["id"])` with UUID
- **Extends**: `User` from users subgraph (HTTP federation)
- **Database**: PostgreSQL (orders database)
- **Relations**: Foreign key to User.id (UUID)
- **Mutations**: create_order, update_order_status, delete_order

### Products Subgraph (Port 4003)

- **Owns**: `Product` entity
- **Key**: `@key(fields=["id"])` with UUID
- **Extends**: `Order` from orders subgraph (HTTP federation)
- **Database**: PostgreSQL (products database)
- **Relations**: Order can have products
- **Mutations**: create_product, update_product_stock, update_product_price, delete_product

### Apollo Router Gateway (Port 4000)

- Composes all three subgraphs
- Executes federated queries
- Handles entity resolution across boundaries

## Quick Start

### Prerequisites

- Docker and Docker Compose installed
- At least 2GB free memory

### Start Services

```bash
cd tests/integration
docker-compose up -d

# Wait for health checks to pass (30-60 seconds)
docker-compose ps

# Verify all services are healthy
docker-compose logs apollo-router | grep "server running"
```

### Query Federation

**Simple Query (single subgraph)**:

```bash
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id identifier } }"}'
```

**Federated Query (cross-subgraph)**:

```bash
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query {
      users {
        id
        identifier
        orders {
          id
          status
          total
        }
      }
    }"
  }'
```

**3-Hop Federated Query**:

```bash
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "query {
      users {
        id
        orders {
          id
          products {
            id
            name
            price
          }
        }
      }
    }"
  }'
```

### Stop Services

```bash
docker-compose down -v  # -v removes volumes
```

### Logs

View logs for specific service:

```bash
docker-compose logs -f users-subgraph
docker-compose logs -f orders-subgraph
docker-compose logs -f products-subgraph
docker-compose logs -f apollo-router
```

## Database Seeding

### Initial Data

**Users** (10 seeded):

- IDs: 550e8400-e29b-41d4-a716-446655440001 through ...10
- Identifiers: user{1-10}@example.com

**Orders** (10 seeded):

- IDs: 650e8400-e29b-41d4-a716-446655440001 through ...10
- References: User IDs via foreign key

**Products** (10 seeded):

- IDs: 750e8400-e29b-41d4-a716-446655440001 through ...10

### Add Test Data

Access any subgraph directly for mutations:

```bash
curl -X POST http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{
    "query": "mutation {
      createUser(identifier: \"alice@example.com\", name: \"Alice\", email: \"alice@example.com\") {
        id
        identifier
      }
    }"
  }'
```

## Testing

### Manual Testing Checklist

- [ ] All services start and health checks pass
- [ ] Users subgraph responds to queries
- [ ] Orders subgraph responds and extends User
- [ ] Products subgraph responds and extends Order
- [ ] Apollo Router composes schema successfully
- [ ] Simple queries work through gateway
- [ ] Federated queries work across 2 subgraphs
- [ ] 3-hop federated queries work
- [ ] Mutations propagate correctly

### Automated Integration Tests

Run integration tests:

```bash
# From project root
cargo test --test federation_docker_compose_integration
```

## Federation Configuration

### Order Service (federation.toml)

```toml
[federation]
enabled = true
version = "v2"

[[federation.subgraphs]]
name = "User"
strategy = "http"
url = "http://users-subgraph:4001/graphql"

[[federation.subgraphs]]
name = "Order"
strategy = "local"
view_name = "v_order"
key_columns = ["id"]
```

### Products Service (federation.toml)

```toml
[federation]
enabled = true
version = "v2"

[[federation.subgraphs]]
name = "Order"
strategy = "http"
url = "http://orders-subgraph:4002/graphql"

[[federation.subgraphs]]
name = "Product"
strategy = "local"
view_name = "v_product"
key_columns = ["id"]
```

## Architecture Decisions

### Why Trinity Pattern?

- **Surrogate Key (pk_{entity})**: Efficient indexing, sequential
- **Federation Key (id UUID)**: Globally unique across services
- **Semantic Key (identifier)**: Human-readable for debugging

### Why Views for Federation?

- FraiseQL queries against v_{entity} views
- Views filter to federation-relevant columns
- Decouples internal schema from federation contracts
- Example: `v_user` exposes only [id, email, name, identifier]

### Why HTTP Federation Strategy?

- PostgreSQL databases on different hosts
- DirectDB strategy requires cross-database drivers
- HTTP is simpler, more observable, easier to debug
- Latency targets: <200ms per federation hop

## Troubleshooting

### Services Won't Start

```bash
# Check Docker disk space
docker system df

# Clear volumes and restart
docker-compose down -v
docker-compose up -d
```

### Health Checks Failing

```bash
# View service logs
docker-compose logs users-subgraph

# Manual health check
curl -f http://localhost:4001/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ __typename }"}'
```

### Gateway Can't Compose Schema

```bash
# Check Apollo Router logs
docker-compose logs apollo-router

# Verify subgraph SDL endpoint
curl http://localhost:4001/graphql?query={_service{sdl}}
```

### Queries Return Errors

```bash
# Get query error details
curl -X POST http://localhost:4000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id } }"}' | jq '.errors'
```

## Performance Characteristics

With full environment running:

- **Local query** (single subgraph): ~5ms
- **Federated query** (2 subgraphs): ~15-30ms
- **3-hop query**: ~50-100ms
- **Batch 100 entities**: ~20-50ms

## Next Steps

After basic harness verification:

1. Run integration test suite (8 pending test scenarios)
2. Benchmark federation performance
3. Test Python/TypeScript schema equivalence
4. Validate Apollo Router composition
