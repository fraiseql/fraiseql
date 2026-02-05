# Common Gotchas & Pitfalls

**Status:** ✅ Production Ready
**Audience:** Developers, Architects
**Reading Time:** 20-25 minutes
**Last Updated:** 2026-02-05

Learn from common mistakes and pitfalls when using FraiseQL. Each gotcha includes diagnosis steps and solutions.

---

## Overview

This guide documents common mistakes, surprising behaviors, and anti-patterns discovered through production use. Understanding these pitfalls will help you avoid costly mistakes.

---

## 1. N+1 Query Problem

### The Problem

**Symptom:** Application is slow despite queries looking simple. Database receives many small queries instead of one large query.

**Example:**

```graphql
query {
  users {
    id
    name
    posts {      # ← This causes N+1!
      id
      title
    }
  }
}
```text

**What happens:**

1. Query fetches 100 users → 1 database query
2. For EACH user, fetches their posts → 100 more queries
3. Total: 101 queries instead of 1

### Why This Happens

FraiseQL executes nested fields one level at a time. Without optimization, it fetches parent entities first, then child entities separately.

### How to Diagnose

**Check database query count:**

```bash
# Enable query logging
FRAISEQL_LOG_LEVEL=debug cargo run

# Count queries in logs
grep "SELECT" logs.txt | wc -l
```text

**Check profiling output:**

```graphql
query {
  users {
    id
    __fraiseql_timing {
      executionTimeMs
      queryCount
    }
  }
}
```text

### Solutions

**Solution 1: Use batch fetching (RECOMMENDED)**

FraiseQL automatically batches nested field queries:

```graphql
query {
  users(first: 100) {
    id
    name
    posts(first: 50) {   # Batched! All users' posts in ~1 query
      id
      title
    }
  }
}
```text

**Result:** ~2 queries total (users + batched posts)

**Solution 2: Use table-backed views (tv_*)**

```python
@FraiseQL.type
class UserWithPosts:
    """Denormalized view with posts included."""
    id: ID
    name: str
    posts_json: List[PostSummary]  # Pre-fetched via view
```text

**Solution 3: Flatten queries temporarily**

Instead of:

```graphql
query {
  users { posts { comments { likes } } }
}
```text

Do:

```graphql
query {
  users { id posts { id } }
}

query {
  posts { id comments { id } }
}

query {
  comments { id likes }
}
```text

**Solution 4: Add pagination to nested fields**

```graphql
query {
  users(first: 50) {          # Smaller parent batch
    id
    name
    posts(first: 10) {        # Smaller child batch
      id
      title
    }
  }
}
```text

### Prevention

- ✅ Monitor query count in production logs
- ✅ Set up alerts for >50 queries per request
- ✅ Use profiling tools to detect N+1 early
- ✅ Test with large datasets (1000+ records)
- ✅ Document expected query count for each resolver

---

## 2. Pagination Edge Cases

### Edge Case: Offset Pagination Past End

**Symptom:** Query with `skip: 10000` returns empty results, but data exists.

**Why:** Offset pagination becomes inefficient with large offsets. After row 10,000, the database must skip 10,000 rows for every query.

**Solutions:**

**Use keyset pagination (RECOMMENDED):**

```graphql
query {
  users(after: "user123", first: 100) {
    id
    name
  }
}
```text

**Keyset advantages:**

- Constant performance regardless of offset
- Works with sorting
- Handles inserts/deletes during pagination

**Or limit maximum offset:**

```toml
[FraiseQL.pagination]
max_offset = 5000  # Disallow offset > 5000
```text

### Edge Case: Results Changing During Pagination

**Symptom:** When paginating through results, you get duplicate records or skip records.

**Why:** If data is inserted/deleted between pagination requests, result set changes.

**Example:**

```text
Request 1: skip 0, take 10   → gets records 1-10
[New record inserted]
Request 2: skip 10, take 10  → gets records 12-21 (record 11 is new)
Result: Skipped record 11!
```text

**Solutions:**

**Use keyset pagination:**

```graphql
query {
  users(after: "cursor_from_previous", first: 10) {
    id
    cursor
  }
}
```text

Keyset pagination uses the last record's ID as cursor, immune to inserts.

**Or use snapshot isolation:**

```graphql
query {
  users(snapshotAt: "2026-02-05T10:00:00Z", skip: 10, take: 10) {
    id
  }
}
```text

### Edge Case: Cursor Expiry

**Symptom:** Pagination cursor becomes invalid after database changes.

**Why:** Cursor points to a record that was deleted or modified.

**Solution:**

**Handle expired cursor gracefully:**

```python
try:
    result = await client.query(query, variables={"after": cursor})
except FraiseQLError as e:
    if e.code == "E_PAGINATION_CURSOR_EXPIRED":
        # Restart from beginning or last valid position
        cursor = None
        result = await client.query(query, variables={"after": cursor})
```text

---

## 3. Cache Invalidation Timing

### Gotcha: Stale Cache After Mutation

**Symptom:** Mutation succeeds, but query still returns old cached value.

**Example:**

```graphql
mutation {
  updateUser(id: "123", name: "Alice") {
    id
    name
  }
}

query {
  user(id: "123") {
    name  # Still returns old name!
  }
}
```text

**Why:** Cache key doesn't match. Query uses `{id: "123"}`, but mutation might cache invalidate `{id: "123", name: "Alice"}`.

### Solutions

**Solution 1: Explicit cache invalidation**

```graphql
mutation {
  updateUser(id: "123", name: "Alice") @cache(invalidate: true) {
    id
    name
  }
}
```text

**Solution 2: TTL-based invalidation**

```toml
[FraiseQL.caching]
ttl_seconds = 60  # All caches expire after 60 seconds
```text

**Solution 3: Dependency-based invalidation**

```python
@FraiseQL.mutation
def update_user(id: str, name: str):
    # Mark all queries involving this user as invalid
    cache.invalidate(User, id=id)
    return update_user_in_db(id, name)
```text

### Gotcha: Cache Hit When You Need Fresh Data

**Symptom:** Critical data is cached but needs to be fresh for real-time operations.

**Example:**

```graphql
query {
  inventory(productId: "123") {
    quantity  # Cached for 5 minutes, but inventory changes every second!
  }
}
```text

### Solutions

**Solution 1: Disable caching for critical fields**

```python
@FraiseQL.type
class Inventory:
    id: ID
    quantity: int = field(cache=False)  # Never cache inventory
    updated_at: DateTime = field(cache=False)
```text

**Solution 2: Use subscriptions for real-time data**

```graphql
subscription {
  inventoryChanged(productId: "123") {
    quantity
    updated_at
  }
}
```text

**Solution 3: Add versioning to cache keys**

```graphql
query {
  user(id: "123", version: "current") {  # Always gets latest
    id
    name
  }
}
```text

---

## 4. Authorization Bypass via Field Omission

### Gotcha: Forgetting Field-Level Authorization

**Symptom:** Sensitive field is readable by unauthorized users.

**Example:**

```python
@FraiseQL.type
class User:
    id: ID
    name: str
    email: str
    password_hash: str  # ← OOPS! No @authorize decorator
    salary: Decimal    # ← OOPS! No @authorize decorator
```text

**Why:** Field-level authorization is opt-in. If you don't add `@authorize`, the field is readable by anyone.

### Solution

**Add authorization to every sensitive field:**

```python
@FraiseQL.type
class User:
    id: ID
    name: str
    email: str = field(authorize={Roles.ADMIN, Roles.SELF})
    password_hash: str = field(authorize=set())  # Never readable
    salary: Decimal = field(authorize={Roles.HR, Roles.SELF})
```text

**Or use row-level security:**

```python
@FraiseQL.type
class User:
    where: Where = FraiseQL.where(
        fk_org=FraiseQL.context.org_id,  # Only users in same org
        is_sensitive_visible=FraiseQL.context.role in [Roles.ADMIN, Roles.SELF]
    )
```text

---

## 5. Type Mismatches in Filters

### Gotcha: String vs Number Comparison

**Symptom:** Filter doesn't match expected records, or returns error.

**Example:**

```graphql
query {
  products(where: { id: { eq: "123" } }) {  # String
    id
  }
}
```text

**Database schema:**

```sql
CREATE TABLE products (
  id INT PRIMARY KEY,  -- Number!
  ...
);
```text

**Why:** Type mismatch. GraphQL string `"123"` doesn't match database INT.

### Solution

**Ensure types match in schema:**

```python
@FraiseQL.type
class Product:
    id: int          # Use int, not str
    sku: str         # Use str for text IDs
    price: Decimal   # Use Decimal for money, not float
```text

### Gotcha: NULL Handling in WHERE Clauses

**Symptom:** Filter with NULL doesn't work as expected.

**Example:**

```graphql
query {
  users(where: { middleName: { eq: null } }) {  # Finds users WITH middle names!
    id
  }
}
```text

**Why:** In SQL, `column = NULL` returns false (use `IS NULL` instead).

### Solution

**Use is_null operator:**

```graphql
query {
  users(where: { middleName: { is_null: true } }) {  # Correct!
    id
  }
}
```text

---

## 6. Circular Dependencies in Federation

### Gotcha: Type A extends Type B, Type B extends Type A

**Symptom:** Schema compilation fails with "circular dependency" error.

**Example:**

```python
# users-service
@FraiseQL.type
@extends
class Order:
    id: str = external()
    user: User  # Extends to User

# orders-service
@FraiseQL.type
@extends
class User:
    id: str = external()
    orders: List[Order]  # Extends back to Order
```text

### Solution

**Flatten the hierarchy:**

```python
# Solution: Only users-service owns User, only orders-service owns Order
# Don't create bidirectional extends

# users-service
@FraiseQL.type
@key("id")
class User:
    id: str
    name: str

# orders-service
@FraiseQL.type
@key("id")
class Order:
    id: str
    user_id: str  # Foreign key reference, not @extends
```text

---

## 7. SAGA Timeout During Long Operations

### Gotcha: SAGA Times Out Before Compensation Can Complete

**Symptom:** SAGA succeeds partially, then times out during compensation.

**Why:** Long-running database operations (data migration, bulk updates) take longer than SAGA timeout.

### Solutions

**Solution 1: Increase SAGA timeout**

```toml
[FraiseQL.federation.sagas]
timeout_seconds = 300  # 5 minutes instead of default 30 seconds
```text

**Solution 2: Break into smaller steps**

```python
@FraiseQL.saga
async def bulk_update_users(user_ids: List[str]):
    # Instead of updating 10,000 users in one step:
    # Break into batches of 100
    for batch in chunks(user_ids, 100):
        step = await saga.add_step(
            update_user_batch,
            args=[batch],
            undo=undo_user_batch
        )
```text

**Solution 3: Use async tasks instead**
For very long operations, use background tasks instead of SAGA:

```python
@FraiseQL.mutation
async def bulk_update_users(user_ids: List[str]):
    # Queue background job
    background_tasks.add_task(
        bulk_update_in_background,
        user_ids=user_ids
    )
    return { "status": "processing", "job_id": job_id }
```text

---

## 8. View Performance Degradation

### Gotcha: Logical View Gets Slower Over Time

**Symptom:** Query that was fast at launch gets slower as table grows.

**Why:** Logical views (v_*) compute aggregations on-the-fly. With 1M rows, computing JSON aggregation is expensive.

### Solution

**Switch to table-backed views (tv_*):**

```python
# Replace v_user_summary (logical view):
@FraiseQL.type
class UserSummary:  # Was v_user_summary
    id: ID
    name: str
    post_count: int

# With tv_user_summary_materialized (table-backed):
@FraiseQL.type
class UserSummary:  # Now tv_user_summary_materialized
    id: ID
    name: str
    post_count: int
    updated_at: DateTime
```text

**Table-backed views advantages:**

- Pre-computed and stored (fast reads)
- No recalculation per query
- Trade-off: requires refresh strategy

---

## 9. Date/Time Timezone Issues

### Gotcha: DateTime vs Date Comparison

**Symptom:** Date filter includes wrong records or excludes correct ones.

**Example:**

```graphql
query {
  orders(where: { createdAt: { gte: "2026-02-05" } }) {
    id
  }
}
```text

**Problem:** `"2026-02-05"` is interpreted as `2026-02-05T00:00:00Z`. If user created order at `2026-02-04T23:00:00Z` (previous day in their timezone), it won't match.

### Solutions

**Solution 1: Use DateTime with timezone**

```graphql
query {
  orders(where: {
    createdAt: {
      gte: "2026-02-05T00:00:00-05:00"  # Explicit timezone
    }
  }) {
    id
  }
}
```text

**Solution 2: Use Date type for date-only fields**

```python
@FraiseQL.type
class Order:
    id: ID
    created_date: Date  # Use Date, not DateTime
    created_at: DateTime  # Use DateTime for timestamps
```text

**Solution 3: Compare at database level**

```sql
SELECT * FROM orders
WHERE DATE(created_at AT TIME ZONE 'UTC') = '2026-02-05'
```text

---

## 10. Memory Leaks from Unclosed Subscriptions

### Gotcha: Subscription Connections Not Closed Properly

**Symptom:** Memory usage grows indefinitely in production.

**Why:** WebSocket connections held open but not properly closed on disconnect.

### Solutions

**Solution 1: Set subscription timeout**

```toml
[FraiseQL.subscriptions]
timeout_seconds = 3600  # Close connection after 1 hour
idle_timeout_seconds = 300  # Close if idle for 5 minutes
```text

**Solution 2: Explicit subscription cleanup**

```python
try:
    async for event in subscription:
        process_event(event)
finally:
    subscription.close()  # Always close
```text

**Solution 3: Monitor active subscriptions**

```bash
# Check for zombie subscriptions
SELECT COUNT(*) FROM pg_stat_activity
WHERE state = 'active' AND query LIKE '%subscription%'
```text

---

## 11. Race Conditions in Multi-Region

### Gotcha: Data Inconsistency Across Regions

**Symptom:** User sees different data depending on which region they connect to.

**Why:** Replication lag between regions. Write completes in US, but Asia region hasn't replicated yet.

### Solution

**Use strong consistency guarantees:**

```toml
[FraiseQL.federation]
consistency_level = "strong"  # Wait for all replicas
```text

**Or use regional routing:**

```python
# Route writes to primary region
# Route reads to local region (accept eventual consistency)
@FraiseQL.query
async def get_user(id: str, region: str = "primary"):
    db = db_connection(region)
    return await db.query("SELECT * FROM users WHERE id = ?", [id])
```text

---

## 12. Query Alias Shadowing

### Gotcha: Query Aliases Hiding Field Names

**Symptom:** Query returns unexpected fields or overwrites data.

**Example:**

```graphql
query {
  user: users(id: "123") {  # Alias "user" shadows field "users"
    id
    name
  }
}
```text

**Result:**

```json
{
  "user": {
    "id": "123",
    "name": "Alice"
  }
}
```text

**Later:**

```graphql
query {
  users(id: "123") {  # Different field name
    id
  }
}
```text

**Leads to confusion about response structure.**

### Solution

**Use aliases carefully and consistently:**

```graphql
query {
  activeUsers: users(status: "active") {
    id
    name
  }
  inactiveUsers: users(status: "inactive") {  # Clear alias
    id
    name
  }
}
```text

**Document expected response structure:**

```python
# Add comment to schema
@FraiseQL.query
def users(status: str = None):
    """
    Returns list of users, optionally filtered by status.

    Response structure:
    {
      "users": [
        {"id": "...", "name": "..."}
      ]
    }
    """
```text

---

## 13. Array Type Confusion (PostgreSQL vs Others)

### Gotcha: Array Operators Don't Work on MySQL

**Symptom:** Query with array operators fails on MySQL/SQLite.

**Example:**

```graphql
query {
  products(where: { tags: { contains: ["sale", "new"] } }) {  # Works on PostgreSQL, fails on MySQL!
    id
  }
}
```text

### Solution

**Check database support:**

```toml
[FraiseQL.validation]
array_operators_postgresql_only = true  # Warn if using array operators
```text

**Or store arrays as JSON:**

```python
@FraiseQL.type
class Product:
    id: ID
    tags: JSON  # Store as JSON, works everywhere
```text

---

## 14. Connection Pool Exhaustion

### Gotcha: All Connections Held by Slow Queries

**Symptom:** New queries fail with "no connections available".

**Why:** Slow query holds connection, preventing other queries from running.

### Solutions

**Solution 1: Set connection timeout**

```toml
[FraiseQL.database]
connection_timeout_seconds = 10
```text

**Solution 2: Implement query timeout**

```toml
[FraiseQL.database]
query_timeout_seconds = 30
```text

**Solution 3: Monitor connection pool**

```bash
# Check active connections
SELECT COUNT(*) FROM pg_stat_activity WHERE state = 'active'

# Kill slow queries
SELECT pg_terminate_backend(pid) FROM pg_stat_activity
WHERE query_start < now() - interval '5 minutes'
```text

---

## 15. Recursive Queries Without Limits

### Gotcha: Infinite Recursion in Self-Referential Queries

**Symptom:** Query hangs or times out.

**Example:**

```graphql
query {
  user(id: "1") {
    id
    manager {
      id
      manager {        # Recursion not bounded!
        id
        manager { ... }
      }
    }
  }
}
```text

### Solution

**Implement depth limits:**

```toml
[FraiseQL.validation]
max_query_depth = 15  # Prevent deep nesting
```text

**Or use explicit iteration:**

```graphql
query {
  user(id: "1") {
    id
    manager {
      id
      manager {
        id
        # Stop here (3 levels)
      }
    }
  }
}
```text

---

## See Also

**Related Guides:**

- **[Common Patterns](./PATTERNS.md)** — Real-world solutions avoiding gotchas
- **[Performance Tuning Runbook](../operations/performance-tuning-runbook.md)** — Optimizing query performance
- **[Testing Strategy](./testing-strategy.md)** — Testing to catch gotchas early
- **[Troubleshooting Decision Tree](./troubleshooting-decision-tree.md)** — Route to correct guide
- **[Consistency Model](./consistency-model.md)** — Understanding consistency guarantees

**Architecture & Design:**

- **[Execution Semantics](../architecture/core/execution-semantics.md)** — How queries execute
- **[Schema Design Best Practices](./schema-design-best-practices.md)** — Designing to avoid issues
- **[Federation Guide](../integrations/federation/guide.md)** — Federation pitfalls

**Operations:**

- **[Monitoring & Observability](./monitoring.md)** — Catching issues in production
- **[Observability Architecture](../operations/observability-architecture.md)** — Observing patterns

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
