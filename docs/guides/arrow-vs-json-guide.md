# Arrow vs JSON Data Plane: Decision Guide

**Status:** ✅ Production Ready
**Audience:** Architects, Developers, Data Engineers
**Reading Time:** 15-20 minutes
**Last Updated:** 2026-02-05

## Quick Answer

```text
Use JSON (HTTP/GraphQL) for:
├─ Web applications (browser, mobile)
├─ Individual user requests
├─ Real-time updates
├─ Small-to-medium result sets
└─ API gateway scenarios

Use Arrow (gRPC/Flight) for:
├─ Analytics workloads (1M+ rows)
├─ Data export/ETL
├─ Dashboards (Tableau, Superset)
├─ Time-series analysis
├─ Large batch operations
└─ Direct data science access (Python, R)
```text

---

## Comparison Matrix

### Performance

| Metric | JSON | Arrow |
|--------|------|-------|
| **Latency (100 rows)** | 5-50ms | 5-50ms (similar) |
| **Latency (100K rows)** | 500-2000ms | 50-200ms |
| **Latency (1M rows)** | >5000ms | 200-500ms |
| **Throughput** | Limited by HTTP | High (gRPC) |
| **Serialization** | Text-based (large) | Binary (compact) |

### Data Transfer

| Size | JSON | Arrow | Reduction |
|------|------|-------|-----------|
| **100 rows** | 50KB | 40KB | 20% |
| **1K rows** | 500KB | 200KB | 60% |
| **10K rows** | 5MB | 1.5MB | 70% |
| **100K rows** | 50MB | 8MB | 84% |
| **1M rows** | 500MB | 50MB | 90% |

### Features

| Feature | JSON | Arrow |
|---------|------|-------|
| **Real-time updates** | ✅ WebSocket | ❌ Not yet |
| **Streaming** | ✅ Server-sent events | ✅ gRPC streaming |
| **Schema** | GraphQL schema | Apache Arrow schema |
| **Filtering** | ✅ GraphQL WHERE | ✅ SQL-like WHERE |
| **Aggregation** | ✅ Yes | ✅ Yes (better performance) |
| **Nested objects** | ✅ Natural in GraphQL | ⚠️ Flattened |
| **Browser access** | ✅ Direct | ❌ Requires proxy |
| **Python/R access** | ✅ HTTP client | ✅ Arrow Flight SDK |

### Use Case Suitability

| Use Case | JSON | Arrow |
|----------|------|-------|
| **Web app query** | ✅ Perfect | ❌ Not designed for |
| **Dashboard** | ⚠️ Slow | ✅ Perfect |
| **Mobile app** | ✅ Perfect | ❌ No |
| **Analytics query** | ❌ Too slow | ✅ Perfect |
| **Data export** | ⚠️ Slow, then convert | ✅ Direct export |
| **Real-time updates** | ✅ WebSocket | ⚠️ Polling only |
| **User-initiated query** | ✅ Perfect | ⚠️ Too complex |
| **Batch process** | ❌ Not ideal | ✅ Perfect |

---

## Decision Flowchart

### Question 1: Client Type?

```text
Browser or mobile app?
├─ YES → JSON ✅
│        (Only standard HTTP, no gRPC)
│
└─ NO: Data science / Analytics?
   ├─ YES → Arrow ✅
   │        (Python, R, Jupyter)
   │
   └─ NO: Backend service?
      └─ Depends on next question
```text

### Question 2: Data Volume?

```text
Result size typically...

< 10K rows?
├─ YES → JSON fine ✅
│        (Performance acceptable)
│
10K - 100K rows?
├─ → Could be either ⚠️
│   └─ Use JSON if already working
│   └─ Use Arrow if slow
│
> 100K rows?
└─ YES → Arrow ✅
         (JSON too slow)
```text

### Question 3: Update Frequency?

```text
Need real-time updates?
├─ YES → JSON (WebSocket) ✅
│        (Arrow not for real-time)
│
└─ NO: Refresh every...
   ├─ <1 second → JSON ✅
   ├─ 1-10 seconds → Either OK
   ├─ >10 seconds → Arrow preferred ✅
   └─ Hours/days → Arrow ✅
```text

### Question 4: Access Pattern?

```text
How will data be accessed?

Direct GraphQL queries?
├─ YES → JSON ✅
│
Jupyter notebook?
├─ YES → Arrow ✅
│
BI dashboard (Tableau, Superset)?
├─ YES → Arrow ✅
│
API from web app?
└─ YES → JSON ✅
```text

---

## Detailed Comparison

### JSON (HTTP/GraphQL)

**Best for:**

- Web applications
- Mobile applications
- Real-time subscriptions
- API gateway scenarios
- Individual user requests
- <10K rows typically

**Performance Profile:**

```text
Result Size  │ Latency    │ Throughput
─────────────┼────────────┼───────────
100 rows     │ 5-20ms     │ 1000+ req/s
1K rows      │ 20-100ms   │ 100-500 req/s
10K rows     │ 100-500ms  │ 10-100 req/s
100K rows    │ 1000-5000ms│ 1-10 req/s
1M rows      │ >10s       │ <1 req/s
```text

**Example:**

```graphql
query {
  users(limit: 1000) {
    id
    name
    email
    orders {
      id
      total
    }
  }
}
```text

**Response:**

```json
{
  "data": {
    "users": [
      {
        "id": "1",
        "name": "Alice",
        "email": "alice@example.com",
        "orders": [
          {"id": "1", "total": "100.00"}
        ]
      }
    ]
  }
}
```text

**Advantages:**

- ✅ Familiar (standard GraphQL)
- ✅ Real-time capable (WebSocket)
- ✅ Web/mobile friendly
- ✅ Human readable
- ✅ Easy debugging (cURL works)
- ✅ Nested objects natural

**Disadvantages:**

- ❌ Slow with large result sets
- ❌ Text-based (large payload)
- ❌ Serialization overhead
- ❌ Not ideal for analytics

**When to use:**

- Building user-facing features
- Need real-time updates
- Small-to-medium result sets
- Web/mobile clients

---

### Arrow (gRPC/Flight)

**Best for:**

- Analytics workloads
- Data export/ETL
- BI dashboards
- Data science notebooks
- >100K rows typically

**Performance Profile:**

```text
Result Size  │ Latency     │ Throughput
─────────────┼─────────────┼─────────────
100 rows     │ 5-20ms      │ 10000+ req/s
1K rows      │ 10-50ms     │ 1000-5000 req/s
10K rows     │ 50-100ms    │ 100-500 req/s
100K rows    │ 100-500ms   │ 10-100 req/s
1M rows      │ 200-1000ms  │ 1-10 req/s
10M rows     │ 1-5s        │ 0.1-1 req/s
```text

**Example:**

```python
import pyarrow.flight as flight
from pyarrow import csv

client = flight.connect(("localhost", 50051))

query = """
  SELECT user_id, COUNT(*) as order_count
  FROM orders
  GROUP BY user_id
  LIMIT 1000000
"""

table = client.do_get(flight.FlightDescriptor.for_command(query)).read_all()
print(f"Retrieved {len(table)} rows")

# Save to Parquet
csv.write_csv(table, "orders.parquet")
```text

**Response:** Binary Apache Arrow format (50-90% smaller than JSON)

**Advantages:**

- ✅ 10-100x faster for large datasets
- ✅ Binary format (compact, efficient)
- ✅ Columnar storage (analytics optimized)
- ✅ Direct Python/R/Pandas access
- ✅ Streaming support
- ✅ Perfect for data science

**Disadvantages:**

- ❌ Not for browsers (gRPC limitations)
- ❌ No real-time updates yet
- ❌ Requires Arrow client SDK
- ❌ Less human-readable
- ❌ Additional infrastructure (requires gRPC)

**When to use:**

- Analytics queries
- Data export
- BI dashboards
- Jupyter notebooks
- Python/R scripts
- Large result sets

---

## Use Case Examples

### Example 1: Web Dashboard (User Count)

**Query:** Show count of users by country

**Data volume:** 200 rows
**Update frequency:** Every 10 minutes
**Client:** React dashboard

**Choice:** JSON (HTTP) ✅

```graphql
query {
  users_by_country_aggregate {
    country
    count
  }
}
```text

**Why JSON:**

- Small result set (200 rows)
- Frequent UI updates
- HTTP standard in browser
- Real-time preferred (but polling OK)

---

### Example 2: Analytics Dashboard (Sales Data)

**Query:** Revenue trends by product category

**Data volume:** 1M rows before aggregation, 365 rows after
**Update frequency:** Daily
**Client:** Tableau dashboard

**Choice:** Arrow (gRPC) ✅

```python
import pyarrow.flight as flight

client = flight.connect(("fraiseql.internal", 50051))

query = """
  SELECT
    DATE_TRUNC('day', created_at) as date,
    category,
    SUM(revenue) as total_revenue,
    COUNT(*) as order_count
  FROM orders
  WHERE created_at > NOW() - INTERVAL '1 year'
  GROUP BY 1, 2
"""

table = client.do_get(
    flight.FlightDescriptor.for_command(query)
).read_all()

# Export to Tableau
table.to_pandas().to_csv("sales_trends.csv")
```text

**Why Arrow:**

- Large intermediate result set (1M rows)
- Columnar aggregation much faster
- 50-90% smaller data transfer
- Tableau has Arrow support
- Daily refresh acceptable (not real-time)

---

### Example 3: User Query (Orders)

**Query:** Show my recent orders

**Data volume:** 50 rows (typical user)
**Update frequency:** Real-time (WebSocket)
**Client:** Mobile app

**Choice:** JSON (HTTP) ✅

```graphql
query {
  my_orders(limit: 50) {
    id
    total
    created_at
    items {
      product_name
      quantity
      price
    }
  }
}
```text

**Why JSON:**

- Real-time updates needed (WebSocket)
- Mobile client (no gRPC support)
- Small result set (50 rows)
- Standard GraphQL experience

---

## Migration & Hybrid Approach

### Pattern: Start JSON, Add Arrow

```text
Phase 1: JSON Only (MVP)
  ├─ Web app: JSON ✅
  └─ Reports: Slow JSON ⚠️

Phase 2: Add Arrow (Analytics)
  ├─ Web app: JSON ✅
  ├─ Analytics: Arrow ✅
  └─ Reports: Now fast ✅

Result: Best of both worlds
```text

**Timeline:** 2-3 weeks to add Arrow support

### Pattern: Cached JSON, Arrow for Exports

```text
Real-time data: JSON (with caching)
  ├─ Frequent queries → Redis cache → Fast
  ├─ Infrequent queries → Database → Slower

Export/ETL: Arrow
  ├─ Large bulk exports → Arrow Flight → Fast
  └─ Data warehouse load → Direct Arrow → Efficient
```text

---

## Performance Tuning

### JSON Optimization

```graphql
# ❌ Too complex
query {
  users {
    id
    name
    orders {
      id
      items {
        product {
          category {
            name
          }
        }
      }
    }
  }
}

# ✅ Simpler (denormalize if needed)
query {
  users {
    id
    name
  }
}
query {
  orders {
    id
    user_id
    product_category
  }
}
```text

**Optimization strategies:**

- Limit result sets (pagination)
- Denormalize to flatten queries
- Use caching for repeated queries
- Implement query complexity scoring

### Arrow Optimization

```python
# ❌ Full table
query = "SELECT * FROM large_table"

# ✅ Filtered & aggregated
query = """
  SELECT
    user_id,
    COUNT(*) as count,
    SUM(revenue) as total
  FROM large_table
  WHERE created_at > NOW() - INTERVAL '30 days'
  GROUP BY user_id
"""
```text

**Optimization strategies:**

- Pre-aggregate in database
- Use WHERE to limit data
- Partition by date/region
- Use ClickHouse for analytics

---

## Troubleshooting Data Plane Choice

### "JSON queries are slow on large datasets"

**Problem:** JSON serialization overhead with millions of rows

**Solution:** Migrate large-result queries to Arrow

```bash
# Measure: Time the slow query
time curl -X POST http://api/graphql -d '{ query }'

# If >2 seconds for >10K rows: Use Arrow instead
```text

### "Arrow queries failing - schema mismatch"

**Diagnosis:**

1. Check schema in database: `SELECT * FROM table LIMIT 1;`
2. Check Arrow schema: `client.get_flight_info(...)`
3. Compare types: String vs Int, Timestamp vs Date

**Solutions:**

- Regenerate Arrow schema
- Ensure all values match declared type
- Use CAST in query if needed

### "Can't access Arrow from browser"

**Expected.** Arrow uses gRPC, which needs proxy layer.

**Solutions:**

- Use gRPC-Web proxy (Envoy, grpcwebproxy)
- Keep JSON API for browser clients
- Use Arrow for backend/analytics only

### "Arrow performance not better than JSON"

**Possible causes:**

- Result set too small (<10K rows)
- Network bottleneck (both equally affected)
- Query already optimized (not I/O bound)

**Check:**

- Is database query fast? `EXPLAIN ANALYZE`
- Is network latency limiting? Test locally
- Is result set actually large?

---

## Decision Summary Table

| Scenario | Choice | Reason |
|----------|--------|--------|
| Web app query | JSON | Browser, real-time |
| Mobile app | JSON | HTTP only |
| Analytics query (1M rows) | Arrow | 10-100x faster |
| Dashboard (Tableau) | Arrow | Columnar, efficient |
| Data export | Arrow | Bulk transfer |
| User-facing feature | JSON | Familiar GraphQL |
| Jupyter notebook | Arrow | Direct data access |
| Real-time updates | JSON | WebSocket ready |
| Batch job | Arrow | High throughput |
| Decision uncertain | JSON | Default to familiar |

---

## See Also

- **[Arrow Plane Architecture](../architecture/database/arrow-plane.md)** - Technical details
- **[Analytics Patterns](./analytics-patterns.md)** - Analytics use cases
- **[Arrow Flight Migration](../integrations/arrow-flight/migration-guide.md)** - Step-by-step adoption
- **[Performance Optimization](../architecture/performance/advanced-optimization.md)** - Tuning both planes

---

**Remember:** You don't have to choose one forever. Use JSON for web, Arrow for analytics. Both can coexist.
