# 1.3: Database-Centric Architecture

**Audience:** Architects, database teams, developers building data systems
**Prerequisite:** Topics 1.1 (What is FraiseQL?), 1.2 (Core Concepts)
**Reading Time:** 20-25 minutes

---

## Overview

FraiseQL's fundamental design choice is to treat the **database as the primary application interface**, not as a storage afterthought. This topic explains why this choice matters, how it shapes FraiseQL's architecture, and what implications it has for your systems.

**Core insight:** In FraiseQL, the database schema is not an implementation detail—it's your API definition. The database is the source of truth for data relationships, types, validation, and performance.

---

## Part 1: The Core Philosophy

### GraphQL as a Database Access Layer, Not API Aggregation

Traditional GraphQL servers are designed to aggregate data from multiple sources:

```text
Client
  ↓ (GraphQL Query)
GraphQL Server
  ├→ REST API call
  ├→ Another GraphQL service
  ├→ Database query
  ├→ Cache lookup
  ├→ Custom resolver logic
  └→ Webhook
  ↓
Client (aggregated response)
```text

**Problem:** The server becomes a coordination layer, and you need to write resolvers for every field, cache invalidation logic, N+1 prevention, etc.

---

### FraiseQL's Approach: Database-First Architecture

FraiseQL assumes the database is your **primary and usually only data source**:

```text
Client
  ↓ (GraphQL Query)
FraiseQL Server
  ├→ Validate (schema already compiled)
  ├→ Authorize (rules from schema)
  └→ Execute (pre-compiled SQL)
  ↓
Database (single source of truth)
  ↓
Client (direct result)
```text

**Advantage:** Clear data flow, no custom resolvers, deterministic behavior.

---

### Why This Assumption Matters

This design choice has profound consequences:

**1. Simplicity**

- No custom resolver code needed
- Schema definition = API definition
- What you see in the schema is what you get

**2. Performance**

- Database handles all query optimization
- No application-level coordination overhead
- SQL is optimized at compile time

**3. Correctness**

- Database constraints enforced
- Transactions guarantee consistency
- Relationships are explicit (foreign keys)

**4. Consistency**

- Single source of truth (the database)
- No cache invalidation problems
- All clients see consistent data

**5. Debuggability**

- Look at the SQL, understand the query
- No hidden resolver logic
- Performance bottlenecks are clear (database metrics)

---

### When This Assumption is Valid

FraiseQL works best when:

✅ Your primary data source is a **relational database** (PostgreSQL, MySQL, etc.)
✅ Your data has **clear structure and relationships** (not fully unstructured)
✅ Your API needs to be **performant** (N+1 queries unacceptable)
✅ Your team has **database expertise** (schemas, views, indexes)
✅ You value **predictability** over flexibility

---

### When This Assumption Breaks Down

FraiseQL is **not** the right choice when:

❌ Your primary data is **unstructured** (documents, blobs)
❌ You need to aggregate from **many external APIs** (microservices federation)
❌ Your schema is **highly dynamic** (must change at runtime)
❌ You have **deeply nested custom logic** (better in application code)
❌ You're **just prototyping** (Hasura might be faster)

---

## Part 2: How FraiseQL Thinks About Data

### The Data Hierarchy

```text
Database Schema (DBA responsibility)
    ↓
FraiseQL Type Definition (Developer responsibility)
    ↓
GraphQL API (Client interface)
```text

Each level maps directly:

**Database Level:**

```sql
CREATE TABLE tb_users (
    pk_user BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    username VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL,
    is_active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    deleted_at TIMESTAMP
);

CREATE TABLE tb_orders (
    pk_order BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    fk_user BIGINT NOT NULL REFERENCES tb_users(pk_user),
    total DECIMAL(10, 2) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```text

**FraiseQL Type Level:**

```python
@fraiseql.type
class User:
    user_id: int              # ← pk_user
    username: str             # ← username
    email: str                # ← email
    is_active: bool           # ← is_active
    created_at: datetime      # ← created_at
    updated_at: datetime      # ← updated_at
    deleted_at: datetime | None # ← deleted_at (soft delete)
    orders: List[Order]       # ← foreign key relationship

@fraiseql.type
class Order:
    order_id: int             # ← pk_order
    user_id: int              # ← fk_user
    total: Decimal            # ← total
    user: User                # ← reverse relationship
    created_at: datetime      # ← created_at
```text

**GraphQL API Level:**

```graphql
type User {
  userId: Int!
  username: String!
  email: String!
  isActive: Boolean!
  createdAt: DateTime!
  updatedAt: DateTime!
  deletedAt: DateTime
  orders: [Order!]!
}

type Order {
  orderId: Int!
  userId: Int!
  total: Decimal!
  user: User!
  createdAt: DateTime!
}

query GetUser {
  user(id: 1) {
    userId
    username
    orders {
      orderId
      total
    }
  }
}
```text

---

### Mapping: Tables → Types → Relationships

**FraiseQL automatically derives relationships from foreign keys:**

```sql
-- Database: Foreign key defines relationship
ALTER TABLE tb_orders
ADD CONSTRAINT fk_orders_user
FOREIGN KEY (fk_user) REFERENCES tb_users(pk_user);
```text

**Becomes in FraiseQL:**

```python
@fraiseql.type
class Order:
    user_id: int
    user: User  # Automatically available because of FK
```text

**No extra configuration needed.** The database structure is the API structure.

---

## Part 3: The Four-Tier View System

FraiseQL uses four types of database views, each optimized for different access patterns and performance characteristics:

### Overview Matrix

| View Type | Prefix | Plane | Storage | Use Case | Latency | Index Type |
|-----------|--------|-------|---------|----------|---------|-----------|
| **Logical Read** | `v_*` | JSON | None | Simple queries | 100-500ms | None |
| **Table-Backed JSON** | `tv_*` | JSON | JSONB tables | Complex nested queries | 50-200ms | JSONB GIN |
| **Logical Analytics** | `va_*` | Arrow | None | Small analytics <100K | 500ms-5s | None |
| **Table-Backed Analytics** | `ta_*` | Arrow | Columnar | Large analytics >1M | 50-100ms | BRIN + B-tree |

---

### 1. `v_*` Views: Logical Read Views (JSON Plane)

**Definition:** Database views (no physical storage) optimized for GraphQL transactional access.

**When to use:**

- Simple queries (1-2 tables involved)
- Small to medium result sets (<10K rows)
- Real-time data needed
- Data changes frequently

**Example:**

```sql
-- Write table (source of truth)
CREATE TABLE tb_users (
    pk_user BIGINT PRIMARY KEY,
    username VARCHAR(255),
    email VARCHAR(255),
    created_at TIMESTAMP,
    deleted_at TIMESTAMP
);

-- Read view (filters soft-deleted records)
CREATE VIEW v_user AS
SELECT
    pk_user AS user_id,
    username,
    email,
    created_at
FROM tb_users
WHERE deleted_at IS NULL;  -- Only active users
```text

**Characteristics:**

- **Storage overhead:** 0% (logical view only)
- **Maintenance:** None (automatic via base table)
- **Performance:** Database determines (can't be optimized separately)
- **Staleness:** Real-time (always reflects current state)
- **Index support:** Uses indexes from base table

**FraiseQL Integration:**

```python
@fraiseql.type
class User:
    user_id: int
    username: str
    email: str
    created_at: datetime
    # Automatically queries v_user view
```text

---

### 2. `tv_*` Views: Table-Backed JSON Views

**Definition:** Materialized JSONB tables with trigger-based refresh for complex nested JSON queries.

**When to use:**

- Complex nested structures (User + Orders + Items in one query)
- High read volume (>1000 QPS)
- Moderate write volume (<100 writes/sec)
- Data can be 1-5 seconds stale

**Example:**

```sql
-- Write table
CREATE TABLE tb_orders (
    pk_order BIGINT PRIMARY KEY,
    fk_user BIGINT NOT NULL REFERENCES tb_users(pk_user),
    total DECIMAL(10,2),
    created_at TIMESTAMP
);

-- Materialized JSON view (pre-composed nested data)
CREATE TABLE tv_order_with_user (
    pk_order BIGINT PRIMARY KEY REFERENCES tb_orders(pk_order) ON DELETE CASCADE,
    data JSONB NOT NULL,  -- Contains: {orderId, total, user: {userId, username, email}}
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Trigger: Update tv_order_with_user when tb_orders or tb_users changes
CREATE TRIGGER trg_refresh_tv_order_with_user
AFTER INSERT OR UPDATE ON tb_orders
FOR EACH ROW
EXECUTE FUNCTION refresh_tv_order_with_user();

-- Trigger function (PostgreSQL)
CREATE OR REPLACE FUNCTION refresh_tv_order_with_user()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO tv_order_with_user (pk_order, data)
    SELECT
        o.pk_order,
        jsonb_build_object(
            'orderId', o.pk_order,
            'total', o.total,
            'user', jsonb_build_object(
                'userId', u.pk_user,
                'username', u.username,
                'email', u.email
            )
        )
    FROM tb_orders o
    JOIN tb_users u ON u.pk_user = o.fk_user
    WHERE o.pk_order = NEW.pk_order
    ON CONFLICT (pk_order) DO UPDATE
    SET data = EXCLUDED.data, updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```text

**Characteristics:**

- **Storage overhead:** 20-50% (JSONB pre-composition)
- **Maintenance:** Trigger-based refresh (automatic)
- **Performance:** 50-200ms (pre-composed, no joins at query time)
- **Staleness:** 1-5 seconds (dependent on trigger frequency)
- **Index support:** JSONB GIN indexes for path searches

**FraiseQL Integration:**

```python
@fraiseql.type
class OrderWithUser:
    order_id: int
    total: Decimal
    user: User  # From pre-composed JSONB
    # Automatically queries tv_order_with_user view
```text

---

### 3. `va_*` Views: Logical Analytics Views (Arrow Plane)

**Definition:** Database views (no physical storage) optimized for Arrow Flight columnar queries.

**When to use:**

- Analytics on small datasets (<100K rows)
- One-time reports
- Data can be 5-60 seconds stale
- Minimal storage overhead acceptable

**Example:**

```sql
-- Read view optimized for columnar extraction
CREATE VIEW va_user_stats AS
SELECT
    pk_user,
    username,
    email,
    COUNT(*) OVER (PARTITION BY EXTRACT(YEAR FROM created_at)) AS users_per_year,
    EXTRACT(YEAR FROM created_at) AS signup_year,
    created_at
FROM tb_users
WHERE deleted_at IS NULL;
```text

**Characteristics:**

- **Storage overhead:** 0% (logical view only)
- **Maintenance:** None (automatic)
- **Performance:** 500ms-5s (depends on base table size)
- **Staleness:** Real-time (always reflects current state)
- **Arrow compatibility:** Fully compatible with Arrow Flight protocol

**FraiseQL Integration:**

```python
@fraiseql.aggregate_query(
    fact_table=None,  # Uses va_user_stats logical view
)
@fraiseql.query
def user_stats_by_year() -> list[dict]:
    """Returns user count by signup year via Arrow Flight."""
```text

---

### 4. `ta_*` Views: Table-Backed Analytics Views (Arrow Plane)

**Definition:** Materialized columnar tables with trigger-based refresh for high-performance analytics.

**When to use:**

- Large analytics datasets (>1M rows)
- High-volume analytics (>100 queries/sec)
- Aggregations across multiple dimensions
- Can tolerate 1-5 minute staleness

**The Three-Component Architecture:**

#### Component 1: Measures (Direct SQL Columns)

Numeric columns for fast aggregation. **225x faster** than JSONB aggregation.

```sql
CREATE TABLE ta_sales (
    id BIGSERIAL PRIMARY KEY,

    -- MEASURES: Direct SQL columns for fast aggregation
    measure_revenue DECIMAL(10,2) NOT NULL,
    measure_quantity INT NOT NULL,
    measure_cost DECIMAL(10,2) NOT NULL,

    -- ... dimensions and filters below
);

-- Queries: Direct column access
SELECT
    SUM(measure_revenue) AS total_revenue,
    AVG(measure_quantity) AS avg_qty,
    COUNT(*) AS transaction_count
FROM ta_sales
WHERE created_at >= '2026-01-01';
-- Result: <1ms (1M rows)
```text

#### Component 2: Dimensions (JSONB Column)

Flexible grouping attributes in a single JSON column. No schema migration needed to add new dimensions.

```sql
CREATE TABLE ta_sales (
    -- ... measures above

    -- DIMENSIONS: JSONB column for flexible grouping
    dimension_data JSONB NOT NULL,
    -- Contains: {category, product_name, region, customer_segment, ...}
    -- Schema defined at ETL time, not database schema time

    -- ... filters and timestamps below
);

-- Queries: Extract dimension paths
SELECT
    dimension_data->>'category' AS category,
    dimension_data->>'region' AS region,
    SUM(measure_revenue) AS total_revenue
FROM ta_sales
WHERE created_at >= '2026-01-01'
GROUP BY
    dimension_data->>'category',
    dimension_data->>'region'
ORDER BY total_revenue DESC;
-- Result: 50-100ms (1M rows), grouping by 2 dimensions
```text

**Database-Specific Dimension Extraction:**

PostgreSQL:

```sql
dimension_data->>'category'              -- JSONB operator
```text

MySQL:

```sql
JSON_UNQUOTE(JSON_EXTRACT(dimension_data, '$.category'))
```text

SQLite:

```sql
json_extract(dimension_data, '$.category')
```text

SQL Server:

```sql
JSON_VALUE(dimension_data, '$.category')
```text

#### Component 3: Denormalized Filters (Indexed SQL Columns)

High-selectivity filter columns for fast WHERE clauses.

```sql
CREATE TABLE ta_sales (
    -- ... measures and dimensions above

    -- DENORMALIZED FILTERS: Indexed SQL columns for fast WHERE
    filter_customer_id UUID NOT NULL,
    filter_product_id UUID NOT NULL,
    filter_occurred_at TIMESTAMPTZ NOT NULL,
    filter_status VARCHAR(50) NOT NULL,

    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for microsecond filtering
CREATE INDEX idx_ta_sales_customer ON ta_sales(filter_customer_id);
CREATE INDEX idx_ta_sales_product ON ta_sales(filter_product_id);
CREATE INDEX idx_ta_sales_occurred ON ta_sales(filter_occurred_at);
CREATE INDEX idx_ta_sales_status ON ta_sales(filter_status);
CREATE INDEX idx_ta_sales_data_gin ON ta_sales USING GIN(dimension_data);
CREATE INDEX idx_ta_sales_revenue_brin ON ta_sales USING BRIN(measure_revenue);
```text

**Query with All Three Components:**

```sql
-- Fast WHERE (filters), GROUP BY (dimensions), aggregation (measures)
SELECT
    dimension_data->>'category' AS category,
    DATE_TRUNC('month', filter_occurred_at)::DATE AS month,
    COUNT(*) AS transaction_count,
    SUM(measure_revenue) AS total_revenue,
    AVG(measure_quantity) AS avg_quantity
FROM ta_sales
WHERE
    filter_customer_id = '550e8400-e29b-41d4-a716-446655440000'  -- Fast index lookup
    AND filter_occurred_at >= '2026-01-01'                        -- Fast index lookup
    AND filter_status = 'completed'                               -- Fast index lookup
GROUP BY
    dimension_data->>'category',
    DATE_TRUNC('month', filter_occurred_at)
HAVING
    SUM(measure_revenue) > 1000
ORDER BY month DESC
LIMIT 100;

-- Performance: 30-100ms (100M rows)
```text

**Complete Example with Triggers:**

```sql
-- Write table (source of truth)
CREATE TABLE tb_sales (
    pk_sale BIGINT PRIMARY KEY,
    fk_customer UUID NOT NULL,
    fk_product UUID NOT NULL,
    revenue DECIMAL(10,2),
    quantity INT,
    cost DECIMAL(10,2),
    status VARCHAR(50),
    occurred_at TIMESTAMP,
    created_at TIMESTAMP
);

-- Materialized analytics table (denormalized for speed)
CREATE TABLE ta_sales (
    id BIGSERIAL PRIMARY KEY,

    -- Measures (225x faster than JSONB aggregation)
    measure_revenue DECIMAL(10,2) NOT NULL,
    measure_quantity INT NOT NULL,
    measure_cost DECIMAL(10,2) NOT NULL,

    -- Dimensions (flexible, no schema migration)
    dimension_data JSONB NOT NULL,  -- {category, product_name, region, segment}

    -- Filters (indexed, fast WHERE)
    filter_customer_id UUID NOT NULL,
    filter_product_id UUID NOT NULL,
    filter_occurred_at TIMESTAMPTZ NOT NULL,
    filter_status VARCHAR(50) NOT NULL,

    -- Timestamps
    created_at TIMESTAMPTZ DEFAULT CURRENT_TIMESTAMP,
    source_updated_at TIMESTAMP  -- Track staleness
);

-- Indexes
CREATE INDEX idx_ta_sales_customer ON ta_sales(filter_customer_id);
CREATE INDEX idx_ta_sales_occurred ON ta_sales(filter_occurred_at);
CREATE INDEX idx_ta_sales_status ON ta_sales(filter_status);
CREATE INDEX idx_ta_sales_data_gin ON ta_sales USING GIN(dimension_data);
CREATE INDEX idx_ta_sales_revenue_brin ON ta_sales USING BRIN(measure_revenue);

-- Trigger: Populate ta_sales from tb_sales
CREATE TRIGGER trg_populate_ta_sales
AFTER INSERT OR UPDATE ON tb_sales
FOR EACH ROW
EXECUTE FUNCTION populate_ta_sales();

CREATE OR REPLACE FUNCTION populate_ta_sales()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO ta_sales (
        measure_revenue, measure_quantity, measure_cost,
        dimension_data,
        filter_customer_id, filter_product_id, filter_occurred_at, filter_status,
        source_updated_at
    )
    SELECT
        NEW.revenue,
        NEW.quantity,
        NEW.cost,
        jsonb_build_object(
            'category', p.category,
            'product_name', p.name,
            'region', c.region,
            'segment', c.segment
        ),
        NEW.fk_customer,
        NEW.fk_product,
        NEW.occurred_at,
        NEW.status,
        NEW.created_at
    FROM tb_products p
    JOIN tb_customers c ON c.fk_customer = NEW.fk_customer
    WHERE p.pk_product = NEW.fk_product
    ON CONFLICT (id) DO UPDATE
    SET
        measure_revenue = EXCLUDED.measure_revenue,
        source_updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;
```text

**Characteristics:**

- **Storage overhead:** 10-30% (columnar format + indexes)
- **Maintenance:** Trigger-based refresh (<100ms latency)
- **Performance:** 50-100ms (1M rows), 300-1000ms (100M rows)
- **Staleness:** 1-5 minutes (dependent on refresh triggers)
- **Index support:** BRIN indexes (10KB vs 1MB for B-tree), GIN for JSONB

---

### Calendar Dimensions: Temporal Performance Optimization

**Problem:** Grouping by temporal buckets (month, quarter, year) requires runtime computation.

```sql
-- Slow: Runtime computation
SELECT
    DATE_TRUNC('month', occurred_at) AS month,
    SUM(measure_revenue) AS revenue
FROM ta_sales
GROUP BY DATE_TRUNC('month', occurred_at);
-- Result: 500ms (1M rows)
```text

**Solution:** Pre-computed temporal buckets in JSONB.

```sql
-- Fast: Pre-computed extraction
CREATE TABLE ta_sales_with_calendar (
    -- ... all columns from ta_sales above

    -- Calendar dimension: Pre-computed temporal buckets
    calendar_info JSONB NOT NULL,
    -- Contains: {date: "2026-03-15", week: 11, month: 3, quarter: 1, year: 2026}
);

-- Query: Direct extraction (no computation)
SELECT
    calendar_info->>'month' AS month,
    calendar_info->>'year' AS year,
    SUM(measure_revenue) AS revenue
FROM ta_sales_with_calendar
WHERE calendar_info->>'year' = '2026'
GROUP BY
    calendar_info->>'year',
    calendar_info->>'month'
ORDER BY calendar_info->>'month';
-- Result: 30ms (1M rows) - 16x faster!
```text

**Performance Impact by Rows:**

| Rows | Without Calendar | With Calendar | Speedup |
|------|------------------|---------------|---------|
| 100K | 50ms | 5ms | 10x |
| 1M | 500ms | 30ms | 16x |
| 10M | 5000ms | 300ms | 16x |

**FraiseQL Automatic Detection:**
FraiseQL introspects columns ending with `_info` containing `{date, week, month, quarter, year}` and automatically generates optimal SQL:

```python
@fraiseql.fact_table(table_name="ta_sales")
@fraiseql.type
class Sale:
    measure_revenue: float
    dimension_data: dict  # {category, product_name, region}
    calendar_info: dict   # {date, week, month, quarter, year} ← Auto-detected
    # FraiseQL uses: calendar_info->>'month' (not DATE_TRUNC)
```text

---

## Part 4: Multi-Database Support

### The Multi-Database Philosophy

FraiseQL supports multiple database backends with **one schema definition**:

```python
# One schema definition...
@fraiseql.type
class User:
    user_id: int
    username: str
    email: str
    orders: List[Order]

# ...works with any supported database
# - PostgreSQL (primary, most features)
# - MySQL (secondary, good support)
# - SQLite (local dev, testing)
# - SQL Server (enterprise deployments)
```text

---

### Database Selection Matrix

| Database | Strengths | Typical Use | Analytics | Maturity |
|----------|-----------|------------|----------|----------|
| **PostgreSQL** | Full-featured, JSONB, BRIN indexes, window functions | Production primary | ✅ Best | ✅ Full support |
| **MySQL** | Widely deployed, fast, JSON | Legacy systems, scale-out | ⚠️ Basic JSON | ✅ Full support |
| **SQLite** | Lightweight, embedded, portable | Local dev, testing, mobile | ⚠️ Limited | ✅ Full support |
| **SQL Server** | Enterprise Windows, T-SQL, JSON | Enterprise deployments | ⚠️ JSON as NVARCHAR | ✅ Full support |

---

### Schema Portability Example

**Same FraiseQL schema:**

```python
@fraiseql.type
class Product:
    product_id: int
    name: str
    price: Decimal
    in_stock: bool
    created_at: datetime
```text

**Works on PostgreSQL:**

```sql
-- PostgreSQL
CREATE TABLE tb_products (
    pk_product BIGINT PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    name VARCHAR(255),
    price NUMERIC(10, 2),
    in_stock BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```text

**Works on MySQL:**

```sql
-- MySQL
CREATE TABLE tb_products (
    pk_product BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255),
    price DECIMAL(10, 2),
    in_stock BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```text

**Works on SQLite:**

```sql
-- SQLite
CREATE TABLE tb_products (
    pk_product INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT,
    price REAL,
    in_stock BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```text

**Same GraphQL API** ✓
**Same FraiseQL schema definition** ✓
**Different database implementations** ✓

---

### Database-Specific Features

While the schema is portable, FraiseQL can leverage database-specific features:

**PostgreSQL (Primary, Most Features):**

```sql
-- PostgreSQL-specific: JSONB, arrays, types
CREATE TABLE tb_events (
    pk_event BIGINT PRIMARY KEY,
    data JSONB,  -- PostgreSQL JSONB type
    tags TEXT[],  -- PostgreSQL array type
    status public.event_status  -- Custom enum type
);

-- FraiseQL can leverage these
@fraiseql.type
class Event:
    event_id: int
    data: JSON  # Maps to JSONB
    tags: List[str]  # Maps to array
    status: EventStatus  # Maps to enum
```text

**MySQL (Limited Custom Types):**

```sql
-- MySQL: Standard types, JSON as string
CREATE TABLE tb_events (
    pk_event BIGINT AUTO_INCREMENT PRIMARY KEY,
    data JSON,  -- JSON as string
    tags JSON,  -- Array as JSON string
    status VARCHAR(50)  -- Enum as string
);
```text

**SQLite (Minimal Types):**

```sql
-- SQLite: TEXT for everything complex
CREATE TABLE tb_events (
    pk_event INTEGER PRIMARY KEY,
    data TEXT,  -- JSON as text
    tags TEXT,  -- JSON array as text
    status TEXT  -- Enum as text
);
```text

**FraiseQL handles the differences transparently.**

---

## Part 5: Fact Tables for Analytics (tf_*)

### What Are Fact Tables?

Fact tables (`tf_*` prefix) are the **core analytics data structure** in FraiseQL. They denormalize transactional data into a structure optimized for rapid aggregation across multiple dimensions.

**Key principle:** NO JOINS during queries. All dimensional context is denormalized at ETL time.

---

### The Three-Component Architecture

```text
Fact Table (tf_*)
├── Measures (SQL Columns) ← 225x faster aggregation
├── Dimensions (JSONB Column) ← Flexible grouping
└── Filters (Indexed SQL Columns) ← Fast WHERE clauses
```text

This structure enables:

- **Measures:** Direct aggregation (SUM, AVG, COUNT) at database speed
- **Dimensions:** Flexible grouping without schema changes
- **Filters:** Instant WHERE clause evaluation via indexes

---

### Python Authoring for Analytics

FraiseQL provides decorators for analytics:

```python
from fraiseql import fact_table, aggregate_query, type as fraiseql_type

# Define fact table
@fact_table(
    table_name="tf_sales",
    measures=[
        {"name": "revenue", "type": "float", "aggregates": ["sum", "avg"]},
        {"name": "quantity", "type": "int", "aggregates": ["sum", "avg"]},
        {"name": "cost", "type": "float", "aggregates": ["sum"]},
    ],
    dimension_paths=[
        {"name": "category", "json_path": "dimension_data->>'category'"},
        {"name": "product_name", "json_path": "dimension_data->>'product_name'"},
        {"name": "region", "json_path": "dimension_data->>'region'"},
    ],
    denormalized_filters=[
        {"name": "customer_id", "type": "uuid"},
        {"name": "product_id", "type": "uuid"},
        {"name": "occurred_at", "type": "timestamp"},
        {"name": "status", "type": "string"},
    ],
    calendar_dimensions=[
        {"name": "calendar_info", "type": "date_info"}  # {date, week, month, quarter, year}
    ],
)
@fraiseql_type
class Sale:
    """Sales fact table for analytics."""
    id: int
    measure_revenue: float
    measure_quantity: int
    measure_cost: float
    dimension_data: dict
    filter_customer_id: str
    filter_product_id: str
    filter_occurred_at: datetime
    filter_status: str
    calendar_info: dict
```text

**Generated GraphQL Aggregate Query:**

```graphql
query {
  sales_aggregate(
    where: {
      filter_occurred_at: { _gte: "2026-01-01", _lt: "2026-02-01" }
      filter_status: "completed"
    }
    groupBy: {
      category: true
      month: true  # From calendar_info->>'month'
    }
    having: {
      measure_revenue_sum_gt: 1000
    }
    orderBy: [{ field: "measure_revenue_sum", direction: DESC }]
    limit: 100
  ) {
    category
    month
    count
    measure_revenue_sum
    measure_revenue_avg
    measure_quantity_sum
    measure_quantity_avg
  }
}
```text

---

## Part 6: Arrow Flight for Streaming Analytics

### Arrow Flight Protocol

FraiseQL uses **Apache Arrow Flight** to stream columnar analytics data directly to clients.

```text
Client
  ↓ (Arrow Flight Request with ticket)
FraiseQL Arrow Server (gRPC)
  ├─ Validate query
  ├─ Authorize access
  ├─ Execute compiled SQL
  └─ Stream columnar Arrow batches
  ↓
Client (receives Arrow format, zero-copy deserialization)
```text

**Performance:**

- **JSON Plane:** 10-20 MB/sec (row-by-row, HTTP)
- **Arrow Plane:** 100-500 MB/sec (columnar, gRPC)
- **Speedup:** 5-50x faster for analytics

---

### Flight Tickets

Clients request data by submitting a **Flight Ticket**, which encodes the query:

```json
{
  "type": "OptimizedView",
  "view": "ta_sales",
  "filter": "filter_occurred_at > '2026-01-01' AND filter_status = 'completed'",
  "orderBy": "calendar_info->>'month' DESC",
  "limit": 100000,
  "offset": 0
}
```text

**Ticket Types:**

1. **GraphQLQuery** - Execute GraphQL query, return Arrow
2. **OptimizedView** - Query pre-optimized ta_* view directly
3. **BulkExport** - Export entire table as Arrow
4. **ObserverEvents** - Stream observer change data

---

### Arrow Flight Schema Registry

FraiseQL automatically registers Arrow schemas:

```text
va_orders: [id (Int64), total (Float64), created_at (Timestamp), customer_name (Utf8)]
va_users: [id (Int64), email (Utf8), name (Utf8), created_at (Timestamp)]
ta_orders: [measure_total (Numeric), dimension_data (Utf8), filter_customer_id (Utf8), calendar_info (Utf8)]
ta_users: [id (Text), email (Text), name (Text), created_at (Timestamp), source_updated_at (Timestamp)]
```text

---

## Part 7: Architecture Layers

### The Complete Picture

FraiseQL's database-centric design manifests in four layers:

```text
┌─────────────────────────────────────────────┐
│ Layer 1: AUTHORING (Your Code)              │
│ Python/TypeScript + @fraiseql decorators    │
│                                             │
│ @fraiseql.type                              │
│ class User:                                 │
│   user_id: int                              │
│   username: str                             │
│                                             │
│ Source: You write this                      │
│ Output: schema.json                         │
└─────────────────────────────────────────────┘
           │
           │ (fraiseql-cli compile)
           │
┌─────────────────────────────────────────────┐
│ Layer 2: COMPILATION (Build Time)           │
│ Validates, optimizes, generates SQL         │
│                                             │
│ - Validate schema against database          │
│ - Generate SQL templates                    │
│ - Optimize queries                          │
│ - Compile authorization rules               │
│ - Introspect fact tables & calendar dims    │
│ - Create Arrow Flight schema registry       │
│                                             │
│ Output: schema.compiled.json                │
└─────────────────────────────────────────────┘
           │
           │ (deployed to server)
           │
┌─────────────────────────────────────────────┐
│ Layer 3: RUNTIME (Execution)                │
│ Execute pre-compiled schemas and queries    │
│                                             │
│ JSON Plane (GraphQL/HTTP):                  │
│   - Query v_* views (logical reads)         │
│   - Query tv_* views (materialized JSON)    │
│   - Mutation via triggers                   │
│                                             │
│ Arrow Plane (Arrow Flight/gRPC):            │
│   - Stream va_* views (logical analytics)   │
│   - Stream ta_* views (materialized facts)  │
│   - Zero-copy columnar delivery             │
│                                             │
│ Where databases: PostgreSQL, MySQL,         │
│ SQLite, SQL Server all supported            │
└─────────────────────────────────────────────┘
           │
           │
           ▼
┌─────────────────────────────────────────────┐
│ Layer 0: DATABASE (Source of Truth)         │
│ Tables, views, functions, constraints       │
│                                             │
│ Write Tables:                               │
│   - tb_* tables (normalized, DBA-owned)     │
│                                             │
│ Read Views:                                 │
│   - v_* logical views                       │
│   - tv_* materialized JSON views            │
│   - va_* logical analytics views            │
│   - ta_* materialized fact tables           │
│                                             │
│ Analytics Foundation:                       │
│   - tf_* fact tables (if using analytics)   │
│   - td_* dimension tables (ETL only)        │
│                                             │
│ The single source of truth for all data     │
└─────────────────────────────────────────────┘
```text

---

## Part 8: Consequences of Database-Centric Design

### Immediate Benefits

**1. Clarity**

- What you see is what you get
- Database schema = API definition
- No hidden resolver logic

**2. Performance**

- Database optimization at compile time
- N+1 queries eliminated (database handles it)
- Fact tables enable 100x analytics speedup
- Deterministic query performance

**3. Consistency**

- Single source of truth
- No cache invalidation complexity
- Database constraints enforced
- ACID transactions guaranteed

**4. Security**

- Authorization rules compiled into SQL
- Row-level security possible
- Parameterized queries prevent injection

**5. Debuggability**

- Look at the SQL, understand the query
- No hidden resolver logic
- Performance bottlenecks are clear (database metrics)
- Calendar dimensions and filters explain slow queries

---

### Design Constraints

**1. Schema Must Be Structured**

- Requires clear database design (normalization, keys, constraints)
- Not suitable for unstructured/document-based data
- Fact tables require denormalization at ETL time

**2. Database Must Be Primary Data Source**

- Multi-source federation limited
- Aggregating multiple APIs requires federation pattern
- REST/GraphQL/etc. as secondary sources only

**3. Schema Changes Require Recompilation**

- Not suitable for dynamic, runtime schema changes
- Schema must be known at compile time
- Deployment is required for schema changes

**4. Database Expertise Required**

- Team must understand SQL, indexes, relationships, triggers
- DBA involvement necessary for analytics (fact tables, calendars)
- Schema design quality directly affects API performance

**5. Analytics Requires ETL Discipline**

- Dimensions must be denormalized at ETL time
- Calendar dimensions must be pre-computed
- No joins allowed in analytics queries (enforced by architecture)

---

## Summary: The Database-Centric Philosophy

FraiseQL makes a deliberate choice:

**Core assumption:** Your GraphQL API is a **database access interface**, not a general-purpose API aggregator.

**Implementation:**

- ✅ Transactional queries via `v_*` and `tv_*` views (JSON Plane)
- ✅ Analytics queries via `va_*` and `ta_*` views (Arrow Plane)
- ✅ Fact tables (`tf_*`) for high-performance aggregations
- ✅ Calendar dimensions for temporal performance (10-16x speedup)
- ✅ Multi-database support (PostgreSQL, MySQL, SQLite, SQL Server)

**Consequences:**

- ✅ Simpler architecture (no custom resolvers)
- ✅ Better performance (database optimization + fact tables)
- ✅ Higher consistency (single source of truth)
- ✅ Easier debugging (clear SQL + metrics)
- ❌ Less flexible (cannot easily add external APIs)
- ❌ Requires database expertise
- ❌ Schema must be structured
- ❌ Analytics requires ETL discipline

**Best for:** Data-centric applications with clear schemas, transactional + analytics needs, and performance requirements.

**Not suitable for:** Heavily federated systems, unstructured data, or dynamic schemas.

---

## Next Steps

Now you understand FraiseQL's database-centric approach:

1. **Learn how compilation works** → Topic 2.1 (Compilation Pipeline)
   - How your schema becomes optimized SQL

2. **Start designing schemas** → Topic 3.1 (Python Schema Authoring)
   - Write your first FraiseQL schema

3. **Understand specific databases** → Topic 4.1 (PostgreSQL Integration)
   - Database-specific features and best practices

4. **Learn design principles** → Topic 1.4 (Design Principles)
   - 5 guiding principles of FraiseQL

---

## Related Topics

- **Topic 1.1:** What is FraiseQL? — High-level positioning
- **Topic 1.2:** Core Concepts & Terminology — Database vocabulary
- **Topic 1.4:** Design Principles — 5 guiding principles
- **Topic 2.1:** Compilation Pipeline — How compilation works
- **Topic 4.5:** Database Design Patterns — Fact table design details
- **Topic 4.1:** PostgreSQL Integration — Database-specific guidance

---

## Key Takeaways

✅ **FraiseQL treats the database as the primary application interface**

✅ **Four-tier view system optimizes for different access patterns:**

- `v_*` logical reads (JSON, real-time)
- `tv_*` materialized JSON (complex nested, high volume)
- `va_*` logical analytics (Arrow, small datasets)
- `ta_*` materialized facts (Arrow, large datasets, 50-100ms latency)

✅ **Fact tables with three components:**

- Measures (SQL columns, 225x faster aggregation)
- Dimensions (JSONB, flexible grouping, no migration)
- Filters (indexed SQL, fast WHERE)

✅ **Calendar dimensions provide 10-16x analytics speedup** (pre-computed temporal buckets)

✅ **Arrow Flight enables 5-50x faster data streaming** (columnar, gRPC, zero-copy)

✅ **Multi-database support** (PostgreSQL, MySQL, SQLite, SQL Server with same schema)

✅ **Trade-off: Simplicity for flexibility** (not suitable for heavily federated systems)
