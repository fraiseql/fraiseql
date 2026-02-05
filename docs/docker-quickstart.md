<!-- Skip to main content -->
---
title: FraiseQL Docker Quickstart
description: Get FraiseQL running in 30 seconds without local Rust compilation.
keywords: []
tags: ["documentation", "reference"]
---

# FraiseQL Docker Quickstart

Get FraiseQL running in 30 seconds without local Rust compilation.

## Start in One Command

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml up -d
```text
<!-- Code example in TEXT -->

Wait for all services to be healthy (typically 15-30 seconds):

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml ps
```text
<!-- Code example in TEXT -->

## Open Your Browser

| Service | URL | Purpose |
|---------|-----|---------|
| **GraphQL IDE** | <http://localhost:3000> | Execute queries and explore schema |
| **Tutorial** | <http://localhost:3001> | Step-by-step interactive learning |
| **Admin Dashboard** | <http://localhost:3002> | System monitoring and debugging |
| **FraiseQL Server** | <http://localhost:8000> | GraphQL API endpoint |

## Try Your First Query

1. Open <http://localhost:3000> (GraphQL IDE)
2. Copy this query into the editor:

```graphql
<!-- Code example in GraphQL -->
query GetUsers {
  users(limit: 10) {
    id
    name
    email
    created_at
  }
}
```text
<!-- Code example in TEXT -->

1. Click the **Play** button
2. See the results in the right panel!

## What You're Running

### Services

| Service | Port | Technology | Purpose |
|---------|------|-----------|---------|
| **FraiseQL Server** | 8000 | Rust | GraphQL API (compiled schema) |
| **GraphQL IDE** | 3000 | Node.js | Browser-based query explorer |
| **Tutorial** | 3001 | Node.js | Step-by-step interactive learning |
| **Admin Dashboard** | 3002 | Node.js | System monitoring and debugging |
| **PostgreSQL** | 5432 | PostgreSQL | Blog database |

### Database

The PostgreSQL database comes pre-populated with sample blog data:

- **Users** (3 sample users)
- **Posts** (4 sample posts with authors)
- **Relationships** (Author → Posts)

## Example Queries

The `examples/basic/queries/` directory contains starter queries:

```bash
<!-- Code example in BASH -->
# List all users
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "query { users(limit: 10) { id name email } }"}'

# Get user by ID
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "query { user(id: 1) { id name email } }"}'

# Get posts by author
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "query { posts(filter: {author_id: 1}) { id title author { name } } }"}'
```text
<!-- Code example in TEXT -->

## Learn More

- **Understanding the Schema**: View `/examples/basic/schema.compiled.json`
- **Database Setup**: View `/examples/basic/sql/setup.sql`
- **Example Queries**: See `/examples/basic/queries/`
- **Full Documentation**: See `/docs/`

## Useful Commands

### View Service Logs

```bash
<!-- Code example in BASH -->
# All services
docker compose -f docker/docker-compose.demo.yml logs -f

# Specific service
docker compose -f docker/docker-compose.demo.yml logs -f FraiseQL-server
docker compose -f docker/docker-compose.demo.yml logs -f postgres-blog
```text
<!-- Code example in TEXT -->

### Stop Everything

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml down
```text
<!-- Code example in TEXT -->

### Remove Database Volume (Fresh Start)

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml down -v
```text
<!-- Code example in TEXT -->

### Check Service Health

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml ps
```text
<!-- Code example in TEXT -->

### Access PostgreSQL CLI

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml exec postgres-blog \
  psql -U FraiseQL -d blog_fraiseql
```text
<!-- Code example in TEXT -->

## Troubleshooting

### Services Won't Start

**Check if ports are in use:**

```bash
<!-- Code example in BASH -->
lsof -i :8000  # Check port 8000
lsof -i :3000  # Check port 3000
lsof -i :5432  # Check port 5432
```text
<!-- Code example in TEXT -->

**If ports are occupied**, either:

1. Stop the service using the port
2. Or modify `docker-compose.demo.yml` to use different ports

### GraphQL Server Not Responding

**Check if server is healthy:**

```bash
<!-- Code example in BASH -->
curl http://localhost:8000/health
```text
<!-- Code example in TEXT -->

**View server logs:**

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml logs FraiseQL-server
```text
<!-- Code example in TEXT -->

### Database Connection Failed

**Verify PostgreSQL is running:**

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml logs postgres-blog
```text
<!-- Code example in TEXT -->

**Check database directly:**

```bash
<!-- Code example in BASH -->
docker compose -f docker/docker-compose.demo.yml exec postgres-blog \
  pg_isready -U FraiseQL -d blog_fraiseql
```text
<!-- Code example in TEXT -->

## Next Steps

1. **Explore the Schema**: Browse types and fields in GraphQL IDE
2. **Try Sample Queries**: Use examples from `/examples/basic/queries/`
3. **Read Documentation**: See `/docs/GETTING_STARTED.md`
4. **Build Your Schema**: Learn to author schemas in `/docs/guides/schema-authoring.md`
5. **Deploy to Production**: See `/docs/deployment/guide.md`

## Using Make (Convenience)

If you have `make` installed, use these shortcuts:

```bash
<!-- Code example in BASH -->
make demo-start      # Start demo stack
make demo-stop       # Stop demo stack
make demo-logs       # View logs
make demo-status     # Check health
make demo-clean      # Remove volumes and stop
```text
<!-- Code example in TEXT -->

See `Makefile` for complete list of targets.

## What is FraiseQL?

FraiseQL is a **compiled GraphQL execution engine** that:

1. **Compiles at build time**: Schema → SQL (zero runtime overhead)
2. **Validates at compile time**: Catch errors before deployment
3. **Optimizes queries**: Generated SQL is automatically optimized
4. **Works with existing databases**: PostgreSQL, MySQL, SQLite, SQL Server
5. **Enterprise-ready**: Security, observability, federation built-in

Learn more: <https://github.com/anthropics/FraiseQL>

## Getting Help

- **Documentation**: `/docs/`
- **Examples**: `/examples/`
- **Issues**: <https://github.com/anthropics/FraiseQL/issues>
- **Community**: Discord/GitHub Discussions

## Docker Image Details

The demo uses the official FraiseQL Docker image built from the latest source. It includes:

- **FraiseQL-server**: GraphQL execution engine
- **FraiseQL-cli**: Schema compilation tool
- Pre-compiled schemas for examples

For production deployments, see `/docs/deployment/guide.md`.
