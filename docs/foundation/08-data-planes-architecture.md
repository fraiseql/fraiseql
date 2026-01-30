# 2.3: Data Planes Architecture

**Audience:** Data engineers, backend architects, analytics teams, performance-critical applications
**Prerequisite:** Topics 1.3 (Database-Centric Architecture), 2.1 (Compilation Pipeline), 2.2 (Query Execution Model)
**Reading Time:** 20-25 minutes

---

## Overview

FraiseQL supports two distinct **data planes** for different use cases:

1. **JSON Plane** - For transactional queries (real-time, point queries, updates)
2. **Arrow Plane** - For analytical queries (bulk export, aggregations, streaming)

**Key Insight:** Not all data access patterns are equal. FraiseQL lets you choose the best execution path for your use case.

---

## The Two Data Planes

```
Client Request
        ↓
Is this a transaction or analytics query?
        ↓
    ┌───┴────┐
    ↓        ↓
JSON Plane   Arrow Plane
(OLTP)       (OLAP)
    ↓        ↓
  Result   Result
```

---

## Plane Selection Decision Tree

```
Query Type?
├─ Point query (get one user by ID)
│  └─ Use JSON Plane
│
├─ Small list (get 10 orders)
│  └─ Use JSON Plane
│
├─ Transactional mutation (update order)
│  └─ Use JSON Plane
│
├─ Bulk export (export 100K users)
│  └─ Use Arrow Plane
│
├─ Aggregation (total sales per category)
│  └─ Use Arrow Plane
│
├─ Real-time streaming (subscribe to updates)
│  └─ Use JSON Plane
│
└─ Time-series analysis (analyze sales by hour)
   └─ Use Arrow Plane
```

---

## JSON Plane: Transactional (OLTP)

### Purpose
Optimized for **transactional workloads**: point queries, small result sets, mutations, real-time responsiveness.

### Characteristics

| Aspect | Value |
|--------|-------|
| **Latency** | 10-50ms (typical) |
| **Throughput** | 100-2000 QPS per server |
| **Result Size** | 1-10,000 rows (typical) |
| **Serialization** | JSON |
| **Protocol** | HTTP (GraphQL) or WebSocket (subscriptions) |
| **Connection Model** | Connection pooling (persistent) |
| **Ideal For** | Web applications, mobile apps, real-time UIs |

### How It Works

**Query:**
```graphql
query GetUser($userId: Int!) {
  user(userId: $userId) {
    userId
    email
    createdAt
  }
}
```

**Execution (JSON Plane):**
```
Request
  ↓
Look up template
  ↓
Bind parameters
  ↓
Execute SQL: SELECT pk_user_id, email, created_at FROM tb_users WHERE pk_user_id = $1
  ↓
Fetch result from database
  ↓
Format as JSON
  ↓
Stream to client (HTTP response or WebSocket message)
  ↓
Response (complete, single payload)
```

**Response:**
```json
{
  "data": {
    "user": {
      "userId": 123,
      "email": "user@example.com",
      "createdAt": "2026-01-01T10:00:00Z"
    }
  }
}
```

### Performance Characteristics

**Latency Breakdown (10-50ms typical):**
```
Query lookup:        0.1ms
Parameter binding:   0.5ms
Authorization:       1.0ms
Database query:     20.0ms   ← Dominant factor
JSON serialization:  2.0ms
Network/streaming:   5.0ms
─────────────────────────────
Total:             ~28.6ms
```

**Throughput:**
```
Single server (4 CPUs, 8GB RAM):
- Simple queries: 1000-2000 QPS
- Average queries: 500-1000 QPS
- Complex queries: 100-500 QPS

Limiting factors:
- Database connection pool size
- Database capacity
- Network bandwidth
```

### Best Practices for JSON Plane

**1. Keep Result Sets Small**
```graphql
# ❌ Bad: Fetching too much data
query {
  users {
    userId
    email
    createdAt
    orders {
      orderId
      items {
        itemId
        name
        description  # Too much data
      }
    }
  }
}

# ✅ Good: Fetch only needed fields
query {
  users(limit: 10) {
    userId
    email
    orders(limit: 5) {
      orderId
      total
    }
  }
}
```

**2. Use Pagination for Lists**
```graphql
# ❌ Bad: No pagination
query {
  orders {
    orderId
    total
  }
}

# ✅ Good: Paginated
query GetOrders($userId: Int!, $limit: Int = 10, $offset: Int = 0) {
  orders(userId: $userId, limit: $limit, offset: $offset) {
    orderId
    total
  }
}
```

**3. Add Filters to Reduce Result Size**
```graphql
# ❌ Bad: Fetch all then filter client-side
query {
  orders {
    orderId
    total
    createdAt
  }
}

# ✅ Good: Filter at database
query GetRecentOrders($userId: Int!, $days: Int = 7) {
  orders(userId: $userId, createdAfter: $daysAgo(7)) {
    orderId
    total
    createdAt
  }
}
```

---

## Arrow Plane: Analytical (OLAP)

### Purpose
Optimized for **analytical workloads**: bulk export, aggregations, streaming, large result sets, columnar analysis.

### Characteristics

| Aspect | Value |
|--------|-------|
| **Latency** | 500ms-5s (typical) |
| **Throughput** | 10-100 QPS per server |
| **Result Size** | 10,000-1,000,000+ rows |
| **Serialization** | Apache Arrow (columnar binary format) |
| **Protocol** | Apache Arrow Flight |
| **Connection Model** | Persistent bidirectional stream |
| **Ideal For** | Data exports, analytics, BI tools, data science |

### How It Works

**Query:**
```graphql
query ExportSalesData($startDate: Date!, $endDate: Date!) {
  sales(dateRange: {start: $startDate, end: $endDate}) {
    saleId
    productId
    quantity
    unitPrice
    total
    createdAt
  }
}
```

**Execution (Arrow Plane):**
```
Request (Arrow Flight protocol)
  ↓
Look up template
  ↓
Bind parameters
  ↓
Check authorization
  ↓
Execute SQL (potentially with streaming)
  ↓
Convert results to Arrow columnar format
  ↓
Stream Arrow batches to client
  ↓
Client receives stream of Arrow batches (not complete in one payload)
```

**Response (Arrow Flight Streaming):**
```
Batch 1: 65,536 rows in Arrow format (binary, compressed)
  ↓
Batch 2: 65,536 rows in Arrow format
  ↓
Batch 3: 65,536 rows in Arrow format
  ↓
...
  ↓
Batch N: Remaining rows
  ↓
Stream complete
```

### Arrow vs JSON Format

**JSON Format (OLTP):**
```json
[
  {"saleId": 1, "productId": 10, "quantity": 5, "unitPrice": 29.99, "total": 149.95},
  {"saleId": 2, "productId": 20, "quantity": 2, "unitPrice": 49.99, "total": 99.98},
  {"saleId": 3, "productId": 30, "quantity": 1, "unitPrice": 199.99, "total": 199.99}
]

Size: ~450 bytes for 3 rows
```

**Arrow Format (OLAP):**
```
Columnar layout:
┌──────────────────────────────────────────────────┐
│ saleId:      [1, 2, 3, ...]                      │
│ productId:   [10, 20, 30, ...]                   │
│ quantity:    [5, 2, 1, ...]                      │
│ unitPrice:   [29.99, 49.99, 199.99, ...]         │
│ total:       [149.95, 99.98, 199.99, ...]        │
│ createdAt:   [timestamp, timestamp, ...]         │
└──────────────────────────────────────────────────┘

Size: ~120 bytes for 3 rows (binary + compression)
Compression ratio: 73% smaller than JSON
```

### Performance Characteristics

**Latency (for 100K row export):**
```
Initial query (setup):  500ms
Streaming results:      2-4s
─────────────────────────────
Total:                 ~2.5-4.5s

Factors:
- Query complexity
- Result set size
- Network bandwidth
- Compression overhead
- Client processing speed
```

**Throughput:**
```
Single server (4 CPUs, 8GB RAM):
- Small exports (1K-10K rows): 50-100 QPS
- Medium exports (10K-100K rows): 10-50 QPS
- Large exports (100K+ rows): 1-10 QPS

Limiting factors:
- Network bandwidth (Arrow streams are fast but still network-bound)
- CPU (columnar encoding/compression)
- Client processing speed
```

### Arrow Flight Protocol

Arrow Flight is an RPC framework built on Arrow columnar format:

```
Client                           Server
  │                               │
  ├─ GetFlightInfo (query) ───────>│
  │                                 │
  │<─ FlightInfo (schemas, tickets)─┤
  │                                 │
  ├─ DoGet (ticket) ──────────────>│
  │                                 │
  │<─ Arrow batch 1 ───────────────┤
  │<─ Arrow batch 2 ───────────────┤
  │<─ Arrow batch 3 ───────────────┤
  │<─ ... ─────────────────────────┤
  │<─ End of stream ───────────────┤
  │                                 │
```

### FraiseQL Arrow Flight Tickets

FraiseQL generates Arrow Flight tickets for different query types:

**Ticket 1: GraphQLQuery**
```json
{
  "type": "GraphQLQuery",
  "query": "query ExportSalesData($startDate: Date!) { sales(dateRange: {start: $startDate}) { saleId, productId, quantity } }",
  "variables": {"startDate": "2026-01-01"}
}
```

**Ticket 2: OptimizedView**
```json
{
  "type": "OptimizedView",
  "viewName": "va_sales_summary",
  "filters": {"startDate": "2026-01-01"},
  "projection": ["saleId", "productId", "quantity"]
}
```

**Ticket 3: BulkExport**
```json
{
  "type": "BulkExport",
  "table": "ta_sales_fact",
  "dateRange": {"start": "2026-01-01", "end": "2026-01-31"},
  "compression": "snappy"
}
```

### Best Practices for Arrow Plane

**1. Use for Large Result Sets**
```graphql
# ✅ Good: 100K+ rows, use Arrow
query ExportAllSales {
  sales {
    saleId
    productId
    quantity
    total
    createdAt
  }
}
# Arrow Flight will stream results in batches
```

**2. Use Materialized Views for Analytics**
```sql
-- Pre-computed analytics view
CREATE MATERIALIZED VIEW va_sales_summary AS
SELECT
  DATE(created_at) as sale_date,
  product_id,
  COUNT(*) as sale_count,
  SUM(total) as total_revenue
FROM tb_sales
GROUP BY DATE(created_at), product_id;

-- Query materialized view via Arrow
query GetSalesSummary($startDate: Date!) {
  salesSummary(dateRange: {start: $startDate}) {
    saleDate
    productId
    saleCount
    totalRevenue
  }
}
```

**3. Use Fact Tables for Denormalized Analytics**
```sql
-- Fact table with pre-denormalized dimensions
CREATE TABLE ta_sales_fact (
  pk_sale_id BIGINT PRIMARY KEY,
  sale_date DATE INDEXED,
  product_id INT,
  category TEXT,
  region TEXT,
  quantity INT,
  unit_price NUMERIC,
  total NUMERIC,
  created_at TIMESTAMP
);

-- Arrow can efficiently scan this without joins
```

**4. Stream in Batches**
```python
# Client side: Process Arrow batches as they arrive
for batch in arrow_stream:
    # Process batch_size rows (e.g., 65,536 rows)
    df = batch.to_pandas()  # Convert to Pandas
    # Process or aggregate
    print(f"Processed {len(df)} rows")
```

---

## Choosing Between JSON and Arrow

### Decision Matrix

| Scenario | Plane | Why |
|----------|-------|-----|
| **User dashboard** | JSON | Small result sets, low latency needed |
| **Mobile app query** | JSON | Bandwidth-constrained, small results |
| **List view (100 items)** | JSON | Small result set, real-time responsiveness |
| **Data export (100K rows)** | Arrow | Bulk data, columnar efficiency |
| **Analytics dashboard** | Arrow | Aggregations, multiple dimensions |
| **Real-time subscription** | JSON | Streaming updates, small payloads |
| **Batch ETL job** | Arrow | Large data movement, efficient format |
| **Business Intelligence tool** | Arrow | Multi-dimensional analysis |
| **Search results page** | JSON | Paginated, user-facing, real-time |
| **Data science export** | Arrow | Bulk analysis, columnar format |

---

## Performance Comparison

### Example: Exporting 100K rows

**JSON Plane:**
```
Query execution: 2000ms
JSON serialization: 5000ms
Network transfer (30MB JSON): 15000ms (on 20 Mbps connection)
Client processing: 2000ms
─────────────────────────────
Total: ~24 seconds
```

**Arrow Plane:**
```
Query execution: 2000ms
Arrow serialization: 500ms
Arrow compression: 1000ms
Network transfer (8MB Arrow, compressed): 3000ms (on 20 Mbps connection)
Client streaming: Concurrent with network
─────────────────────────────
Total: ~5-6 seconds
```

**Result:** Arrow is **4-5x faster** for bulk exports

### Example: Real-time Dashboard (10 rows)

**JSON Plane:**
```
Query execution: 20ms
JSON serialization: 1ms
Network transfer: 1ms
─────────────────────────
Total: ~22ms
```

**Arrow Plane:**
```
Query execution: 20ms
Arrow setup: 50ms (overhead not worth it for small result)
Network transfer: 2ms
─────────────────────────
Total: ~72ms
```

**Result:** JSON is **3x faster** for small results (Arrow overhead not worth it)

---

## Real-World Examples

### Example 1: E-Commerce Dashboard (JSON Plane)

```graphql
# Real-time dashboard showing latest orders
query GetDashboard($userId: Int!) {
  # Small result set, needs low latency
  recentOrders: orders(userId: $userId, limit: 10) {
    orderId
    total
    createdAt
  }

  # Small aggregation, needs real-time
  orderCount: ordersCount(userId: $userId)

  # Small user details
  user: user(userId: $userId) {
    email
    lastLogin
  }
}

# JSON Response (~2KB)
# Latency: ~30ms
# Perfect for real-time UI updates
```

### Example 2: Monthly Sales Analysis (Arrow Plane)

```graphql
# Bulk export for data science analysis
query ExportMonthlySales($month: Int!, $year: Int!) {
  # Large result set, analytical query
  sales(dateRange: {start: "$year-$month-01", end: "..."}) {
    saleId
    productId
    productName
    category
    quantity
    unitPrice
    total
    saleDate
    region
    salesperson
  }
}

# Arrow Flight Response (~50MB+ data, 100K+ rows)
# Streaming in batches (65,536 rows per batch)
# Latency: ~5 seconds for full export
# Perfect for data science & analytics
```

### Example 3: Real-Time Subscription (JSON Plane)

```graphql
# Real-time updates via WebSocket
subscription OnOrderCreated($userId: Int!) {
  orderCreated(userId: $userId) {
    orderId
    total
    createdAt
  }
}

# JSON pushed via WebSocket as orders arrive
# Latency: ~100ms (for each update)
# Payload: ~500 bytes per update
# Perfect for live notifications
```

### Example 4: Data Warehouse Sync (Arrow Plane)

```graphql
# Daily ETL job syncing to data warehouse
query ExportAllProductMetrics($date: Date!) {
  # Millions of rows for analytics platform
  productMetrics(dateRange: {start: $date, end: $date}) {
    productId
    productName
    category
    viewCount
    clickCount
    conversionRate
    revenuePerVisitor
    averageRating
  }
}

# Arrow Flight Response (~500MB+, millions of rows)
# Processed in streaming batches
# Latency: ~30-60 seconds for full sync
# Perfect for bulk data warehouse operations
```

---

## Architecture Integration

### JSON Plane in System Architecture

```
Web Browser
     ↓ (HTTP/WebSocket)
FraiseQL Server (JSON Plane)
     ├─ Query lookup (O(1))
     ├─ Parameter validation
     ├─ Authorization
     ├─ SQL execution
     └─ JSON serialization
     ↓
PostgreSQL
     (10-50ms latency)
     ↓
Response (JSON)
     ↓
Browser renders
```

### Arrow Plane in System Architecture

```
Data Science App
     ↓ (Arrow Flight protocol)
FraiseQL Server (Arrow Plane)
     ├─ Query lookup
     ├─ Parameter validation
     ├─ Authorization
     ├─ SQL execution (streaming)
     └─ Arrow serialization + compression
     ↓
PostgreSQL
     (stream results)
     ↓
Arrow Flight Stream (batches)
     ↓
Data Science App processes batches
     (5s-1m latency, streaming)
```

---

## Related Topics

- **Topic 1.3:** Database-Centric Architecture (data planes overview)
- **Topic 2.1:** Compilation Pipeline (how queries are optimized)
- **Topic 2.2:** Query Execution Model (execution for JSON plane)
- **Topic 2.7:** Performance Characteristics (performance tuning)
- **Topic 5.1:** Performance Optimization (using data planes effectively)

---

## Summary

FraiseQL supports two data planes optimized for different workloads:

**JSON Plane (OLTP - Transactional):**
- Latency: 10-50ms
- Throughput: 100-2000 QPS
- Best for: User-facing queries, real-time UIs, small result sets
- Protocol: HTTP/GraphQL or WebSocket
- Format: JSON

**Arrow Plane (OLAP - Analytical):**
- Latency: 500ms-5s
- Throughput: 10-100 QPS
- Best for: Data exports, analytics, large result sets, bulk operations
- Protocol: Apache Arrow Flight
- Format: Arrow columnar (5-10x more efficient than JSON for bulk data)

**Choose JSON Plane for:**
- Transactional workloads
- Small result sets (< 10,000 rows)
- Real-time responsiveness required
- Web and mobile applications

**Choose Arrow Plane for:**
- Analytical workloads
- Large result sets (> 10,000 rows)
- Bulk data exports
- Data warehouse syncing
- Data science & BI tools

**Performance Impact:**
- Arrow is 4-5x faster for bulk exports (100K+ rows)
- JSON is 3x faster for small results (< 100 rows)
- Both leverage pre-compiled, optimized SQL
- Choose based on your actual use case, not hype
