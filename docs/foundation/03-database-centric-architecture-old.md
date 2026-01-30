# 1.3: Database-Centric Architecture

**Audience:** Architects, database teams, developers building data systems
**Prerequisite:** Topics 1.1 (What is FraiseQL?), 1.2 (Core Concepts)
**Reading Time:** 15-20 minutes

---

## Overview

FraiseQL's fundamental design choice is to treat the **database as the primary application interface**, not as a storage afterthought. This topic explains why this choice matters, how it shapes FraiseQL's architecture, and what implications it has for your systems.

**Core insight:** In FraiseQL, the database schema is not an implementation detail—it's your API definition. The database is the source of truth for data relationships, types, validation, and performance.

---

## Part 1: The Core Philosophy

### GraphQL as a Database Access Layer, Not API Aggregation

Traditional GraphQL servers are designed to aggregate data from multiple sources:

```
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
```

**Problem:** The server becomes a coordination layer, and you need to write resolvers for every field, cache invalidation logic, N+1 prevention, etc.

---

### FraiseQL's Approach: Database-First Architecture

FraiseQL assumes the database is your **primary and usually only data source**:

```
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
```

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

```
Database Schema (DBA responsibility)
    ↓
FraiseQL Type Definition (Developer responsibility)
    ↓
GraphQL API (Client interface)
```

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
```

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
```

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
```

---

### Mapping: Tables → Types → Relationships

**FraiseQL automatically derives relationships from foreign keys:**

```sql
-- Database: Foreign key defines relationship
ALTER TABLE tb_orders
ADD CONSTRAINT fk_orders_user
FOREIGN KEY (fk_user) REFERENCES tb_users(pk_user);
```

**Becomes in FraiseQL:**

```python
@fraiseql.type
class Order:
    user_id: int
    user: User  # Automatically available because of FK
```

**No extra configuration needed.** The database structure is the API structure.

---

### View-Based Architecture

FraiseQL leverages database views extensively:

**Write Tables** (`tb_*`):
- Normalized schema (3NF or BCNF)
- DBA-owned and maintained
- Source of truth
- Used for mutations (INSERT, UPDATE, DELETE)

```sql
CREATE TABLE tb_users (
    pk_user BIGINT PRIMARY KEY,
    username VARCHAR(255),
    email VARCHAR(255),
    created_at TIMESTAMP,
    deleted_at TIMESTAMP
);
```

**Read Views** (`v_*` or `tv_*`):
- Curated for GraphQL access patterns
- Handle soft deletes (WHERE deleted_at IS NULL)
- Simplify complex queries
- Used for queries

```sql
CREATE VIEW v_user AS
SELECT
    pk_user,
    username,
    email,
    created_at,
    -- Hidden: deleted_at (soft delete handled)
FROM tb_users
WHERE deleted_at IS NULL;  -- Only active users
```

**Analytics Views** (`va_*`):
- Optimized for columnar access (Arrow plane)
- Denormalized for performance
- Aggregate data efficiently
- Used for large-scale data fetches

```sql
CREATE VIEW va_user_monthly_stats AS
SELECT
    EXTRACT(YEAR_MONTH FROM created_at) AS month,
    COUNT(*) AS user_count,
    COUNT(DISTINCT email) AS unique_emails,
    ARRAY_AGG(username) AS usernames
FROM tb_users
WHERE deleted_at IS NULL
GROUP BY month;
```

**Advantages of view-based approach:**
- Separation of concerns (DBA manages writes, developers use reads)
- Security (views can filter sensitive data)
- Performance (views optimize specific access patterns)
- Consistency (single source of truth, the table)

---

## Part 3: Multi-Database Support

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
# - PostgreSQL (primary)
# - MySQL
# - SQLite
# - SQL Server
```

---

### Database Selection Matrix

| Database | Strengths | Typical Use | Maturity |
|----------|-----------|------------|----------|
| **PostgreSQL** | Full-featured, JSONB, arrays, types | Production, primary | ✅ Full support |
| **MySQL** | Widely deployed, fast | Legacy systems, scale-out | ✅ Full support |
| **SQLite** | Lightweight, embedded, portable | Local dev, testing, mobile | ✅ Full support |
| **SQL Server** | Enterprise Windows, T-SQL | Enterprise deployments | ✅ Full support |

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
```

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
```

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
```

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
```

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
```

**MySQL (Limited Custom Types):**
```sql
-- MySQL: Standard types, JSON as string
CREATE TABLE tb_events (
    pk_event BIGINT AUTO_INCREMENT PRIMARY KEY,
    data JSON,  -- JSON as string
    tags JSON,  -- Array as JSON string
    status VARCHAR(50)  -- Enum as string
);
```

**SQLite (Minimal Types):**
```sql
-- SQLite: TEXT for everything complex
CREATE TABLE tb_events (
    pk_event INTEGER PRIMARY KEY,
    data TEXT,  -- JSON as text
    tags TEXT,  -- JSON array as text
    status TEXT  -- Enum as text
);
```

**FraiseQL handles the differences transparently.**

---

## Part 4: Data Planes and Access Patterns

### Two Data Planes for Different Needs

FraiseQL supports two fundamental data access patterns:

---

### JSON Plane: Transactional Access

**What it is:** Standard GraphQL over HTTP/JSON

**When to use:** API clients, web applications, transactional queries

**Characteristics:**
- Row-by-row data delivery
- Supports real-time relationships (User → Orders)
- Built-in pagination
- Good for OLTP (Online Transaction Processing)
- Latency: Milliseconds

**Example:**
```graphql
query GetUserWithOrders {
  user(id: 1) {
    userId
    username
    orders(limit: 10) {
      orderId
      total
      createdAt
    }
  }
}
```

**JSON Plane uses:**
- `v_*` views (read views)
- `tv_*` views (transaction views for mutations)
- Standard HTTP/JSON protocol
- SQL queries with JOINs and WHERE clauses

---

### Arrow Plane: Analytical Access

**What it is:** Apache Arrow Flight protocol for columnar data

**When to use:** Data pipelines, analytics, bulk exports, machine learning

**Characteristics:**
- Columnar data delivery (100x faster for analytics)
- Streaming results (no client-side buffering)
- Built-in compression
- Good for OLAP (Online Analytical Processing)
- Throughput: Hundreds of MB/sec

**Example:**
```graphql
query ExportUserAnalytics {
  users {
    userId
    username
    createdAt
    accountAge: daysActive
    totalOrders: orderCount
  }
}
# Returns 1M rows in seconds, not minutes
```

**Arrow Plane uses:**
- `va_*` views (analytics views, denormalized)
- `ta_*` views (fact tables for aggregations)
- Arrow Flight protocol (gRPC + Arrow format)
- Optimized column-store queries

---

### Choosing Your Data Plane

```
┌─────────────────────────────────────────┐
│ What's your use case?                   │
└─────────────────────────────────────────┘
           │
    ┌──────┴──────┐
    │             │
    ▼             ▼
OLTP?          OLAP?
(Real-time)    (Bulk)
  │              │
  │              ▼
  │         Need 1M rows?
  │              │
  │         ┌────┴────┐
  │         │          │
  ▼         ▼          ▼
JSON     Arrow (fast)  JSON
Plane    Plane        Plane
(small   (~100x)      (if <10K
<10K                   rows)
rows)

Result:
Use JSON Plane    ← JSON Plane: REST API, web apps
Use Arrow Plane   ← Arrow Plane: analytics, pipelines
```

---

## Part 5: Architecture Layers

### The Complete Picture

FraiseQL's database-centric design manifests in three layers:

```
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
│ - Create capability manifest                │
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
│ GraphQL Query Received                      │
│   ↓                                          │
│ Validate (schema)                           │
│   ↓                                          │
│ Authorize (rules)                           │
│   ↓                                          │
│ Execute (compiled SQL)                      │
│   ↓                                          │
│ Database                                     │
│   ↓                                          │
│ Format Results                              │
│   ↓                                          │
│ Return to Client                            │
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
│ - tb_* tables (normalized, DBA-owned)       │
│ - v_* views (read views)                    │
│ - tv_* views (transaction views)            │
│ - va_* views (analytics views)              │
│ - fn_* functions (business logic)           │
│                                             │
│ The single source of truth for all data     │
└─────────────────────────────────────────────┘
```

---

## Part 6: Consequences of Database-Centric Design

### Immediate Benefits

**1. Clarity**
- What you see is what you get
- Database schema = API definition
- No hidden resolver logic

**2. Performance**
- Database optimization at compile time
- N+1 queries eliminated (database handles it)
- Deterministic query performance

**3. Consistency**
- Single source of truth
- No cache invalidation complexity
- Database constraints enforced

**4. Security**
- Authorization rules compiled into SQL
- Row-level security possible
- Parameterized queries prevent injection

---

### Design Constraints

**1. Schema Must Be Structured**
- Requires clear database design (normalization, keys, constraints)
- Not suitable for unstructured/document-based data
- Must map GraphQL concepts to database concepts

**2. Database Must Be Primary Data Source**
- Multi-source federation limited
- Aggregating multiple APIs requires federation pattern
- REST/GraphQL/etc. as secondary sources only

**3. Schema Changes Require Recompilation**
- Not suitable for dynamic, runtime schema changes
- Schema must be known at compile time
- Deployment is required for schema changes

**4. Database Expertise Required**
- Team must understand SQL, indexes, relationships
- DBA involvement necessary
- Schema design quality affects API performance

---

## Summary: The Database-Centric Philosophy

FraiseQL makes a deliberate choice:

**Core assumption:** Your GraphQL API is a **database access interface**, not a general-purpose API aggregator.

**Consequences:**
- ✅ Simpler architecture (no custom resolvers)
- ✅ Better performance (database optimization)
- ✅ Higher consistency (single source of truth)
- ✅ Easier debugging (clear data flow)
- ❌ Less flexible (cannot easily add external APIs)
- ❌ Requires database expertise
- ❌ Schema must be structured

**Best for:** Data-centric applications with clear schemas and performance requirements.

**Not suitable for:** Heavily federated systems, unstructured data, or dynamic schemas.

---

## Next Steps

Now you understand why FraiseQL is database-centric:

1. **Learn how compilation works** → Topic 2.1 (Compilation Pipeline)
   - How your schema becomes SQL

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
- **Topic 4.1:** PostgreSQL Integration — Database-specific guidance
- **Topic 4.2:** MySQL Integration — MySQL-specific guidance

---

## Key Takeaways

✅ **FraiseQL treats the database as the primary application interface**

✅ **Database schema is your API definition** (no separate resolver code)

✅ **Single source of truth eliminates inconsistency and caching complexity**

✅ **Multi-database support** (PostgreSQL, MySQL, SQLite, SQL Server)

✅ **Two data planes** (JSON for transactional, Arrow for analytical)

✅ **Clear architecture layers** (authoring → compilation → runtime)

✅ **Trade-off: Simplicity for flexibility** (not suitable for highly federated systems)
