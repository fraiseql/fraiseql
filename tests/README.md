# FraiseQL Test Files

This directory contains integration test setup and SQL initialization scripts for test databases.

---

## Directory Structure

```
tests/
├── README.md                     # This file
└── sql/                          # Database initialization scripts
    ├── postgres/                 # PostgreSQL test setup
    │   ├── init.sql              # Standard PostgreSQL test data
    │   └── init-vector.sql       # pgvector + full-text search test data
    └── mysql/                    # MySQL test setup
        └── init.sql              # MySQL test data
```

---

## SQL Initialization Scripts

### PostgreSQL - `postgres/init.sql`

Creates test views for core integration tests:

- **`v_user`** - 5 test users
  - Fields: id, email, name, age, active, role, tags, metadata
  - Tests: string operators, boolean, arrays, JSONB, null checks

- **`v_post`** - 4 test posts
  - Fields: id, title, content, author (nested), published, views, tags
  - Tests: nested objects, joins, boolean, numeric comparisons

- **`v_product`** - 4 test products
  - Fields: id, name, price, stock, category, attributes
  - Tests: numeric operators (price, stock), JSONB attributes

**Container:** `fraiseql-postgres-test` on port `5433`

### PostgreSQL + pgvector - `postgres/init-vector.sql`

Creates test views for advanced PostgreSQL features:

- **`v_embedding`** - 5 test embeddings with 3D vectors
  - Tests: vector distance operators (cosine, L2, L1, Hamming)

- **`v_document`** - 4 test documents with full-text search
  - Tests: full-text search operators (matches, plain_query, phrase_query, websearch_query)

**Container:** `fraiseql-postgres-vector-test` on port `5434`

### MySQL - `mysql/init.sql`

Creates equivalent test views for MySQL compatibility:

- **`v_user`** - Same schema as PostgreSQL
- **`v_post`** - Same schema as PostgreSQL
- **`v_product`** - Same schema as PostgreSQL

**Container:** `fraiseql-mysql-test` on port `3307`

**Note:** MySQL uses `JSON` type (not `JSONB`) and has different SQL functions.

---

## Test Data

All test data uses fixed UUIDs for predictable test assertions:

```sql
-- Users
00000000-0000-0000-0000-000000000001  alice@example.com (admin)
00000000-0000-0000-0000-000000000002  bob@example.com (user)
00000000-0000-0000-0000-000000000003  charlie@test.com (moderator)
00000000-0000-0000-0000-000000000004  diana@example.com (user)
00000000-0000-0000-0000-000000000005  eve@test.com (user)

-- Posts
00000000-0000-0000-0000-000000000101  Introduction to GraphQL
00000000-0000-0000-0000-000000000102  Rust Performance
00000000-0000-0000-0000-000000000103  Draft Post
00000000-0000-0000-0000-000000000104  PostgreSQL Tips

-- Products
00000000-0000-0000-0000-000000000201  Laptop ($999.99)
00000000-0000-0000-0000-000000000202  Mouse ($29.99)
00000000-0000-0000-0000-000000000203  Desk ($299.99)
00000000-0000-0000-0000-000000000204  Chair ($199.99)

-- Embeddings (pgvector)
00000000-0000-0000-0000-000000000301  [1.0, 0.0, 0.0]
00000000-0000-0000-0000-000000000302  [0.0, 1.0, 0.0]
00000000-0000-0000-0000-000000000303  [0.0, 0.0, 1.0]
00000000-0000-0000-0000-000000000304  [0.9, 0.1, 0.0]
00000000-0000-0000-0000-000000000305  [0.1, 0.9, 0.0]

-- Documents (full-text search)
00000000-0000-0000-0000-000000000401  GraphQL Introduction
00000000-0000-0000-0000-000000000402  Rust Programming
00000000-0000-0000-0000-000000000403  PostgreSQL Guide
00000000-0000-0000-0000-000000000404  API Design
```

---

## Modifying Test Data

### Add New Test Data

1. Edit the appropriate SQL file:
   - `postgres/init.sql` for standard PostgreSQL
   - `postgres/init-vector.sql` for pgvector/full-text
   - `mysql/init.sql` for MySQL

2. Reset the databases:
   ```bash
   make db-reset
   ```

3. Verify the data:
   ```bash
   make db-verify
   ```

### Add New Test Views

1. Create the table and view in the SQL file
2. Grant SELECT permission to `fraiseql_test` user
3. Add verification query at the end
4. Reset databases to apply changes

Example:

```sql
-- Create table
CREATE TABLE IF NOT EXISTS orders_test (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    user_id UUID REFERENCES users_test(id),
    total NUMERIC(10, 2) NOT NULL,
    status TEXT DEFAULT 'pending'
);

-- Insert test data
INSERT INTO orders_test (id, user_id, total, status) VALUES
    ('00000000-0000-0000-0000-000000000501', '00000000-0000-0000-0000-000000000001', 99.99, 'completed');

-- Create JSONB view
CREATE OR REPLACE VIEW v_order AS
SELECT
    jsonb_build_object(
        'id', id::text,
        'user_id', user_id::text,
        'total', total,
        'status', status
    ) AS data
FROM orders_test;

-- Grant permissions
GRANT SELECT ON v_order TO fraiseql_test;

-- Verify
SELECT 'v_order' AS view_name, COUNT(*) AS row_count FROM v_order;
```

---

## Integration Test Directory (Future)

Integration tests will be in `tests/integration/`:

```
tests/
├── integration/                  # Integration tests (Phase 2+)
│   ├── postgres_test.rs
│   ├── mysql_test.rs
│   └── sqlite_test.rs
└── e2e/                          # End-to-end tests (Phase 6+)
    ├── http_server_test.rs
    └── graphql_query_test.rs
```

**Current:** Integration tests are in `crates/fraiseql-core/src/db/postgres/adapter.rs` with `#[ignore]` attribute.

---

## Running Tests

See [TESTING.md](../TESTING.md) for complete testing guide.

**Quick start:**

```bash
# Start test databases
make db-up

# Run integration tests
cargo test -- --ignored

# Stop databases
make db-down
```
