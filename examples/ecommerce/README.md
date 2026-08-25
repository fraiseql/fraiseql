# E-Commerce FraiseQL Example

A catalogue, its customers and their orders — the same domain the old
`ecommerce_api/` directory documented, authored the way FraiseQL v2 actually
works: Python declares the types, the Rust CLI compiles them, and the Rust
server executes them. There is no Python at runtime.

## Schema Overview

Five types, two of them reached only through nesting:

| Type | Source view | Notes |
|------|-------------|-------|
| `Category` | `v_category` | `productCount` computed by the view |
| `Product` | `v_product` | nested `category`, derived `inStock` |
| `Customer` | `v_customer` | `orderCount` and `lifetimeValue` computed by the view |
| `Order` | `v_order` | nested `customer`, nested list `items` |
| `OrderItem` | `v_order_item` | nested `product`; also queryable on its own |

Plus the `OrderStatus` enum (`PENDING`, `PAID`, `SHIPPED`, `DELIVERED`,
`CANCELLED`).

Sample data: 5 categories, 12 products, 5 customers, 7 orders, 11 order lines.

### Nesting is a view concern, not an N+1

`order { items { product { name } } }` is three levels deep and still one SQL
statement. `v_order` builds `data->'customer'` with `jsonb_build_object` and
`data->'items'` with `jsonb_agg`; the engine projects the selection set out of
that blob and recases the keys. Nothing issues a query per nested field.

### Where storage meets the surface

`tb_order.status` is a lowercase PostgreSQL enum. `OrderStatus`, like every
GraphQL enum, publishes uppercase value names. `v_order` reconciles the two with
`upper(o.status::text)` — the view is the projection layer, so that is where the
conversion belongs, not in the client.

## Quick Start

### 1. Set up the database

```bash
createdb ecommerce_example
psql -v ON_ERROR_STOP=1 -d ecommerce_example -f sql/setup.sql
```

`ON_ERROR_STOP=1` is not decoration: without it psql prints an error, continues,
and exits 0 on a half-loaded schema.

### 2. Generate and compile the schema

```bash
pip install fraiseql   # inside this repo: pip install -e sdks/official/fraiseql-python
python3 schema.py      # writes schema.json

cargo run -p fraiseql-cli -- compile schema.json -o schema.compiled.json
```

### 3. Ask it a question

The fastest end-to-end check needs no server:

```bash
export DATABASE_URL="postgresql://localhost/ecommerce_example"
cargo run -p fraiseql-cli -- query '{ products(limit: 3) { sku name price category { name } } }'
```

`fraiseql query` boots the compiled schema in-process, runs one operation and
exits non-zero if it does not resolve.

### 4. Or run the server

```bash
export DATABASE_URL="postgresql://localhost/ecommerce_example"
export FRAISEQL_SCHEMA_PATH="$PWD/schema.compiled.json"
cargo run -p fraiseql-server
```

```bash
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ categories { name productCount } }"}'
```

## Files

| File | Description |
|------|-------------|
| `schema.py` | Python schema definition (authoring only) |
| `schema.json` | Generated intermediate schema |
| `schema.compiled.json` | Compiled schema for the runtime (gitignored — step 2 makes it) |
| `sql/setup.sql` | Tables, views and sample data |
| `queries/*.graphql` | Example queries, all of them runnable |

## Queries

| File | Shows |
|------|-------|
| `01-product-listing.graphql` | the catalogue with each product's category |
| `02-filter-out-of-stock.graphql` | `where` filtering on a derived field |
| `03-customer-order-history.graphql` | two root fields in one operation, filtering through a nested object |
| `04-order-analysis.graphql` | three levels of nesting in one statement |
| `05-categories.graphql` | view-computed aggregates |

Run one against the database:

```bash
cargo run -p fraiseql-cli -- query \
  --variables '{"customerId": "c3000000-0000-4000-8000-000000000001"}' \
  "$(cat queries/03-customer-order-history.graphql)"
```

The seed data uses fixed UUIDs, so the ids in these examples are stable across a
rebuild.

## Architecture

```
schema.py (Python SDK)
    │
    ▼ (generates)
schema.json (intermediate)
    │
    ▼ (fraiseql compile)
schema.compiled.json (runtime)
    │
    ▼ (loaded by)
fraiseql-server
    │
    ▼ (reads)
PostgreSQL — v_category, v_product, v_customer, v_order, v_order_item
```
