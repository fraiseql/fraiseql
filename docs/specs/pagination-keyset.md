# Keyset Pagination Architecture (JSON + Arrow Planes)

**Version:** 1.0
**Date:** January 11, 2026
**Status:** Complete
**Audience:** Architects, Backend Engineers, BI Integration Teams

---

## 1. Overview

FraiseQL implements **keyset-based pagination** as the primary pagination strategy for both the JSON (GraphQL) and Arrow (analytics) planes. This avoids performance cliffs at scale (OFFSET doesn't scale >1M rows) while providing a unified cursor format across both planes.

### Design Goals

✅ **Unified interface** — Same pagination API for both JSON and Arrow planes
✅ **Relay compatibility** — Standard `edges`, `pageInfo`, `cursor` format
✅ **Scale-safe** — No OFFSET performance penalties at 10M+ rows
✅ **Stateless** — Cursors are self-contained; no server-side state needed
✅ **Deterministic** — Consistent ordering across queries and mutations
✅ **Type-safe** — Compile-time cursor validation
✅ **Backward compatible** — OFFSET/LIMIT still supported (with warnings)

---

## 2. Core Concept: Keyset Pagination

### Traditional Offset/Limit (Problem)

```sql
-- Query at OFFSET=10,000,000
SELECT id, name, email
FROM users
ORDER BY id
LIMIT 100 OFFSET 10000000;

-- Database must:
-- 1. Sort 20M rows by id
-- 2. Skip first 10M rows
-- 3. Return next 100 rows
-- Cost: O(n + offset) = O(20M) operations
```

**Result:** 2-5 second latency at high offsets. Unusable for analytics.

### Keyset Pagination (Solution)

```sql
-- Query with keyset cursor
SELECT id, name, email
FROM users
WHERE id > ?              -- ← Last row from previous page
ORDER BY id
LIMIT 100;

-- Database must:
-- 1. Use index on id to find starting point
-- 2. Read next 100 rows
-- 3. Return
-- Cost: O(limit) = O(100) operations
```

**Result:** <50ms latency regardless of position. Scales linearly.

### Why it Works

Keyset pagination replaces positional offsets with **value-based cursors**:

| Property | Offset/Limit | Keyset |
|---|---|---|
| Mechanism | Skip N rows | Find rows after cursor value |
| Performance | O(n) = O(20M) at offset 10M | O(1) with index = <50ms |
| Stateless | ✅ Yes | ✅ Yes |
| Resilient to mutations | ❌ No (rows shift) | ✅ Yes (cursor anchors to value) |
| Real-time updates | ❌ Skipped rows change | ✅ Cursor moves with value |

---

## 3. Cursor Format & Encoding

### Cursor Structure

A cursor encodes an **ordered key value** that uniquely identifies the last row returned:

```python
# Phase 1: Simple keyset (single column)
cursor = base64(encode({
    'id': 'user_12345'
}))

# Phase 2+: Composite keyset (multiple columns for stable ordering)
cursor = base64(encode({
    'created_at': '2025-01-11T15:30:00Z',
    'id': 'user_12345'
}))
```

### Self-Describing Format

Cursors are **self-describing** to support future evolution:

```json
// Internal representation (not exposed to clients)
{
  "version": 1,
  "projection": "user_analytics_v1",
  "keyset": {
    "created_at": "2025-01-11T15:30:00Z",
    "id": "user_12345"
  },
  "direction": "forward"
}

// Encoded as base64
eyJ2ZXJzaW9uIjogMSwgInByb2plY3Rpb24iOiAidXNlcl9hbmFseXRpY3NfdjEiLCAia2V5c2V0IjogeyJjcmVhdGVkX2F0IjogIjIwMjUtMDEtMTFUMTU6MzA6MDBaIiwgImlkIjogInVzZXJfMTIzNDUifSwgImRpcmVjdGlvbiI6ICJmb3J3YXJkIn0=
```

### Encoding Rules

✅ **Deterministic** — Same keyset always encodes to same cursor
✅ **Opaque** — Clients don't construct cursors; only FraiseQL generates them
✅ **Self-validating** — Include projection name to detect schema changes
✅ **Forward-compatible** — Version field allows future format changes

---

## 4. JSON Plane Pagination

### GraphQL Request (Relay Spec)

```graphql
query {
  users(first: 100, after: "eyJpZCI6ICJ1c2VyXzEwMDAifQ==") {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
      hasPreviousPage  # Optional, requires backward keyset
      startCursor      # Optional
    }
  }
}
```

### GraphQL Response

```json
{
  "data": {
    "users": {
      "edges": [
        {
          "node": {
            "id": "user_1001",
            "name": "Alice",
            "email": "alice@example.com"
          },
          "cursor": "eyJpZCI6ICJ1c2VyXzEwMDEifQ=="
        },
        {
          "node": {
            "id": "user_1002",
            "name": "Bob",
            "email": "bob@example.com"
          },
          "cursor": "eyJpZCI6ICJ1c2VyXzEwMDIifQ=="
        }
      ],
      "pageInfo": {
        "hasNextPage": true,
        "endCursor": "eyJpZCI6ICJ1c2VyXzEwMDIifQ==",
        "hasPreviousPage": false,
        "startCursor": "eyJpZCI6ICJ1c2VyXzEwMDEifQ=="
      }
    }
  }
}
```

### Compiled SQL

```sql
-- FraiseQL compiles the GraphQL query to:
SELECT
  pk_user, id, name, email,
  ROW_NUMBER() OVER (ORDER BY id) AS seq
FROM v_user
WHERE id > $1                    -- ← Keyset value from cursor
  AND deleted_at IS NULL
ORDER BY id
LIMIT 101;                       -- ← Fetch N+1 to detect endOfList

-- Parameters: [$1 = 'user_1000' (decoded from cursor)]
```

### Backward Pagination

For backward pagination (`last`, `before`):

```graphql
query {
  users(last: 100, before: "eyJpZCI6ICJ1c2VyXzEwMDAifQ==") {
    edges { node { id } cursor }
    pageInfo { hasPreviousPage startCursor }
  }
}
```

**Compiled SQL:**
```sql
-- Reverse keyset: find rows BEFORE cursor
SELECT ... FROM v_user
WHERE id < $1
ORDER BY id DESC
LIMIT 101;
-- Then reverse results to return in original order
```

**Note:** Backward pagination requires **secondary index** on the keyset columns in reverse order.

---

## 5. Arrow Plane Pagination

### Arrow Request (Keyset Format)

Arrow projections use **identical keyset pagination** as JSON plane:

```graphql
query {
  userAnalytics(first: 10000, after: "eyJjcmVhdGVkX2F0IjogIjIwMjUtMDEtMTEiLCAiaWQiOiAidXNlcl8xMDAwIn0=") {
    edges {
      node {
        # Arrow batches (not nested objects)
        users { id name email }
        orders { id user_id amount }
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

### Arrow Response

```json
{
  "data": {
    "userAnalytics": {
      "edges": [
        {
          "node": {
            "users": [
              { "id": "user_1001", "name": "Alice", "email": "alice@example.com" }
            ],
            "orders": [
              { "id": "order_5001", "user_id": "user_1001", "amount": 99.99 }
            ]
          },
          "cursor": "eyJjcmVhdGVkX2F0IjogIjIwMjUtMDEtMTEiLCAiaWQiOiAidXNlcl8xMDAxIn0="
        }
      ],
      "pageInfo": {
        "hasNextPage": true,
        "endCursor": "eyJjcmVhdGVkX2F0IjogIjIwMjUtMDEtMTEiLCAiaWQiOiAidXNlcl8xMDAxIn0="
      }
    }
  }
}
```

### Arrow via Binary Protocol

Arrow projections also support **direct binary pagination** (via HTTP Accept header):

```bash
# Request: Arrow IPC stream with cursor
GET /graphql \
  -H "Accept: application/x-arrow" \
  -d '{"query": "{ userAnalytics(first: 10000, after: \"...\") }"}'

# Response: Arrow IPC stream with metadata
```

**Arrow Stream Structure:**
```
[Schema Metadata] [Batch 1: users] [Batch 2: orders] [Footer with endCursor]
```

The `endCursor` is transmitted as Arrow metadata, not JSON.

---

## 6. Keyset Ordering: Choosing Columns

### Phase 1: Single-Column Keysets (v2.1)

Most queries use the **primary key** as the single keyset column:

```python
# Implicit keyset ordering
@fraiseql.type
class User:
    id: ID  # ← Keyset column (primary key)
    name: str
```

**Compiled keyset:**
```python
ORDER BY id
LIMIT 100
```

**Pros:**
- Simple, predictable
- Every table has a primary key
- Scales well

**Cons:**
- Order changes if primary key is UUID (non-monotonic)
- Insertion order not stable (new users appear randomly)

### Phase 2: Composite Keysets (v2.2+)

For deterministic, stable ordering, use **composite keysets**:

```python
@fraiseql.arrow_projection(
  name="user_analytics",
  keyset=["created_at", "id"]  # ← Composite keyset
)
class UserAnalytics:
    users: Arrow.Batch([...])
```

**Compiled keyset:**
```sql
WHERE (created_at, id) > (?, ?)
ORDER BY created_at, id
LIMIT 100
```

**Keyset Stability:**

| Scenario | Single ID | Composite (created_at, id) |
|---|---|---|
| **New rows inserted** | Changes pagination | Stable (new rows go to end) |
| **Rows deleted** | Stable (no older rows) | Stable (deletion doesn't shift) |
| **Primary key UUID** | Non-monotonic | Monotonic (creation time ordered) |
| **Real-time dashboard** | Updates shift visible rows | Consistent, predictable |

### Phase 3: Expression-Based Keysets (v2.3+)

Advanced use cases can define **custom keyset expressions**:

```python
@fraiseql.arrow_projection(
  name="high_value_orders",
  keyset={
    "expression": "LEAST(total_amount, 1000) DESC, id",
    "direction": "desc"  # High-value orders first
  }
)
```

**Keyset complexity:**
- Simple columns: No overhead
- Composite columns: Small overhead (<1% query time)
- Expression-based: May require index hints

---

## 7. Cursor Validation & Security

### Cursor Validation

Cursors are validated at **compile time and runtime**:

**Compile time:**
```python
# ❌ Error: Cursor references non-existent field
query = users(after: cursor)  # cursor built for old schema
```

**Runtime:**
```python
# Cursor decoded and validated
cursor = base64_decode("eyJ...")
assert cursor.version == 1
assert cursor.projection == "user_analytics_v1"
assert set(cursor.keyset.keys()) == {"created_at", "id"}
```

### Cursor Tampering

Cursors are **signed** to prevent tampering:

```python
# Cursor structure (actual storage)
{
  "version": 1,
  "projection": "user_analytics_v1",
  "keyset": {"created_at": "...", "id": "user_1001"},
  "hmac": "sha256(secret, json_dump)"  # ← Signature
}
```

**Validation:**
```python
# Verify HMAC before using cursor
if compute_hmac(cursor_data, secret) != cursor.hmac:
    raise CursorTamperedError()
```

### Cursor Expiration (Optional)

For sensitive queries, cursors can **expire**:

```python
@fraiseql.type
class SensitiveData:
    ... cursor_ttl = 300  # Seconds

# If cursor older than 5 minutes, client must restart
```

---

## 8. Handling Mutations During Pagination

### Problem: Rows Move During Pagination

```
Initial state:
users: [id=1, id=2, id=3, id=4, id=5]

Query 1: first=2  →  [id=1, id=2], cursor="id=2"

User deletes id=1 (rows shift):
users: [id=2, id=3, id=4, id=5]

Query 2: after="id=2"  →  Should be [id=3, id=4]
```

### Keyset Solution: Cursor "Holes"

Keyset pagination handles mutations gracefully:

```sql
-- Keyset query with deletion
WHERE id > 2          -- Last cursor was id=2
ORDER BY id
LIMIT 100

-- Results: [id=3, id=4, ...] ← Correct! Skipped deleted row.
```

**Why this works:**
- Cursor value (id=2) is immutable
- Query finds rows > that value
- If id=2 deleted, id=3 is now next row
- **No duplicates, no skips** (unless row is <2, already seen)

### Real-Time Mutations

For real-time dashboards, FraiseQL supports **event-driven cursor refresh**:

```python
# If mutation detected during pagination, emit warning
if rows_modified_since_cursor:
    emit_warning({
        "type": "pagination_stale",
        "suggestion": "Restart pagination from beginning"
    })
```

---

## 9. Index Requirements

### Keyset Index Structure

Efficient keyset pagination requires **indexes on keyset columns**:

```sql
-- For single-column keyset (Phase 1)
CREATE INDEX idx_user_id ON tb_user(id);

-- For composite keyset (Phase 2+)
CREATE INDEX idx_user_created_id ON tb_user(created_at, id);

-- For reverse pagination (backward keyset)
CREATE INDEX idx_user_created_id_desc ON tb_user(created_at DESC, id DESC);
```

**Index Planning:**

✅ **Required:** Index on keyset columns in order
⚠️ **Optional:** Covering index if WHERE filter uses other columns

**Example: With WHERE filter**

```sql
-- Query with both keyset and filter
SELECT ... FROM tb_user
WHERE
  (created_at, id) > (?, ?)      -- ← Keyset
  AND status = 'active'           -- ← Filter
ORDER BY created_at, id
LIMIT 100

-- Best index: (status, created_at, id)
CREATE INDEX idx_user_status_created_id
ON tb_user(status, created_at, id);
```

---

## 10. Performance Characteristics

### Latency Comparison

**Reference Deployment** (PostgreSQL 15, 1M rows):

| Pagination Type | Offset | Latency | Memory | Recommendation |
|---|---|---|---|---|
| **Keyset (id)** | Any | ~2-5ms | <1MB | ✅ Use for all cases |
| **Keyset (created_at, id)** | Any | ~5-10ms | <1MB | ✅ Use for stability |
| **OFFSET/LIMIT 100** | 0 | ~1-2ms | <1MB | Acceptable |
| **OFFSET/LIMIT 100** | 100K | ~50ms | <1MB | Discouraged |
| **OFFSET/LIMIT 100** | 10M | ~2-5s | <1MB | ❌ Unacceptable |

### Throughput

```
Sequential pagination through 1M rows:
- Keyset pagination: 1,000 pages × 5ms = 5 seconds ✅
- OFFSET/LIMIT: 1,000 pages × (2ms + offset) = 500+ seconds ❌
```

---

## 11. Implementation Phases

### Phase 1: Basic Keyset (v2.1)

**Timeline:** Weeks 1-2

**Deliverables:**
- Single-column keyset (primary key only)
- Relay pagination format
- Forward pagination only
- OFFSET/LIMIT deprecated (warnings emitted)

**SQL Pattern:**
```sql
SELECT ... FROM table
WHERE primary_key > ?
ORDER BY primary_key
LIMIT ?
```

### Phase 2: Composite Keyset (v2.2)

**Timeline:** Weeks 3-4

**Deliverables:**
- Multi-column composite keysets
- Stable ordering (created_at + id)
- Backward pagination support
- Arrow plane integration

**SQL Pattern:**
```sql
SELECT ... FROM table
WHERE (col1, col2) > (?, ?)
ORDER BY col1, col2
LIMIT ?
```

### Phase 3: Optimization (v2.3)

**Timeline:** Weeks 5-6

**Deliverables:**
- Expression-based keysets
- Query optimization hints
- Index auto-suggestion
- Performance monitoring

---

## 12. Migration Path: OFFSET → Keyset

### Backward Compatibility Window

**v2.1-v2.2:** OFFSET/LIMIT still works, but emits warnings

```
WARNING: Large OFFSET (100000) detected.
Keyset pagination is faster. See: docs/pagination-keyset.md
Current: SELECT ... LIMIT 100 OFFSET 100000;
Better:  SELECT ... WHERE id > ? LIMIT 100;
```

**v2.3+:** OFFSET/LIMIT available but not recommended for large offsets

### Client Migration Path

**Step 1: Adopt Relay pagination interface**
```graphql
# Old (offset-based)
query { users(skip: 1000, take: 100) { ... } }

# New (keyset-based)
query { users(first: 100, after: cursor) { ... } }
```

**Step 2: Update cursor handling**
```javascript
// Old
let offset = 1000;
const response = await fetch(url, { skip: offset });

// New
let cursor = null;
while (true) {
  const response = await fetch(url, { after: cursor });
  cursor = response.pageInfo.endCursor;
  if (!response.pageInfo.hasNextPage) break;
}
```

**Step 3: Remove pagination loops**
```javascript
// Old (manual offset increment)
for (let offset = 0; offset < total; offset += 100) {
  const page = await fetch(url, { skip: offset, take: 100 });
  process(page);
}

// New (cursor-based)
let cursor = null;
while (true) {
  const page = await fetch(url, { first: 100, after: cursor });
  process(page);
  if (!page.pageInfo.hasNextPage) break;
  cursor = page.pageInfo.endCursor;
}
```

---

## 13. FAQ

**Q: Are cursors URL-safe?**

A: Yes. Base64-encoded cursors are URL-safe and can be used in query strings.

**Q: What if I need both keyset and OFFSET?**

A: Use keyset pagination. If you must use OFFSET (for legacy systems), it's available but with latency warnings.

**Q: Can I use keyset pagination with aggregations?**

A: Yes, if you include the aggregation key in the keyset:

```sql
SELECT status, COUNT(*) as count
FROM users
GROUP BY status
ORDER BY status
LIMIT 100
```

**Q: How do I paginate through items with the same keyset value?**

A: Use a composite keyset with a tiebreaker:

```sql
-- All 10 "John" users have same created_at
WHERE (created_at, name, id) > (?, ?, ?)
```

**Q: What about cursor expiration?**

A: Optional. Cursors are stateless, so they don't expire by default. For security-sensitive queries, you can set `cursor_ttl`.

---

## 14. Related Documentation

- `arrow-plane.md` — Arrow projection pagination (Phase 2)
- `compiled-schema.md` — Cursor type definitions
- `database-targeting.md` — Index requirements per database

---

**Status: Complete** — Keyset pagination architecture documented for both JSON and Arrow planes.
