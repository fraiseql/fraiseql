<!-- Skip to main content -->
---
title: Arrow Plane: Columnar Data Acceleration
description: 1. [Introduction & Philosophy](#1-introduction--philosophy)
keywords: ["design", "scalability", "performance", "patterns", "security"]
tags: ["documentation", "reference"]
---

# Arrow Plane: Columnar Data Acceleration

**Version:** 2.0
**Date:** February 5, 2026
**Status:** ✅ Implemented in v2.0.0-alpha.1 (Feature-gated in cargo features)
**Audience:** Analytics Engineers, Data Platform Architects, Performance-Sensitive Developers

## Table of Contents

1. [Introduction & Philosophy](#1-introduction--philosophy)
2. [Arrow Plane Architecture](#2-arrow-plane-architecture)
3. [Authoring Arrow Projections](#3-authoring-arrow-projections)
4. [Querying Arrow Projections](#4-querying-arrow-projections)
5. [Database Implementation](#5-database-implementation)
6. [Performance Characteristics](#6-performance-characteristics)
7. [Security & Data Governance](#7-security--data-governance)
8. [Multi-Database Support](#8-multi-database-support)
9. [Limitations & Trade-offs](#10-limitations--trade-offs)
10. [Examples](#11-examples)
11. [Decision Guide: When to Use Each Plane](#13-decision-guide-when-to-use-each-plane)
12. [FAQ](#14-faq)
13. [Appendix: Arrow Type Reference](#15-appendix-arrow-type-reference)

---

## 1. Introduction & Philosophy

### Core Philosophy

> **FraiseQL is a multi-plane system: JSON for interaction, Arrow for computation.**

The JSON plane handles application queries (nested objects, flexible schema). The Arrow plane handles analytical workloads (columnar, typed, bulk operations). One schema. Two optimal execution strategies.

### What is the Arrow Plane?

The **Arrow plane** is an optional, high-performance data projection layer that exposes FraiseQL data in **Apache Arrow format** — a language-agnostic, columnar, strongly-typed in-memory representation optimized for analytics, BI tools, and data science workloads.

While FraiseQL's **JSON plane** is optimized for application clients (nested objects, flexible schema), the Arrow plane is optimized for:

- **Analytics workflows** (Pandas, Polars, DuckDB, Spark)
- **BI tools** (Tableau, Looker, PowerBI native connectors)
- **ML pipelines** (columnar feature extraction, batch inference)
- **Data export** (Parquet, CSV, cloud data warehouses)
- **Streaming analytics** (10+ million rows/second server-side throughput in columnar serialization)

### Design Principle: Relational, Not Nested

Where the JSON plane represents nested GraphQL selections as single responses with composed JSONB objects, the Arrow plane represents them as **multiple flat, keyed batches**:

**JSON Plane (Nested):**

```json
<!-- Code example in JSON -->
{
  "user": {
    "id": "123",
    "name": "Alice",
    "posts": [
      { "id": "a1", "title": "First" },
      { "id": "a2", "title": "Second" }
    ]
  }
}
```text
<!-- Code example in TEXT -->

**Arrow Plane (Flat, Relational):**

```text
<!-- Code example in TEXT -->
users_batch:
  | id  | name  |
  | 123 | Alice |

posts_batch:
  | id | user_id | title  |
  | a1 | 123     | First  |
  | a2 | 123     | Second |
```text
<!-- Code example in TEXT -->

The Arrow plane outputs **multiple batches** with explicit foreign key references. Clients join them using columnar tools (Pandas merge, SQL, Spark join).

### Architecture Principle: Compile-Time Schema Specialization

Arrow plane definitions are **compiled, not interpreted**:

1. **Authoring time:** Developer declares which fields to include in Arrow projections
2. **Compile time:** FraiseQL generates Arrow schema (column names, types, nullability)
3. **SQL generation:** Arrow queries compile to SQL with batch-aware aggregation
4. **Runtime:** Rust runtime produces Arrow IPC streaming format or Apache Arrow files

**No dynamic Arrow schema generation.** Schema is deterministic, typed, and knowable at compile time.

### Arrow Projections as Analytical Contracts

Arrow projections are not merely data formats—they are **stable, versioned analytical contracts** between FraiseQL and downstream systems (BI tools, data warehouses, ML pipelines, dashboards).

**Contract Guarantees:**

- ✅ **Schema stability** — Column names, types, and nullability are versioned and change-tracked
- ✅ **Backward compatibility** — Existing consumers (Tableau, Looker, Parquet pipelines) continue working across versions
- ✅ **Deprecation clarity** — Removed projections have documented migration paths
- ✅ **Auditability** — All projection changes recorded in version history

**Why this matters:**
BI tools, ETL pipelines, and data warehouses embed these contracts. Breaking changes cause cascading failures downstream. Contract formality prevents that.

---

## 1.5 Analytical Contract Specification

### Schema Versioning Strategy

Arrow projections follow **semantic versioning** for their schemas:

```yaml
<!-- Code example in YAML -->
# Projection declaration
@FraiseQL.arrow_projection(
  name="orders_analytics",
  version="2.3.1",              # MAJOR.MINOR.PATCH
  stability="stable"            # stable, beta, deprecated
)
```text
<!-- Code example in TEXT -->

**Version Semantics:**

| Change Type | Version | Example | Consumer Action |
|---|---|---|---|
| **New optional field** | MINOR (2.2→2.3) | Add `updated_at` column | Auto-compatible |
| **New required field** | MAJOR (2→3) | Add `compliance_flag` | May break consumers |
| **Remove field** | MAJOR (2→3) | Remove `legacy_id` | Deprecation period first |
| **Rename column** | MAJOR (2→3) | `created_at`→`created_timestamp` | Deprecation period first |
| **Change type** | MAJOR (2→3) | `Int32`→`Int64` | Breaking change |
| **Bug fix to values** | PATCH (2.3→2.3.1) | Fix timezone conversion | No schema change |

**Deprecation Period:**

- Removed/renamed fields require **2 minor releases (60 days)** notice
- During deprecation: Both old and new field names available
- Consumers must migrate; support ends after period

### Backward Compatibility Rules

✅ **Always backward-compatible** (MINOR version):

- Add new optional columns at end of batch
- Extend enum with new values
- Expand Decimal precision
- Extend nullable fields from non-null to nullable

❌ **Breaking changes** (MAJOR version):

- Remove or rename columns
- Change column type
- Make nullable field non-null
- Reorder columns (some tools expect positional stability)
- Change foreign key targets

### Projection Deprecation Policy

When a projection becomes obsolete:

1. **Announce** (version N): Mark as `stability="deprecated"` with replacement guidance

   ```python
<!-- Code example in Python -->
   @FraiseQL.arrow_projection(
     name="orders_v1",
     stability="deprecated",
     replacement="orders_analytics_v2",
     sunset_date="2026-07-11"  # 6 months ahead
   )
   ```text
<!-- Code example in TEXT -->

2. **Support** (N+2 minor versions / 60 days): Both old and new available
   - Consumers see warnings in logs
   - Migration guides published
   - Support team notified

3. **Sunset** (N+2 releases): Old projection removed
   - All consumers must have migrated
   - Replacement projection fully stable

### Multi-Batch Versioning Rule

**Critical rule for multi-batch projections:**

> A projection version applies to the entire batch set. Any breaking change in **any batch** increments the projection MAJOR version.

**Example: Why this matters**

```python
<!-- Code example in Python -->
# v1.0: orders + order_items (2 batches)
@FraiseQL.arrow_projection(name="order_analytics", version="1.0")
class OrderAnalytics:
    orders: Arrow.Batch([...])         # 4 columns: id, amount, created_at, status
    order_items: Arrow.Batch([...])    # 5 columns: id, order_id, product_id, qty, price
```text
<!-- Code example in TEXT -->

**Scenario 1: Add column to order_items only**

```python
<!-- Code example in Python -->
# v1.1: order_items now has 6 columns (new "discount" column)
orders: Arrow.Batch([...])         # ← Still 4 columns
order_items: Arrow.Batch([...])    # ← Now 6 columns (NEW)
```text
<!-- Code example in TEXT -->

**Decision:** Increment to v1.1 (backward-compatible MINOR change)

- Reason: Adding optional column is safe
- Downstream systems: See new column but can ignore

**Scenario 2: Remove column from order_items**

```python
<!-- Code example in Python -->
# v2.0: order_items no longer has "price" column
orders: Arrow.Batch([...])         # ← Still 4 columns
order_items: Arrow.Batch([...])    # ← Now 4 columns (removed "price")
```text
<!-- Code example in TEXT -->

**Decision:** Increment to v2.0 (breaking MAJOR change)

- Reason: Removing column breaks downstream systems expecting it
- Downstream systems: MUST migrate before v2.0 release
- Why entire projection versioned: BI tools treat whole projection as atomic dataset

**Why projection-level versioning?**

- BI tools (Tableau, Looker) import projections as datasets
- They see "order_analytics" as one entity, not as independent batches
- A breaking change in any batch affects the entire dataset
- Clients need single version number to track compatibility

---

---

## 2. Arrow Plane Architecture

### 2.1 Multi-Batch Composition

Arrow projections are **inherently relational**. A GraphQL selection like:

```graphql
<!-- Code example in GraphQL -->
query {
  user(id: "123") {
    id
    name
    email
    posts {
      id
      title
      createdAt
    }
  }
}
```text
<!-- Code example in TEXT -->

Compiles to **two Arrow batches**:

**Batch 1: `users`**

```text
<!-- Code example in TEXT -->
┌────┬───────┬──────────────────┐
│ id │ name  │ email            │
├────┼───────┼──────────────────┤
│123 │ Alice │ alice@example.com│
└────┴───────┴──────────────────┘
```text
<!-- Code example in TEXT -->

**Batch 2: `user_posts`** (with foreign key reference)

```text
<!-- Code example in TEXT -->
┌────┬──────┬─────────────────────────────┐
│ id │ user_id │ title        │ createdAt │
├────┼─────────┼──────────────┼───────────┤
│ a1 │ 123     │ First Post   │ 2025-01-01│
│ a2 │ 123     │ Second Post  │ 2025-01-02│
└────┴─────────┴──────────────┴───────────┘
```text
<!-- Code example in TEXT -->

**Client reconstruction** (Pandas):

```python
<!-- Code example in Python -->
import pandas as pd

users_df = arrow_batches['users']        # DataFrame
posts_df = arrow_batches['user_posts']   # DataFrame

# Join using explicit foreign key
result = pd.merge(
    users_df,
    posts_df,
    left_on='id',
    right_on='user_id'
)
```text
<!-- Code example in TEXT -->

### 2.1.5 Foreign Key Semantics

**Critical clarification for BI tools and downstream systems:**

Foreign keys in Arrow projections are **logical, not enforced constraints**. They guarantee semantic joinability but do **not** imply database-level referential integrity enforcement.

**What they guarantee:**
✅ Foreign key columns are present and typed
✅ Values can be used for deterministic joins
✅ Semantic relationship is documented (parent-child)

**What they do NOT guarantee:**
❌ Database-level referential integrity (no FK constraint check)
❌ Every child has a parent (orphaned children are valid)
❌ Every parent has children (empty parent batches are valid)
❌ No N:M relationships (multiple parents per child may exist)

**Implications for analytical work:**

**Scenario 1: Orphaned rows (valid)**

```text
<!-- Code example in TEXT -->
users: [id=1, id=2, id=3]
posts: [user_id=1, user_id=1, user_id=99]  ← user_id=99 doesn't exist

posts.user_id=99 is a valid analytical state.
No error. No constraint violation.
BI tool must treat as missing parent.
```text
<!-- Code example in TEXT -->

**Scenario 2: Empty child batch (valid)**

```text
<!-- Code example in TEXT -->
users: [id=1, id=2, id=3]
posts: []  ← No posts from any user

Empty batch is valid analytical state.
Null rows in join are expected.
```text
<!-- Code example in TEXT -->

**Scenario 3: Multiple parents per child (possible in wide projections)**

```text
<!-- Code example in TEXT -->
# If projection includes: orders + items + warehouses
items.warehouse_id could reference multiple batches
```text
<!-- Code example in TEXT -->

**Best practice:**
Treat Arrow FK joins like SQL LEFT OUTER JOIN, not INNER JOIN:

```python
<!-- Code example in Python -->
# ✅ Correct: Preserve unmatched rows
result = pd.merge(users_df, posts_df, on='user_id', how='left')

# ❌ Risky: Drops orphaned children
result = pd.merge(users_df, posts_df, on='user_id', how='inner')
```text
<!-- Code example in TEXT -->

This prevents silent data loss when processing.

### 2.2 Projection Depth: The Shallow Principle

Arrow projections should remain **shallow** (1–2 relationship hops maximum).

**Why:**

- **Batch explosion** — 5 levels deep can produce 50+ batches for a single query
- **Client complexity** — Multiple sequential joins become difficult to reason about
- **Performance degradation** — More batches = more network, more memory, slower aggregation
- **Analytical clarity** — Wide, flat projections are easier to optimize than deep graphs

**Design Rule:**

```text
<!-- Code example in TEXT -->
Arrow Depth Limit: 2 relationship hops maximum
├─ Level 0: Entity (User, Order, Product)
├─ Level 1: Direct relationships (User → Orders, Orders → Items)
└─ Level 2: Secondary relationships (User → Orders → Invoices)
    └─ STOP: Don't go deeper

For deeper requirements: Define multiple specialized projections
```text
<!-- Code example in TEXT -->

**Example: What NOT to do**

```graphql
<!-- Code example in GraphQL -->
# ❌ BAD: 4 levels deep
query {
  user {
    posts {           # Level 1
      comments {      # Level 2
        author {      # Level 3
          profile {   # Level 4 ← STOP
            avatar
          }
        }
      }
    }
  }
}
```text
<!-- Code example in TEXT -->

**Solution: Create focused projections**

```python
<!-- Code example in Python -->
# ✅ GOOD: 3 separate projections for different analytical needs

# Projection 1: User engagement (shallow)
@FraiseQL.arrow_projection(name="user_engagement")
class UserEngagement:
    users: Arrow.Batch([...])
    posts: Arrow.Batch([...])
    # 1 hop: User → Posts

# Projection 2: Post engagement (isolated)
@FraiseQL.arrow_projection(name="post_engagement")
class PostEngagement:
    posts: Arrow.Batch([...])
    comments: Arrow.Batch([...])
    # 1 hop: Post → Comments

# Projection 3: Comment threads (isolated)
@FraiseQL.arrow_projection(name="comment_threads")
class CommentThreads:
    comments: Arrow.Batch([...])
    authors: Arrow.Batch([...])  # Author profile info
    # 1 hop: Comment → Author
```text
<!-- Code example in TEXT -->

**BI Tool Compatibility:**
Most BI tools (Tableau, Looker, PowerBI) work best with **3-5 related tables maximum**. Shallow projections align naturally with BI architecture.

### 2.3 Arrow Schema Definition

Arrow schemas are **determined at compile time**. Each batch has explicit types:

**Type Mapping:**

| FraiseQL Type | Arrow Type | Notes |
|---------------|-----------|-------|
| `ID` | `String` | 36-char UUID |
| `String` | `String` | UTF-8 encoded |
| `Int` | `Int32` or `Int64` | Depends on field range |
| `Float` | `Float64` (IEEE-754) | 64-bit double |
| `Boolean` | `Bool` | Single bit |
| `DateTime` | `Timestamp(us, UTC)` | Microsecond precision, UTC |
| `Date` | `Date32` | Days since epoch |
| `Decimal` | `Decimal128` | 128-bit precision decimal |
| `JSON` | `String` | JSON-serialized string |
| `list[T]` | Not supported in single batch | Must be separate batch with FK |

**Example: Compiled Arrow Schema**

```yaml
<!-- Code example in YAML -->
# Compile-time generated for OrderWithItems query
batches:
  - name: orders
    columns:
      - name: id
        type: String          # FraiseQL ID → Arrow String
        nullable: false
      - name: customer_id
        type: String
        nullable: false
      - name: total
        type: Decimal128
        nullable: false
      - name: created_at
        type: Timestamp(us, UTC)
        nullable: false

  - name: order_items
    columns:
      - name: id
        type: String
        nullable: false
      - name: order_id        # Foreign key back to orders.id
        type: String
        nullable: false
      - name: product_id
        type: String
        nullable: false
      - name: quantity
        type: Int32
        nullable: false
      - name: unit_price
        type: Decimal128
        nullable: false
```text
<!-- Code example in TEXT -->

### 2.4 Streaming vs. File Format

Arrow supports multiple serialization formats:

| Format | Use Case | Throughput | Latency | Example |
|--------|----------|-----------|---------|---------|
| **IPC Streaming** | Real-time dashboards, feeds | 10M+ rows/sec | <100ms | WebSocket to Grafana |
| **Arrow Files** | Batch export, Parquet | 50M+ rows/sec | Seconds | S3 download |
| **Parquet** | Data warehouse archival | 100M+ rows/sec | Minutes | Athena, BigQuery |
| **CSV** | Legacy BI tools, spreadsheets | 1-5M rows/sec | Seconds | Excel, Tableau desktop |

The FraiseQL runtime chooses the format based on the **HTTP `Accept` header**:

```text
<!-- Code example in TEXT -->
Accept: application/x-arrow               → IPC Streaming (real-time)
Accept: application/vnd.apache.arrow.file → Arrow File format
Accept: application/parquet               → Parquet (if enabled)
Accept: text/csv                          → CSV (fallback)
Accept: application/json                  → JSON (fallback)
```text
<!-- Code example in TEXT -->

### 2.5 Arrow vs. JSON Plane Trade-offs

| Characteristic | JSON Plane | Arrow Plane |
|---|---|---|
| **Format** | JSON (nested, flexible) | Arrow (columnar, typed) |
| **Data model** | Object graphs | Relational tables |
| **Relationship handling** | Nested objects | Separate batches + FK |
| **Schema rigidity** | Dynamic | Compile-time static |
| **Throughput** | 1-5M rows/sec | 10-100M+ rows/sec |
| **Latency** | 50-500ms | <100ms (streaming) |
| **Memory overhead** | 20-40% (JSON parsing) | <5% (binary columnar) |
| **Ideal use case** | Application clients | Analytics, BI, ML |
| **Tool compatibility** | JavaScript, Python, REST | Pandas, Spark, Arrow ecosystem |
| **Client implementation** | Parse JSON, navigate graph | Load Arrow batches, join via FK |

### 2.6 Key Differences from JSON Plane

**JSON Plane (Composition):**

1. Single nested response
2. Database composes JSONB
3. Client receives complete object graph
4. Implicit relationships (nested objects)

**Arrow Plane (Relational):**

1. Multiple flat batches
2. Database materializes columns
3. Client joins batches via foreign keys
4. Explicit relationships (FK references)

---

## 3. Authoring Arrow Projections

### 3.1 Python Authoring

Arrow projections are declared in schema authoring:

```python
<!-- Code example in Python -->
import FraiseQL
from FraiseQL import Arrow, ArrowField

@FraiseQL.type
class Order:
    id: FraiseQL.ID
    customer_id: FraiseQL.ID
    total: FraiseQL.Decimal
    created_at: FraiseQL.DateTime
    items: list['OrderItem']

@FraiseQL.type
class OrderItem:
    id: FraiseQL.ID
    product_id: FraiseQL.ID
    quantity: FraiseQL.Int
    unit_price: FraiseQL.Decimal

# Define Arrow projection: single flat view of order with items
@FraiseQL.arrow_projection(
    name="order_with_items",
    description="Orders with line items for analytics"
)
class OrderWithItemsArrow:
    # Primary batch: orders
    orders: Arrow.Batch(
        fields=[
            ArrowField("id", "String", nullable=False),
            ArrowField("customer_id", "String", nullable=False),
            ArrowField("total", "Decimal128", nullable=False),
            ArrowField("created_at", "Timestamp(us, UTC)", nullable=False),
        ]
    )

    # Related batch: order items
    order_items: Arrow.Batch(
        fields=[
            ArrowField("id", "String", nullable=False),
            ArrowField("order_id", "String", nullable=False, foreign_key="orders.id"),
            ArrowField("product_id", "String", nullable=False),
            ArrowField("quantity", "Int32", nullable=False),
            ArrowField("unit_price", "Decimal128", nullable=False),
        ]
    )
```text
<!-- Code example in TEXT -->

### 3.2 TypeScript Authoring

```typescript
<!-- Code example in TypeScript -->
import { Arrow, ArrowField, ArrowBatch } from '@FraiseQL/core';

@FraiseQL.arrowProjection({
  name: 'order_with_items',
  description: 'Orders with line items for analytics'
})
class OrderWithItemsArrow {
  orders: Arrow.Batch = {
    fields: [
      new ArrowField('id', 'String', { nullable: false }),
      new ArrowField('customer_id', 'String', { nullable: false }),
      new ArrowField('total', 'Decimal128', { nullable: false }),
      new ArrowField('created_at', 'Timestamp(us, UTC)', { nullable: false }),
    ]
  };

  order_items: Arrow.Batch = {
    fields: [
      new ArrowField('id', 'String', { nullable: false }),
      new ArrowField('order_id', 'String', {
        nullable: false,
        foreignKey: 'orders.id'
      }),
      new ArrowField('product_id', 'String', { nullable: false }),
      new ArrowField('quantity', 'Int32', { nullable: false }),
      new ArrowField('unit_price', 'Decimal128', { nullable: false }),
    ]
  };
}
```text
<!-- Code example in TEXT -->

### 3.3 Compile-Time Validation

FraiseQL validates Arrow projections at compile time:

✅ **All fields must exist in projected types** — No unresolved paths
✅ **Types must be representable in Arrow** — No nested JSONB, only scalars
✅ **Foreign keys must reference valid columns** — Explicit relational integrity
✅ **Column names must be unique within batch** — Deterministic naming
✅ **Decimal precision must be explicit** — Decimal128 or Decimal256
✅ **Timestamps must have timezone** — Prevent ambiguous times

**Example Validation Error:**

```text
<!-- Code example in TEXT -->
❌ ArrowProjectionError in order_with_items:
   Field 'metadata' (JSON type) cannot be represented in Arrow.
   Suggestion: Use JSON.stringify() in database view and project as String
```text
<!-- Code example in TEXT -->

---

## 4. Querying Arrow Projections

### 4.1 HTTP Request

Arrow projections are exposed via standard FraiseQL query endpoint with `Accept` header:

**Request:**

```bash
<!-- Code example in BASH -->
curl -X POST https://api.example.com/graphql \
  -H "Content-Type: application/json" \
  -H "Accept: application/x-arrow" \
  -d '{
    "query": "query { orderWithItems { ... } }"
  }'
```text
<!-- Code example in TEXT -->

**Response Format:**

- `Content-Type: application/x-arrow`
- Body: Apache Arrow IPC streaming format (binary)

### 4.2 Arrow IPC Streaming Format

Arrow IPC (Inter-Process Communication) is a standardized binary format that transmits Arrow batches:

```text
<!-- Code example in TEXT -->
[Arrow Magic Number] [Metadata Message] [RecordBatch] [RecordBatch] ... [EOF]
```text
<!-- Code example in TEXT -->

**Example Decoded (for reference):**

```text
<!-- Code example in TEXT -->
Batch 1 (orders):
  ┌─────────────────────────┐
  │ Metadata                │
  │  - fields: 4            │
  │  - rows: 1              │
  └─────────────────────────┘
  ┌─────────────────────────┐
  │ Buffers (columnar)      │
  │ - id: [0x...123...]     │
  │ - customer_id: [0x...]  │
  │ - total: [0x...]        │
  │ - created_at: [0x...]   │
  └─────────────────────────┘

Batch 2 (order_items):
  ┌─────────────────────────┐
  │ Metadata                │
  │  - fields: 5            │
  │  - rows: 3              │
  └─────────────────────────┘
  ┌─────────────────────────┐
  │ Buffers (columnar)      │
  │ - id: [0x...a1...a2...] │
  │ - order_id: [0x...123...│
  │ - product_id: [0x...]   │
  │ - quantity: [0x...]     │
  │ - unit_price: [0x...]   │
  └─────────────────────────┘
```text
<!-- Code example in TEXT -->

### 4.3 Client-Side Deserialization

**Python (Pandas):**

```python
<!-- Code example in Python -->
import pyarrow as pa
import pandas as pd
import requests

response = requests.post(
    'https://api.example.com/graphql',
    json={'query': '...'},
    headers={'Accept': 'application/x-arrow'}
)

# Deserialize Arrow batches
reader = pa.ipc.open_stream(response.content)
batches = [reader.get_batch(i) for i in range(reader.num_record_batches)]

# Convert to DataFrames
orders_df = batches[0].to_pandas()
items_df = batches[1].to_pandas()

# Join using foreign key
result = pd.merge(orders_df, items_df, left_on='id', right_on='order_id')
```text
<!-- Code example in TEXT -->

**JavaScript (Node.js):**

```javascript
<!-- Code example in JAVASCRIPT -->
import * as arrow from 'apache-arrow';

const response = await fetch('https://api.example.com/graphql', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    'Accept': 'application/x-arrow'
  },
  body: JSON.stringify({ query: '...' })
});

// Deserialize Arrow batches
const buf = await response.arrayBuffer();
const reader = arrow.RecordBatchStreamReader.from(new Uint8Array(buf));

const ordersTable = reader.readNext().value;  // First batch
const itemsTable = reader.readNext().value;   // Second batch

// Join using arrow-js utilities (or convert to DataFrame)
```text
<!-- Code example in TEXT -->

---

## 5. Database Implementation

### 5.1 Arrow Views

Arrow projections compile to **Arrow views** in the database:

```sql
<!-- Code example in SQL -->
-- Generated by compiler
CREATE VIEW v_order_with_items_orders AS
SELECT
  pk_order,
  id,
  customer_id,
  total,
  created_at
FROM tb_order
WHERE deleted_at IS NULL
ORDER BY created_at DESC;

CREATE VIEW v_order_with_items_items AS
SELECT
  pk_order_item,
  id,
  fk_order AS order_id,
  fk_product AS product_id,
  quantity,
  unit_price
FROM tb_order_item
WHERE deleted_at IS NULL
ORDER BY pk_order_item;
```text
<!-- Code example in TEXT -->

### 5.2 Batch Aggregation Queries

For aggregate queries (e.g., "order count by status"), Arrow batches use SQL aggregation:

```sql
<!-- Code example in SQL -->
-- Aggregate query compiling to Arrow
SELECT
  status,
  COUNT(*) as order_count,
  SUM(total) as total_revenue,
  AVG(total) as avg_order_value
FROM tb_order
WHERE created_at > NOW() - INTERVAL '30 days'
  AND deleted_at IS NULL
GROUP BY status;

-- Result: Single Arrow batch with 4 columns
```text
<!-- Code example in TEXT -->

### 5.3 Pagination Support: Keyset from Start

Large Arrow projections support **keyset-based pagination** (cursor-backed by monotonic ordering). This avoids the performance cliff of OFFSET at high cardinality.

**Why keyset pagination?**

OFFSET/LIMIT works fine at small offsets (<1M rows):

```sql
<!-- Code example in SQL -->
SELECT ... ORDER BY id LIMIT 1000 OFFSET 10000000;
```text
<!-- Code example in TEXT -->

But at 10M+ rows, OFFSET scans and skips all prior rows. **Keyset pagination** only reads from the last cursor:

```sql
<!-- Code example in SQL -->
-- ❌ Slow: Scan 10M rows, skip 10M rows, return 1000
SELECT ... LIMIT 1000 OFFSET 10000000;

-- ✅ Fast: Read directly from cursor position
SELECT ... WHERE id > '2025-01-11T15:30:00Z' LIMIT 1000;
```text
<!-- Code example in TEXT -->

**Keyset Pagination Design:**

The cursor encodes an **ordered key value** that identifies the last row returned:

```python
<!-- Code example in Python -->
# Phase 1 (v2.1): Simple keyset based on primary key
cursor = base64(encode(last_id))

# Phase 2+ (v2.2): Composite keyset for stable ordering
cursor = base64(encode({
    'id': last_id,
    'created_at': last_created_at,
    'sequence': last_sequence
}))
```text
<!-- Code example in TEXT -->

**Request Pattern (Relay-Compatible):**

```graphql
<!-- Code example in GraphQL -->
query {
  orderAnalytics(first: 1000, after: "eyJpZCI6IjEyMyJ9") {
    edges {
      node { id amount }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```text
<!-- Code example in TEXT -->

**Compiled SQL:**

```sql
<!-- Code example in SQL -->
-- Phase 1: Simple keyset on primary key
SELECT ... WHERE id > ? ORDER BY id LIMIT 1000;

-- Phase 2: Composite keyset for stable ordering
SELECT ...
WHERE (created_at, id) > (?, ?)
ORDER BY created_at, id
LIMIT 1000;
```text
<!-- Code example in TEXT -->

**Response Format:**

```json
<!-- Code example in JSON -->
{
  "edges": [
    {
      "node": { "id": "123", "amount": 99.99 },
      "cursor": "eyJpZCI6IjEyMyIsImNyZWF0ZWRfYXQiOiIyMDI1LTAxLTExVDE1OjMwOjAwWiJ9"
    }
  ],
  "pageInfo": {
    "hasNextPage": true,
    "endCursor": "eyJpZCI6IjEyNCIsImNyZWF0ZWRfYXQiOiIyMDI1LTAxLTExVDE1OjMxOjAwWiJ9"
  }
}
```text
<!-- Code example in TEXT -->

**Implementation Strategy:**

| Phase | Approach | Suitable For | Latency @ 10M rows |
|---|---|---|---|
| **v2.1** | Keyset on primary key | Initial release, most use cases | <50ms |
| **v2.2** | Composite keyset (created_at, id) | Stable ordering across mutations | <50ms |
| **v2.3** | Indexed keyset hints + query optimization | Very large datasets (100M+) | <20ms |

**Backward Compatibility with JSON Plane:**

Both Arrow and JSON planes use **identical keyset pagination**:

- Same cursor format
- Same `Relay` pagination interface
- Clients switch between planes without pagination logic changes

**Note on Offset-Limit:**

⚠️ **OFFSET/LIMIT will be available but discouraged** for analytical queries:

- ✅ Encouraged: Keyset-based pagination (cursor-backed)
- ⚠️ Discouraged: OFFSET/LIMIT (only for small offsets <100K rows)
- ❌ Anti-pattern: Large OFFSET on 100M+ row tables

FraiseQL will emit warnings in logs if clients attempt large OFFSETs:

```text
<!-- Code example in TEXT -->
WARNING: Large OFFSET (10000000) detected.
Consider using keyset pagination for better performance.
See: docs/pagination.md
```text
<!-- Code example in TEXT -->

---

## 5.5 Integration with Analytical Views

The Arrow plane is particularly well-suited for analytical workloads using FraiseQL's fact table patterns.

### 5.5.1 Fact Table Arrow Projections

Fact tables (`tf_*`) with measures (SQL columns) and dimensions (JSONB) can be efficiently exported via Arrow:

```sql
<!-- Code example in SQL -->
-- Arrow view for fact table
CREATE VIEW av_sales AS
SELECT
    id,
    -- Measures (columnar, typed)
    revenue,
    quantity,
    cost,
    -- Dimensions (extracted from JSONB)
    data->>'category' AS category,
    data->>'region' AS region,
    data->>'product_name' AS product_name,
    -- Denormalized filters
    customer_id,
    occurred_at
FROM tf_sales
WHERE deleted_at IS NULL;
```text
<!-- Code example in TEXT -->

**Arrow schema**:

```json
<!-- Code example in JSON -->
{
  "fields": [
    {"name": "id", "type": "int64"},
    {"name": "revenue", "type": "decimal(10,2)"},
    {"name": "quantity", "type": "int32"},
    {"name": "cost", "type": "decimal(10,2)"},
    {"name": "category", "type": "utf8"},
    {"name": "region", "type": "utf8"},
    {"name": "product_name", "type": "utf8"},
    {"name": "customer_id", "type": "utf8"},
    {"name": "occurred_at", "type": "timestamp[us, UTC]"}
  ]
}
```text
<!-- Code example in TEXT -->

### 5.5.2 Pre-Aggregated Views for BI Tools

Pre-aggregated fact tables (e.g., `tf_sales_daily`) provide pre-computed rollups optimized for Arrow export:

```sql
<!-- Code example in SQL -->
-- Arrow view for daily aggregates
CREATE VIEW av_sales_daily AS
SELECT
    day,
    revenue,              -- Pre-aggregated SUM(revenue)
    quantity,             -- Pre-aggregated SUM(quantity)
    transaction_count,    -- Pre-aggregated COUNT(*)
    data->>'category' AS category,
    data->>'region' AS region
FROM tf_sales_daily;
```text
<!-- Code example in TEXT -->

**Use case**: BI tools (Tableau, PowerBI, Metabase) query `av_sales_daily` via Arrow for 10-100x faster data transfer compared to JSON.

### 5.5.3 Columnar Aggregation Optimization

Arrow's columnar format excels at exporting aggregated data:

**GraphQL Query**:

```graphql
<!-- Code example in GraphQL -->
query {
  sales_aggregate(
    groupBy: { category: true, region: true }
  ) @arrow {
    category
    region
    revenue_sum
    quantity_sum
    count
  }
}
```text
<!-- Code example in TEXT -->

**SQL Execution** (PostgreSQL):

```sql
<!-- Code example in SQL -->
SELECT
    data->>'category' AS category,
    data->>'region' AS region,
    SUM(revenue) AS revenue_sum,
    SUM(quantity) AS quantity_sum,
    COUNT(*) AS count
FROM tf_sales
GROUP BY data->>'category', data->>'region';
```text
<!-- Code example in TEXT -->

**Arrow Batch** (columnar layout):

- Column 1: `category` (utf8)
- Column 2: `region` (utf8)
- Column 3: `revenue_sum` (decimal)
- Column 4: `quantity_sum` (int32)
- Column 5: `count` (int64)

**Performance**: Arrow's columnar format minimizes memory allocation and enables SIMD operations for aggregates, providing 5-10x faster serialization compared to JSON.

### 5.5.4 Temporal Bucketing in Arrow

Temporal dimensions (day, week, month) are natively represented as Arrow temporal types:

```sql
<!-- Code example in SQL -->
CREATE VIEW av_sales_daily AS
SELECT
    DATE_TRUNC('day', occurred_at) AS day,  -- Arrow: date32
    SUM(revenue) AS revenue,
    COUNT(*) AS transaction_count
FROM tf_sales
GROUP BY DATE_TRUNC('day', occurred_at)
ORDER BY day;
```text
<!-- Code example in TEXT -->

**Arrow Schema**:

```json
<!-- Code example in JSON -->
{
  "fields": [
    {"name": "day", "type": "date32"},
    {"name": "revenue", "type": "decimal(10,2)"},
    {"name": "transaction_count", "type": "int64"}
  ]
}
```text
<!-- Code example in TEXT -->

### 5.5.5 Batching Strategy for Grouped Data

Arrow batches can be used to stream grouped aggregates incrementally:

**Scenario**: Export 1M rows grouped by category (100 categories, 10K rows each)

**Strategy**:

```sql
<!-- Code example in SQL -->
-- Batch 1: Electronics (10K rows)
SELECT * FROM av_sales WHERE category = 'Electronics' LIMIT 10000;

-- Batch 2: Clothing (10K rows)
SELECT * FROM av_sales WHERE category = 'Clothing' LIMIT 10000;

-- ... (100 batches total)
```text
<!-- Code example in TEXT -->

**Client receives**: 100 Arrow batches, each representing one category, enabling progressive rendering in BI dashboards.

### 5.5.6 Performance Benefits for Analytics

| Metric | JSON Plane | Arrow Plane | Improvement |
|--------|-----------|-------------|-------------|
| Serialization (1M rows) | 5-10s | 500ms-1s | 5-10x faster |
| Memory usage | 2-3GB | 500MB | 4-6x lower |
| BI tool ingestion | 30-60s | 5-10s | 3-6x faster |
| Column projection | Parse all fields | Read columns only | Zero-cost |
| Type safety | Runtime parsing | Compile-time schema | Type-safe |

**Related documentation**:

- `docs/specs/analytical-schema-conventions.md` - Fact table naming conventions
- `docs/architecture/analytics/aggregation-model.md` - Aggregation compilation
- `docs/guides/analytics-patterns.md` - Practical query patterns

---

## 6. Performance Characteristics

### 6.1 Throughput Benchmarks

**Reference Deployment** (PostgreSQL 15, 4-CPU, 16GB RAM, 100GB table):

| Query Type | Format | Rows/Query | Throughput | Latency | Memory |
|---|---|---|---|---|---|
| Single order | JSON | 1 | 5K/sec | 2ms | 10KB |
| Order with 10 items | JSON | 1 | 5K/sec | 5ms | 50KB |
| 1,000 orders | Arrow | 1,000 | 50K/sec | 20ms | 5MB |
| 10,000 orders | Arrow | 10,000 | 100K+/sec | 100ms | 50MB |
| Analytics (1M rows) | Arrow | 1,000,000 | 100M+/sec | 1s | 500MB |

**Key Observations:**

- Arrow excels at **bulk analytical queries** (100+ rows)
- JSON excels at **single-entity queries** (navigation patterns)
- Arrow memory is **proportional to batch size**, not query complexity
- **Columnar compression** enables 10M+ rows in <1GB

### 6.2 Optimization Strategies

✅ **Index key columns used in WHERE filters** — Faster batch filtering
✅ **Partition large tables by date** — Faster range scans for time-series
✅ **Use LIMIT for exploratory queries** — Reduces memory before aggregation
✅ **Enable table statistics (ANALYZE)** — Better query planning
✅ **Consider vertical partitioning** — Separate hot (queries) from cold (archival) columns

### 6.3 Latency Breakdown

For a 10,000-row Arrow query:

```text
<!-- Code example in TEXT -->
Network setup:        2ms
Query parse:          1ms
Authorization check:  1ms
Database execution:  50ms  ← Dominant
Arrow serialization: 30ms
Network transmission:10ms
─────────────────────────
Total:              ~94ms (target: <100ms)
```text
<!-- Code example in TEXT -->

---

## 7. Security & Data Governance

Arrow projections are data products that require **enterprise-grade security and governance** for adoption by BI teams, data warehouses, and compliance officers.

### 7.1 Row-Level Security (RLS)

Arrow projections inherit FraiseQL's **compile-time authorization rules**. Row-level filters are applied before batches are materialized:

**Example: User-scoped Orders**

```python
<!-- Code example in Python -->
@FraiseQL.arrow_projection(
  name="user_orders_analytics",
  security_context={
    "requires_auth": True,
    "row_filter": "user_id = {context.user_id}"
  }
)
class UserOrdersAnalytics:
    orders: Arrow.Batch([...])
    items: Arrow.Batch([...])
```text
<!-- Code example in TEXT -->

**Compilation Result:**

```sql
<!-- Code example in SQL -->
-- Generated WHERE clause includes authorization
CREATE VIEW v_user_orders_analytics_orders AS
SELECT ... FROM tb_order
WHERE user_id = $1          -- ← Authorization bound at query time
  AND deleted_at IS NULL;
```text
<!-- Code example in TEXT -->

**Runtime Behavior:**

```graphql
<!-- Code example in GraphQL -->
# Client query (authenticated as user_id=456)
query {
  userOrdersAnalytics {
    orders { id amount }
  }
}

# FraiseQL runtime applies:
# 1. Parse authentication context → user_id=456
# 2. Apply row filter: WHERE user_id = 456
# 3. Materialize Arrow batch (only user's orders)
# 4. No sensitive data leaks to BI tool
```text
<!-- Code example in TEXT -->

**Key guarantee:** Row-level security is **deterministic and auditable**. The same SQL predicate applies every time.

### 7.2 Column-Level Masking

Sensitive columns can be **masked** at projection time:

```python
<!-- Code example in Python -->
@FraiseQL.arrow_projection(name="customer_analytics")
class CustomerAnalytics:
    customers: Arrow.Batch(fields=[
        ArrowField("id", "String"),
        ArrowField("name", "String"),
        ArrowField("email", "String", mask="redact"),      # ← Masked column
        ArrowField("phone", "String", mask="hash"),        # ← Hashed
        ArrowField("ssn", "String", mask="encrypt"),       # ← Encrypted
    ])
```text
<!-- Code example in TEXT -->

**Masking Strategies:**

| Strategy | Input | Output | Use Case |
|---|---|---|---|
| `redact` | <alice@example.com> | [REDACTED] | PII removal |
| `hash` | <alice@example.com> | 7d8f92c... | Matching without revealing |
| `encrypt` | 123-45-6789 | {encrypted-blob} | Reversible encryption |
| `first_n` | <alice@example.com> | alic... | Partial reveal |

**Compile-Time Validation:**

```python
<!-- Code example in Python -->
# ✅ Allowed: Masking applies at projection time
ArrowField("email", "String", mask="redact")

# ❌ Error: Can't mask after projection (data already leaked)
ArrowField("email", "String", projection="raw")  # Then mask later
```text
<!-- Code example in TEXT -->

### 7.3 Auditability & Compliance

Arrow projection access is **fully auditable** for compliance (SOC 2, HIPAA, GDPR):

**Audit Trail:**

```json
<!-- Code example in JSON -->
{
  "timestamp": "2026-01-11T15:30:00Z",
  "event": "arrow_projection_fetched",
  "projection": "user_orders_analytics",
  "user_id": "user_456",
  "rows_returned": 1250,
  "format": "parquet",
  "destination": "s3://company-datalake/exports/",
  "authorization": {
    "filter_applied": "user_id = user_456",
    "masking_applied": ["email", "phone"],
    "compliant": true
  }
}
```text
<!-- Code example in TEXT -->

**Compliance Features:**

✅ **GDPR Right to Erasure** — Projection respects soft deletes (deleted_at IS NULL)
✅ **HIPAA Audit Logs** — All access logged with user, time, data scope
✅ **SOC 2 Row-Level Security** — Authorization enforced at database level
✅ **Data Residency** — Projections respect database region constraints
✅ **Retention Policies** — Arrow exports deleted after compliance window

### 7.4 Export Governance

When Arrow projections are exported to external systems (data lakes, Parquet files, cloud warehouses), governance policies apply:

```python
<!-- Code example in Python -->
@FraiseQL.arrow_projection(
  name="analytics_export",
  export_policy={
    "allowed_destinations": ["s3://company-data-lake", "bigquery://company"],
    "encryption": "required",
    "retention_days": 90,
    "requires_approval": True
  }
)
class AnalyticsExport:
    ...
```text
<!-- Code example in TEXT -->

**Export Types & Policies:**

| Export Type | Example | Approval | Encryption | Retention |
|---|---|---|---|---|
| **Direct Download** | CSV/Parquet via browser | User approval | Required | 30 days |
| **S3 Export** | Automated ETL pipeline | Admin approval | Required | 90 days |
| **BigQuery** | Cloud warehouse sync | Admin approval | At-rest encryption | 180 days |
| **Parquet Archive** | Data lake backup | System approval | Required | 365 days |

---

## 8. Multi-Database Support

### 8.1 PostgreSQL (Reference Implementation)

**Status:** ✅ Production-ready

**Features:**

- Full Arrow projection support
- Columnar compression via BRIN indexes
- Native decimal support (numeric type)
- Timezone-aware timestamps

**Indexes:**

```sql
<!-- Code example in SQL -->
CREATE INDEX idx_order_created_at_brin ON tb_order
  USING BRIN (created_at);
```text
<!-- Code example in TEXT -->

### 8.2 SQL Server

**Status:** ✅ Full support

**Features:**

- Arrow projections compile to SQL Server views
- Columnstore indexes for batch performance
- Native decimal2/numeric types

**Indexes:**

```sql
<!-- Code example in SQL -->
CREATE CLUSTERED COLUMNSTORE INDEX idx_order_cs
  ON tb_order;
```text
<!-- Code example in TEXT -->

### 8.3 MySQL

**Status:** ✅ Compatible

**Features:**

- Arrow projections work with standard views
- JSON functions for complex aggregations
- Decimal type support

**Notes:**

- No native columnar storage (use Infinispan external)
- Performance may be 2-5x slower than PostgreSQL/SQL Server

### 8.4 SQLite

**Status:** ⚠️ Limited

**Features:**

- Arrow projections supported
- Limited to in-process performance

**Limitations:**

- No columnar indexes
- <1M rows per query (memory constraint)
- Not recommended for analytics workloads

---

## 9. Implementation Phases

### Core Arrow Support (v2.1)

**Goals:**

- Compile Arrow projection definitions
- Generate Arrow views in database
- Serialize Arrow IPC streaming format
- Support simple, non-nested projections

**Deliverables:**

- Arrow schema compilation (from Python authoring)
- Arrow view DDL generation
- IPC serialization in Rust runtime
- Basic HTTP routing (Accept header)

**Timeline:** 4-6 weeks

### Advanced Features (v2.2)

**Goals:**

- Multi-batch composition with FK references
- Aggregate queries (GROUP BY, COUNT, SUM)
- Pagination support
- Parquet export

**Deliverables:**

- Multi-batch projection compilation
- Foreign key metadata in schema
- Aggregate function support
- OFFSET/LIMIT handling

**Timeline:** 4-6 weeks

### Performance & Optimization (v2.3)

**Goals:**

- Columnar compression (BRIN, Columnstore)
- Query optimization for large batches
- Caching of Arrow schemas
- Zero-copy deserialization

**Deliverables:**

- Index generation advice
- Query optimization recommendations
- Client-side caching headers
- Memory pooling in Rust runtime

**Timeline:** 2-4 weeks

### Ecosystem Integration (v2.4+)

**Goals:**

- Native BI tool connectors
- Parquet/Iceberg support
- Real-time streaming (Kafka, WebSocket)
- SDK improvements (Pandas, Polars, DuckDB)

**Deliverables:**

- Tableau native connector
- Parquet streaming format
- Kafka sink adapter
- Python/JS SDK helpers

**Timeline:** Ongoing

---

## 10. Limitations & Trade-offs

### ✅ Fully Supported

- Scalar types (String, Int, Float, Boolean, Decimal, DateTime, Date)
- Multiple independent batches with explicit FK
- Deterministic, compile-time schema
- Large projections (100K+ rows)
- All database targets (PostgreSQL, SQL Server, MySQL, SQLite)
- Columnar compression (database-dependent)
- Pagination via LIMIT/OFFSET
- Authorization via row-level filters

### ⚠️ Requires Database-Specific Handling

- Decimal precision (database determines best representation)
- Timestamp timezone handling (must be UTC in Arrow)
- Large text fields (may require compression)
- Nested objects (must be flattened to separate batches)

### ❌ Not Supported

- **Deeply nested selections** — Arrow is fundamentally flat/relational
  - Workaround: Use multiple Arrow projections for different relationship depths

- **Dynamic schema** — Schema is compile-time static
  - Workaround: Use JSON plane for dynamic/exploratory queries

- **Nested arrays in single batch** — Would violate columnar structure
  - Workaround: Normalize to separate batch with FK

- **User-provided transformation logic** — No custom serializers
  - Workaround: Transform after deserialization in client code

---

## 11. Examples

### Example 1: Simple Analytics Projection

**Schema Definition:**

```python
<!-- Code example in Python -->
@FraiseQL.type
class Product:
    id: FraiseQL.ID
    name: str
    price: FraiseQL.Decimal
    category: str

@FraiseQL.arrow_projection(name="products_analytics")
class ProductsAnalytics:
    products: Arrow.Batch(fields=[
        ArrowField("id", "String", nullable=False),
        ArrowField("name", "String", nullable=False),
        ArrowField("price", "Decimal128", nullable=False),
        ArrowField("category", "String", nullable=False),
    ])
```text
<!-- Code example in TEXT -->

**Query:**

```graphql
<!-- Code example in GraphQL -->
query {
  productsAnalytics {
    id
    name
    price
    category
  }
}
```text
<!-- Code example in TEXT -->

**Result (Arrow):**

```text
<!-- Code example in TEXT -->
products_batch:
  | id  | name          | price  | category    |
  | p1  | Widget        | 9.99   | Hardware    |
  | p2  | Gadget        | 29.99  | Electronics |
  | p3  | Gizmo         | 49.99  | Electronics |
```text
<!-- Code example in TEXT -->

### Example 2: Multi-Batch with Foreign Keys

**Schema Definition:**

```python
<!-- Code example in Python -->
@FraiseQL.type
class Customer:
    id: FraiseQL.ID
    name: str
    email: str
    orders: list['Order']

@FraiseQL.type
class Order:
    id: FraiseQL.ID
    customer_id: FraiseQL.ID
    total: FraiseQL.Decimal

@FraiseQL.arrow_projection(name="customers_with_orders")
class CustomersWithOrders:
    customers: Arrow.Batch(fields=[
        ArrowField("id", "String", nullable=False),
        ArrowField("name", "String", nullable=False),
        ArrowField("email", "String", nullable=False),
    ])

    orders: Arrow.Batch(fields=[
        ArrowField("id", "String", nullable=False),
        ArrowField("customer_id", "String", nullable=False, foreign_key="customers.id"),
        ArrowField("total", "Decimal128", nullable=False),
    ])
```text
<!-- Code example in TEXT -->

**Client Code (Pandas):**

```python
<!-- Code example in Python -->
import pyarrow as pa
import pandas as pd

reader = pa.ipc.open_stream(response.content)
customers = reader.get_batch(0).to_pandas()
orders = reader.get_batch(1).to_pandas()

# Join via foreign key
result = pd.merge(customers, orders,
                  left_on='id', right_on='customer_id')
```text
<!-- Code example in TEXT -->

### Example 3: Aggregated Metrics Batch

**Schema Definition:**

```python
<!-- Code example in Python -->
@FraiseQL.arrow_projection(name="daily_sales_metrics")
class DailySalesMetrics:
    metrics: Arrow.Batch(fields=[
        ArrowField("date", "Date32", nullable=False),
        ArrowField("total_orders", "Int32", nullable=False),
        ArrowField("total_revenue", "Decimal128", nullable=False),
        ArrowField("avg_order_value", "Decimal128", nullable=False),
    ])
```text
<!-- Code example in TEXT -->

**Compiled SQL:**

```sql
<!-- Code example in SQL -->
SELECT
  DATE(created_at) as date,
  COUNT(*) as total_orders,
  SUM(total) as total_revenue,
  AVG(total) as avg_order_value
FROM tb_order
WHERE deleted_at IS NULL
GROUP BY DATE(created_at)
ORDER BY date DESC;
```text
<!-- Code example in TEXT -->

---

## 12. Related Specifications

- **`core/execution-model.md` Section 14** — Arrow execution pipeline within query execution
- **`core/compilation-pipeline.md` Phase X** — Arrow schema compilation and SQL generation
- **`integration/federation.md`** — Why Arrow plane is separate from federation (protocol vs. format)
- **`core/authoring-languages.md`** — Arrow projection syntax in Python, TypeScript, YAML
- **`database/database-targeting.md` Section Y** — Database-specific Arrow implementations

---

## 13. Decision Guide: When to Use Each Plane

### Quick Reference: Plane Selection Matrix

| Scenario | Plane | Reason |
|----------|-------|--------|
| **User profile screen** | JSON | Single entity, nested relationships natural |
| **Admin dashboard** | Arrow | Bulk read, columnar filtering efficient |
| **CSV export** | Arrow | Batch format, zero client-side joining needed |
| **Mobile app feed** | JSON | Network-conscious, small payloads, nested structure |
| **ML feature extraction** | Arrow | Columnar format, DataFrame-friendly, batch operations |
| **Ad-hoc GraphQL exploration** | JSON | REST-style, interactive, variable nesting |
| **Data warehouse sync** | Arrow | Large volume, Parquet export, staging optimization |
| **Real-time notification** | JSON | Event-driven, small payloads, GraphQL subscriptions |
| **Analytics dashboard** | Arrow | Time-series data, aggregation, bulk reads |
| **Search results page** | JSON | Paginated results, nested metadata, UI-oriented |
| **Business intelligence tools** | Arrow | Tableau, Looker native connectors, stable schemas |
| **Data lake ingestion** | Arrow | Iceberg/Parquet, columnar compression, CDC format |

### Decision Tree

```text
<!-- Code example in TEXT -->
Start: "I want to query FraiseQL data"

├─ Is it for an application user interface?
│  ├─ YES → Use JSON plane
│  │        (UI naturally consumes nested objects)
│  └─ NO → Continue
│
├─ Is it for analytics, BI tools, or data science?
│ ├─ YES → Use Arrow plane
│  │        (Columnar, bulk-oriented, BI tools expect it)
│  └─ NO → Continue
│
├─ Is it for exporting to external systems?
│  ├─ YES → Use Arrow plane
│  │        (Parquet, CSV, cloud warehouses)
│  └─ NO → Continue
│
├─ Is it for real-time, low-latency requests?
│  ├─ YES → Use JSON plane
│  │        (Smaller payload, optimized for single entity)
│  └─ NO → Continue
│
└─ Is it for batch processing 1M+ rows?
   ├─ YES → Use Arrow plane
   │        (Keyset pagination, columnar performance)
   └─ NO → Either (choose based on nesting preference)
```text
<!-- Code example in TEXT -->

### Performance Considerations

**Choose JSON if:**

- Entity depth < 3 levels (minimal nesting)
- Result set < 10,000 rows
- Client has immediate UI dependency
- Real-time performance critical

**Choose Arrow if:**

- Bulk read (10K+ rows)
- Columnar operations natural (Pandas, Spark, DuckDB)
- Need to export to data warehouse
- Stable schema versioning important (BI tools)
- Multi-batch analysis

### Cost Comparison

| Operation | JSON | Arrow | Winner |
|-----------|------|-------|--------|
| Single entity + 2 levels | 2-5ms | 10-20ms | JSON |
| 1,000 rows flat | 50-100ms | 20-50ms | Arrow |
| 1M rows pagination | Variable | <50ms | Arrow |
| Small mobile payload | 5-50KB | 100KB+ | JSON |
| Parquet export | Requires conversion | Native | Arrow |
| BI tool integration | Custom work | Native | Arrow |

### Client Library Support

**JSON Plane:**

- Apollo Client (JavaScript)
- Relay (JavaScript)
- Apollo iOS/Android
- Python requests + graphql-core
- Any GraphQL client library

**Arrow Plane:**

- Apache Arrow (all languages)
- Pandas (Python)
- Polars (Rust, Python)
- DuckDB (SQL)
- Parquet libraries (Spark, Java, Go, Rust)
- PyArrow (Python)
- Arrow-JS (JavaScript)

---

## 13.5 The Delta Plane: Event Streams

**Not theoretical—already fully specified.** FraiseQL has a **third plane for event-driven systems** built from the ground up alongside JSON and Arrow as an integral part of the architecture.

### The Three Planes (Unified Architecture)

```text
<!-- Code example in TEXT -->
JSON Plane      ← Interaction (application queries, request-response)
Arrow Plane     ← Computation (analytics, bulk reads, columnar)
Delta Plane     ← Change Data (events, real-time streams, CDC)
```text
<!-- Code example in TEXT -->

All three planes source from the same database transactions.

### Delta Plane: What Exists Today

**Defined in:** `architecture/realtime/subscriptions.md` (1,600+ lines, complete)

**CDC Format:** `specs/cdc-format.md` (Debezium-compatible, all databases)

**Infrastructure:** `tb_entity_change_log` (durable event buffer, materialized view)

The Delta plane is **already implemented in subscriptions architecture** with:

✅ **Event capture** — PostgreSQL LISTEN/NOTIFY + CDC logging
✅ **Durable buffer** — `tb_entity_change_log` table persists events
✅ **Transport adapters** — graphql-ws, webhooks, Kafka, gRPC
✅ **Replay capability** — Events queryable from any point in time
✅ **Deterministic ordering** — Monotonic sequence numbers
✅ **Compile-time filtering** — WHERE clauses on subscription streams
✅ **Multi-tenant safety** — Row-level security enforced
✅ **Batch efficiency** — 100K+ events/second throughput target

### How Delta Plane Complements Arrow

**Arrow plane:** "Give me 10K rows for analysis"

```sql
<!-- Code example in SQL -->
SELECT * FROM orders LIMIT 10000
-- Keyset pagination for next batch
```text
<!-- Code example in TEXT -->

**Delta plane:** "Stream me ORDER changes in real-time"

```graphql
<!-- Code example in GraphQL -->
subscription {
  orderCreated(where: { status: "HIGH_VALUE" }) {
    id, amount, customer_id, timestamp
    cursor  # For replay from this point
  }
}
```text
<!-- Code example in TEXT -->

**Both use identical::**

- Authorization model (compile-time WHERE filters)
- Batching semantics (chunked delivery)
- Type system (same schema)
- Multi-database support (PostgreSQL, SQL Server, MySQL)

### Use Cases: Delta Plane Today

**Real-time dashboards**

```graphql
<!-- Code example in GraphQL -->
subscription {
  orderUpdated(where: { store_id: $storeId }) {
    id, status, amount, updated_at
  }
}
# Delivered via graphql-ws (<5ms latency, reference deployment)
```text
<!-- Code example in TEXT -->

**Incremental data lake sync**

```graphql
<!-- Code example in GraphQL -->
subscription {
  customerUpdated(where: { modified_after: $cursor }) {
    # All customer fields
    cursor  # For next sync
  }
}
# Delivered via Kafka (<5ms to broker)
# Consumed by Delta Lake, Iceberg for incremental materialization
```text
<!-- Code example in TEXT -->

**Event-driven federation**

```graphql
<!-- Code example in GraphQL -->
subscription {
  # FraiseQL User subgraph subscribes to Order changes
  # Notifies downstream that user's order count changed
  orderCreated(where: { user_id: { ANY: $user_ids } }) {
    user_id, order_count, timestamp
  }
}
```text
<!-- Code example in TEXT -->

**Audit trail (compliance)**

- All mutations automatically appear in Delta plane
- Immutable, time-sequenced events
- Exportable to compliance systems

### Event Format (Already Specified)

**CDC event structure** (from `specs/cdc-format.md`):

```json
<!-- Code example in JSON -->
{
  "event_id": "evt_550e8400...",
  "event_type": "entity:created | entity:updated | entity:deleted",
  "timestamp": "2026-01-11T15:35:00.123456Z",
  "sequence_number": 4521,
  "entity_type": "Order",
  "entity_id": "order_12345",
  "operation": {
    "before": { ... },
    "after": { ... },
    "changed_fields": ["status", "amount"]
  },
  "source": {
    "database": "postgresql",
    "transaction_id": "1234567890"
  }
}
```text
<!-- Code example in TEXT -->

Same format across all databases and transports.

### Why Three Planes Make Sense

| Plane | Model | Transport | Query Pattern |
|-------|-------|-----------|---------------|
| JSON | Nested graph | HTTP/GraphQL | Request-response |
| Arrow | Columnar tables | HTTP/Parquet | Scan large volumes |
| Delta | Change stream | WebSocket/Kafka | Subscribe to events |

**One schema, optimized for three distinct access patterns.**

This is why FraiseQL can be:

- An application GraphQL API (JSON)
- An analytics accelerator (Arrow)
- An event backbone (Delta)

...without three separate systems.

### Delta Plane Capabilities

**Core Event Capture & Delivery**

- Database-native event capture: LISTEN/NOTIFY (PostgreSQL), CDC (MySQL, SQL Server, SQLite)
- Durable event buffer (`tb_entity_change_log`) with monotonic sequence numbers
- Replay capability: Events queryable from any point in time
- Debezium-compatible event envelope format

**Transport Adapters**

- GraphQL WebSocket (graphql-ws) — Real-time UI subscription protocol
- Webhooks — Outgoing HTTP delivery with signature verification and retry logic
- Kafka — Event stream producer for data warehouse integration
- gRPC — Inter-service event delivery with server streaming

**Authorization & Filtering**

- Compile-time WHERE clause evaluation on event streams
- Row-level security (RLS) policies enforced at event capture
- Field-level masking and redaction
- Multi-tenant isolation guarantees
- Per-subscriber authorization context binding

**Performance & Operations**

- Sub-10ms event latency target (reference deployment)
- 100K+ events/second throughput capability
- Connection pooling and backpressure handling
- Event buffer capacity monitoring
- Delivery status tracking and diagnostics

---

## 14. FAQ

**Q: Can I query Arrow projections with GraphQL WHERE clauses?**

A: Yes. Arrow projections support filtering via WHERE conditions that compile to SQL WHERE clauses:

```graphql
<!-- Code example in GraphQL -->
query {
  orderWithItems(where: { total: { gte: 100 } }) {
    # Arrow results filtered to total >= 100
  }
}
```text
<!-- Code example in TEXT -->

**Q: What if my data doesn't fit in memory as Arrow?**

A: Use pagination with LIMIT/OFFSET. The FraiseQL runtime streams batches; clients deserialize incrementally:

```graphql
<!-- Code example in GraphQL -->
query {
  productsAnalytics(first: 10000, after: "cursor") {
    # Returns 10K rows, memory proportional to batch size
  }
}
```text
<!-- Code example in TEXT -->

**Q: Can I export Arrow directly to Parquet?**

A: Yes. Specify `Accept: application/parquet`:

```bash
<!-- Code example in BASH -->
curl -H "Accept: application/parquet" https://api.example.com/graphql
# Returns Parquet file directly
```text
<!-- Code example in TEXT -->

**Q: Does Arrow support nested JSON fields?**

A: No. Arrow is fundamentally columnar. Nested objects must be stored as JSON strings:

```python
<!-- Code example in Python -->
# ❌ Cannot do this:
ArrowField("metadata", "JSON")

# ✅ Do this instead:
ArrowField("metadata_json", "String")  # JSON-serialized

# Then deserialize in client:
metadata = json.loads(row['metadata_json'])
```text
<!-- Code example in TEXT -->

**Q: How do I join Arrow batches in SQL (e.g., DuckDB)?**

A: Download Arrow to Parquet, then query with SQL:

```python
<!-- Code example in Python -->
import duckdb

# Download Arrow → Parquet
parquet_url = 'https://api.example.com/graphql?format=parquet'
duckdb.execute(f"""
  SELECT customers.name, COUNT(orders.id) as order_count
  FROM read_parquet('{parquet_url}#customers') as customers
  LEFT JOIN read_parquet('{parquet_url}#orders') as orders
    ON customers.id = orders.customer_id
  GROUP BY customers.name
""")
```text
<!-- Code example in TEXT -->

---

## 15. Appendix: Arrow Type Reference

### Supported Arrow Types

| Arrow Type | FraiseQL Mapping | Example | Precision |
|---|---|---|---|
| `String` | `str`, `ID` | "hello" | UTF-8 |
| `LargeString` | Long text (>2GB) | Multi-megabyte strings | UTF-8 |
| `Int32` | `int` (small range) | 1,000,000 | -2³¹ to 2³¹-1 |
| `Int64` | `int` (large range) | Timestamp millis | -2⁶³ to 2⁶³-1 |
| `Float32` | `float` (low precision) | 3.14 | IEEE-754 single |
| `Float64` | `float` (default) | 3.141592653589 | IEEE-754 double |
| `Decimal128` | `Decimal` (standard) | $1,234.56 | 38 digits, 2 decimals |
| `Decimal256` | `Decimal` (high precision) | Scientific (1e38) | 76 digits, 2 decimals |
| `Bool` | `bool` | true/false | Single bit |
| `Date32` | `Date` | 2025-01-11 | Days since 1970-01-01 |
| `Timestamp(us, UTC)` | `DateTime` | 2025-01-11T15:30:00Z | Microsecond, UTC |
| `Time64(us)` | `Time` | 15:30:00.123456 | Microsecond precision |
| `Null` | Nullable fields | NULL | Missing value |
| `List<T>` | **Not in single batch** | Use separate batch with FK | Normalized |
| `Struct<...>` | **Not supported** | Use JSON string | Flattened |

### Arrow Schema Example (Full Reference)

```json
<!-- Code example in JSON -->
{
  "schema": {
    "fields": [
      {
        "name": "id",
        "type": "utf8",
        "nullable": false
      },
      {
        "name": "created_at",
        "type": {
          "type": "timestamp",
          "unit": "us",
          "timezone": "UTC"
        },
        "nullable": false
      },
      {
        "name": "price",
        "type": {
          "type": "decimal",
          "precision": 38,
          "scale": 2
        },
        "nullable": true
      }
    ]
  }
}
```text
<!-- Code example in TEXT -->

---

**Specification Status: Complete** — Arrow plane architecture fully defined and validated through implementation.

**Implementation Status: ✅ Complete** — Fully implemented in v2.0.0-alpha.1 (`FraiseQL-arrow` crate). Feature-gated via Cargo. Supports gRPC/Arrow Flight protocol with schema translation, streaming batches, and analytics queries.
