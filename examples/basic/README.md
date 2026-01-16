# Basic FraiseQL Example

A simple blog-style schema demonstrating FraiseQL's core features.

## Schema Overview

This example includes:
- **User** type with id, name, email, created_at
- **Post** type with id, title, content, author_id, created_at
- Queries for listing and fetching users/posts
- WHERE clause filtering support

## Quick Start

### 1. Set Up Database

```bash
# PostgreSQL
psql -U postgres -c "CREATE DATABASE fraiseql_example;"
psql -U postgres -d fraiseql_example -f sql/setup.sql
```

### 2. Generate Schema (Python SDK)

```bash
cd examples/basic
python3 schema.py
# Creates: schema.json
```

### 3. Compile Schema

```bash
# From project root
cargo run -p fraiseql-cli -- compile examples/basic/schema.json -o examples/basic/schema.compiled.json
```

### 4. Run Server

```bash
export DATABASE_URL="postgresql://postgres:postgres@localhost/fraiseql_example"
export FRAISEQL_SCHEMA_PATH="examples/basic/schema.compiled.json"
cargo run -p fraiseql-server
```

### 5. Execute Queries

```bash
# Get all users
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name email } }"}'

# Get user by ID
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ user(id: 1) { id name email posts { title } } }"}'
```

## Files

| File | Description |
|------|-------------|
| `schema.py` | Python schema definition using FraiseQL SDK |
| `schema.json` | Generated intermediate schema |
| `schema.compiled.json` | Compiled schema for runtime |
| `sql/setup.sql` | Database setup (tables, views, sample data) |
| `queries/*.graphql` | Example GraphQL queries |

## GraphQL Queries

See `queries/` directory for example queries:
- `list_users.graphql` - List all users
- `get_user.graphql` - Get user by ID with posts
- `filter_posts.graphql` - Filter posts by author

## Architecture

```
schema.py (Python SDK)
    │
    ▼ (generates)
schema.json (intermediate)
    │
    ▼ (fraiseql-cli compile)
schema.compiled.json (runtime)
    │
    ▼ (loaded by)
fraiseql-server
    │
    ▼ (connects to)
PostgreSQL (v_users, v_posts views)
```
