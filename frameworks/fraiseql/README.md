# FraiseQL Framework Submission

**Framework**: FraiseQL v0.11.5
**Language**: Python 3.13+
**GraphQL Endpoint**: `http://localhost:8000/graphql`
**Health Check**: `http://localhost:8000/health`

## Overview

FraiseQL is a database-first GraphQL framework that implements CQRS (Command Query Responsibility Segregation) with explicit synchronization to deliver sub-millisecond query performance.

### Key Features

- **CQRS Architecture**: Separate command (write) and query (read) sides
- **Explicit Sync**: Manual synchronization from normalized to denormalized tables
- **JSONB Denormalization**: Pre-computed query results for instant responses
- **N+1 Query Prevention**: Built-in through database design
- **Sub-millisecond Performance**: For cached GraphQL queries

## Quick Start

### Prerequisites

- Docker & Docker Compose
- PostgreSQL (via Docker)

### Run the Benchmark

```bash
# Navigate to submission directory
cd frameworks/fraiseql

# Start the application
docker-compose up

# Wait for startup message:
# 🚀 FraiseQL Blog API Ready!

# Access GraphQL playground
open http://localhost:8000/graphql
```

### Test Queries

**Simple Query**:
```graphql
query {
  users(limit: 10) {
    id
    name
    email
  }
}
```

**N+1 Prevention Test**:
```graphql
query {
  users(limit: 5) {
    id
    name
    posts {
      id
      title
      author {
        username
      }
    }
  }
}
```

**Complex Nested Query**:
```graphql
query {
  posts(limit: 10) {
    id
    title
    author {
      username
      fullName
    }
    comments {
      content
      author {
        username
      }
    }
  }
}
```

## Architecture

### CQRS Design

FraiseQL uses Command Query Responsibility Segregation:

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   GraphQL       │ →  │   PostgreSQL     │ →  │   JSONB         │
│   Request       │    │   Query (tv_*)   │    │   Response      │
│                 │    │   Denormalized   │    │   (0.5-2ms)     │
└─────────────────┘    └──────────────────┘    └─────────────────┘
                              ↑
                              │
                       ┌──────────────────┐
                       │   Explicit Sync  │
                       │   (tb_* → tv_*)  │
                       └──────────────────┘
                              ↑
                              │
                       ┌──────────────────┐
                       │   Mutations      │
                       │   (tb_* tables)  │
                       └──────────────────┘
```

### Database Schema

- **`tb_*` tables**: Normalized command side (writes)
- **`tv_*` tables**: Denormalized query side (reads)
- **JSONB storage**: Pre-computed query results

### Performance Optimizations

See [optimizations.md](optimizations.md) for detailed performance optimizations applied.

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgresql://fraiseql:fraiseql@postgres:5432/blog_demo` | PostgreSQL connection |
| `PORT` | `8000` | Server port |
| `WORKERS` | `4` | Uvicorn workers |

### Docker Resource Limits

- **CPU**: 2.0 cores
- **Memory**: 2GB
- **PostgreSQL CPU**: 2.0 cores
- **PostgreSQL Memory**: 2GB

## Benchmark Scenarios

This submission supports all required benchmark scenarios:

### 1. Simple Query (P0)
- **Query**: `users(limit: 10)`
- **Expected**: < 1ms response time
- **Database queries**: 1

### 2. N+1 Prevention (P0)
- **Query**: `users(limit: 50) { posts { author { name } } }`
- **Expected**: Single database query (not 51 queries)
- **Prevention**: CQRS denormalization

### 3. Complex Filtering (P1)
- **Query**: `usersWhere(where: {...}, orderBy: {...})`
- **Expected**: Efficient SQL generation
- **Database queries**: 1

### 4. Mutations (P1)
- **Mutation**: `createUser(input: {...})`
- **Expected**: < 10ms with sync
- **Side effects**: Explicit sync to query side

### 5. Deep Nesting (P2)
- **Query**: Posts with nested author and comments
- **Expected**: Single query performance
- **Database queries**: 1

## Testing

### Correctness Tests

```bash
# Run GraphQL queries and verify responses
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users(limit: 5) { id name } }"}'
```

### N+1 Prevention Test

```bash
# This should execute exactly 2 queries total:
# 1. SELECT from tv_user (with embedded posts)
# 2. SELECT from tv_post (for full post details if needed)

# Check database logs or use query monitoring
```

### Load Testing

```bash
# Install wrk for load testing
wrk -t4 -c100 -d30s http://localhost:8000/graphql \
  -s scripts/simple_query.lua
```

## Monitoring

### Health Check

```bash
curl http://localhost:8000/health
# Expected: {"status": "healthy"}
```

### Metrics

```bash
curl http://localhost:8000/metrics
# Returns sync performance and health metrics
```

### Database Connection

```bash
# Connect to PostgreSQL
docker-compose exec postgres psql -U fraiseql -d blog_demo

# Check sync status
SELECT * FROM sync_log ORDER BY created_at DESC LIMIT 5;

## Troubleshooting

### Common Issues

**Application won't start**:
```bash
# Check logs
docker-compose logs app

# Verify database connection
docker-compose exec postgres pg_isready -U fraiseql
```

**Slow queries**:
```bash
# Check if tv_* tables have data
docker-compose exec postgres psql -U fraiseql -d blog_demo \
  -c "SELECT COUNT(*) FROM tv_post;"

# Check sync logs
docker-compose exec postgres psql -U fraiseql -d blog_demo \
  -c "SELECT * FROM sync_log WHERE success = false;"
```

**N+1 queries detected**:
- Verify that queries use `tv_*` tables (denormalized)
- Check that mutations call explicit sync functions
- Ensure sync operations completed successfully

## Development

### Local Development

```bash
# Install dependencies
pip install -r requirements.txt

# Run migrations
python migrations/run_migrations.py

# Start development server
uvicorn app:app --reload
```

### Code Structure

```
frameworks/fraiseql/
├── app.py              # FastAPI application
├── schema.py           # GraphQL schema
├── sync.py             # Explicit sync logic
├── migrations/         # Database schema
├── docker-compose.yml  # Container orchestration
├── Dockerfile          # Application container
├── optimizations.md    # Performance details
└── README.md           # This file
```

## License

MIT License - see project root LICENSE file.

## Contact

- **Maintainer**: Lionel Hamayon
- **Email**: lionel.hamayon@evolution-digitale.fr
- **GitHub**: [@evoludigit](https://github.com/evoludigit)
- **Repository**: [fraiseql/fraiseql](https://github.com/fraiseql/fraiseql)
