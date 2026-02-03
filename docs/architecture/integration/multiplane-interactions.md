# FraiseQL Multi-Plane Interactions: Cross-Plane Semantics and Composition

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Advanced developers, integration architects, framework contributors

---

## Executive Summary

FraiseQL's three execution planes (JSON, Arrow, Delta) operate from the same schema but are independent execution contexts. This document specifies how they interact, coordinate, and compose.

**Key insight**: Planes are orthogonal (independent) but consistent (same data). Understanding interactions is critical for advanced use cases.

---

## 1. Plane Overview

### 1.1 Three Planes

```
JSON Plane (Interactive)
├─ GraphQL queries/mutations
├─ Real-time requests
├─ Sub-100ms latency target
└─ One user at a time

Arrow Plane (Analytical)
├─ Batch queries
├─ Columnar format
├─ Multi-minute execution OK
└─ High throughput

Delta Plane (Streaming)
├─ Change data capture
├─ Subscriptions
├─ Real-time events
└─ Ordered by entity
```

### 1.2 Concurrent Execution

All three planes can execute simultaneously on same dataset:

```
Time T0:
  JSON: User.getProfile()
  Arrow: AnalyticsReport.run()
  Delta: SubscribeToChanges()

Time T1: Mutation creates new user
  JSON: CreateUser()
    ├─ Write to database
    └─ Completes in 20ms

Time T2 (immediately after T1):
  Arrow: See new user in next batch query
  Delta: Subscribers notified of new user
  JSON: New user available in next query
```

### 1.3 Data Consistency Across Planes

All planes see consistent view of data (snapshot isolation or SERIALIZABLE per database):

```
Database state at time T: {User[1], User[2], User[3]}

JSON Plane query at T+10ms: Sees User[1], User[2], User[3]
Arrow Plane batch at T+20ms: Sees User[1], User[2], User[3]
Delta Plane event at T+5ms: User[4] created

Result at T+30ms:
JSON Plane query now: Sees User[1..4]
Arrow Plane batch now: Sees User[1..4]
```

---

## 2. JSON Plane (Interactive)

### 2.1 Request-Response Model

JSON Plane operates as traditional GraphQL:

```
Client Request (GraphQL query)
    ↓
Server: Parse & validate
    ↓
Server: Check authorization
    ↓
Server: Execute query
    ↓
Server: Fetch from database
    ↓
Server: Serialize to JSON
    ↓
Response (JSON result)
    ↓
Client processes result
```

### 2.2 JSON Plane Execution Isolation

Each JSON request is independent:

```
Request 1 (User A, GetPosts):
  ├─ Transaction 1: SELECT FROM v_post WHERE user_id = A
  └─ Sees data as of T1

Request 2 (User B, GetPosts):
  ├─ Transaction 2: SELECT FROM v_post WHERE user_id = B
  └─ Sees data as of T2

Requests are concurrent but isolated:
  ├─ Each has own database transaction
  ├─ Each has own connection
  └─ Each sees snapshot-consistent data
```

### 2.3 Mutation Side Effects

Mutations in JSON Plane trigger:

1. **Database write** → Persisted immediately
2. **Delta event** → Triggers Delta Plane subscriptions
3. **Cache invalidation** → JSON Plane caches cleared
4. **Audit log** → Access recorded

---

## 3. Arrow Plane (Analytical)

### 3.1 Batch Query Model

Arrow Plane executes batch queries returning columnar Apache Arrow format:

```
Client Request (Arrow batch query)
  {
    "query": "SELECT * FROM v_product WHERE category = 'electronics'",
    "format": "arrow",
    "limit": 1000000
  }

    ↓

Server: Execute query
    ├─ May stream results (Arrow IPC format)
    └─ Returns columnar data

Response (Arrow RecordBatch)
  ├─ Columnar representation
  ├─ Compressed (by default)
  └─ Streaming chunks
```

### 3.2 Arrow Plane Execution Isolation

Arrow queries run in long-lived transactions (may take minutes):

```
Arrow Batch Query Start (T0):
  ├─ BEGIN TRANSACTION
  ├─ SET ISOLATION LEVEL SERIALIZABLE  (or READ_COMMITTED)
  └─ Hold snapshot until query ends

During Query Execution (T0 → T10 minutes):
  ├─ See consistent database snapshot from T0
  ├─ Other mutations happen → Not visible (isolated)
  └─ If conflicting mutation detected → Error or retry

Arrow Query Completes (T0 + 10 min):
  ├─ COMMIT TRANSACTION
  └─ Release snapshot

Meanwhile:
  ├─ JSON Plane queries execute → See mutations from T0+
  ├─ Delta Plane events generated → Per mutation
  └─ Database state evolved
```

### 3.3 Long Transaction Considerations

Arrow queries hold resources:

```
Connection pool:
  ├─ JSON queries: Rapid (release quickly)
  ├─ Arrow batch: Hold connection for 10 minutes
  └─ Resource planning: Account for long-lived connections

Database storage:
  ├─ MVCC versioning: Maintains snapshots for Arrow
  ├─ Old versions not garbage-collected → Disk impact
  └─ Disk space planning: Arrow queries impact storage

Lock contention:
  ├─ Long transactions hold locks longer
  ├─ Mutations may wait for Arrow to complete
  ├─ Can cause JSON Plane latency spikes
```

### 3.4 Arrow Plane Streaming

Results can stream incrementally (not wait for complete result):

```
Query: "SELECT * FROM huge_table" (1 billion rows)

Option 1: Wait for all (memory intensive)
  ├─ Buffer entire 10GB result set
  ├─ Return complete Arrow RecordBatch
  └─ Client processes all at once

Option 2: Stream (recommended)
  ├─ Collect 1000 rows (64KB Arrow chunk)
  ├─ Stream to client
  ├─ Next 1000 rows → stream
  └─ Client processes as data arrives

Streaming advantages:
  ├─ Constant memory usage (client)
  ├─ Can start processing before query finishes
  ├─ Better UX (progress visible)
  └─ Latency: First result in 100ms, complete in 1 hour
```

---

## 4. Delta Plane (Streaming)

### 4.1 Event Model

Delta Plane generates events from database changes:

```
User mutation in JSON Plane
  ├─ INSERT/UPDATE/DELETE in database
  ├─ Transaction commits
  ├─ Trigger fires
  └─ Event published

Event published in Delta Plane
  ├─ Type: "user_created", "post_updated", etc.
  ├─ Timestamp: Exact moment of database commit
  ├─ Payload: Changed entity data
  └─ Ordering: Per-entity (guaranteed)

Subscribers receive events
  ├─ Via WebSocket, webhook, or message queue
  ├─ In order (per entity)
  ├─ At-least-once delivery
```

### 4.2 Delta Events from JSON Mutations

When JSON Plane mutation completes:

```
Mutation execution:
  1. BEGIN TRANSACTION
  2. INSERT/UPDATE/DELETE
  3. COMMIT TRANSACTION
  4. Trigger fires → Event published
  5. Return mutation response

Event availability:
  ├─ Immediately after response sent (same millisecond)
  ├─ Available to Delta Plane subscribers
  ├─ Ordered with other events for same entity

Timing:
  ├─ Mutation: 20ms
  ├─ Event published: 21ms (inside database trigger)
  ├─ Delta subscribers notified: 25ms
  ├─ Total latency to event receipt: 5ms
```

### 4.3 Delta Events from Arrow Queries

Arrow queries (read-only) don't generate Delta events:

```
Arrow batch query:
  ├─ SELECT only (no modification)
  ├─ No triggers fire
  ├─ No Delta events generated
  └─ Subscribers see nothing

Exception: Arrow query includes mutation logic:
  ├─ INSERT into temp table (for analysis)
  ├─ Trigger fires → Event generated
  └─ But temp table events usually filtered
```

### 4.4 Delta Plane Ordering Guarantees

**Per-entity ordering (guaranteed):**

```
Sequence of events for Post #123:

Event 1: post_created (Post #123)
  ├─ Timestamp: T0
  └─ Subscriber 1 receives: Order #1

Event 2: post_updated (Post #123, title changed)
  ├─ Timestamp: T0 + 100ms
  └─ Subscriber 1 receives: Order #2 (after Event 1)

Event 3: post_updated (Post #123, content changed)
  ├─ Timestamp: T0 + 200ms
  └─ Subscriber 1 receives: Order #3 (after Event 2)

Guarantee: Subscriber always receives events in order for same post
```

**Global ordering NOT guaranteed:**

```
Event 1: post_created (Post #123) at T0
Event 2: user_updated (User #456) at T0 + 50ms
Event 3: post_updated (Post #123) at T0 + 100ms

Subscriber to "all events":
  May receive: Event 1 → Event 3 → Event 2 (reordered)
  OR: Event 1 → Event 2 → Event 3 (in order)

No guarantee on relative ordering of different entities
```

---

## 5. Cross-Plane Interactions

### 5.1 JSON Mutation → Delta Event → Arrow Batch

Complete flow:

```
Time T0: JSON Mutation (CreatePost)
  ├─ CreatePost(title="New", content="...")
  ├─ Database: INSERT INTO tb_post (...)
  └─ Returns PostCreatedPayload { id: "post-789", ... }

Time T0 + 1ms: Delta Event
  ├─ post_created event published
  ├─ PostCreatedEvent { id: "post-789", ... }
  ├─ Sent to subscribers
  └─ Available for Delta subscriptions

Time T0 + 100ms: Arrow Batch Query
  ├─ SELECT * FROM v_post WHERE created_at > NOW() - INTERVAL '1 hour'
  ├─ Sees new post created at T0
  ├─ Returns Arrow RecordBatch with new post
  └─ Query runs under SERIALIZABLE isolation

Result:
  ├─ JSON: Mutation confirmed post created
  ├─ Delta: Subscribers notified of new post
  ├─ Arrow: Batch reports sees new post
  ├─ All consistent (same data source: database)
```

### 5.2 Arrow Query Performance Impact on JSON Plane

Long-running Arrow queries can impact JSON Plane performance:

```
Scenario: Large analytics query (10 minutes)

Time T0: Arrow query starts
  ├─ SELECT * FROM v_product (1 billion rows)
  ├─ Holds database connection
  ├─ Holds MVCC snapshot
  └─ Holds locks (if any)

Time T0 + 5 seconds: JSON mutation attempt
  ├─ CreateProduct(name="...", price=100)
  ├─ Needs to update product indexes
  ├─ Arrow query still holding old snapshot
  └─ Possible conflicts or lock waits

Time T0 + 6 seconds: JSON mutation completes (delayed)
  ├─ Took 1 second longer than normal
  ├─ Normal: 20ms, With Arrow: 1020ms
  ├─ Latency spike visible to users
  └─ Delta events delayed

Result: Arrow query impacts JSON latency tail (p99, p99.9)
```

**Mitigation:**

```

1. Use lower isolation level for Arrow (READ_COMMITTED)
   ├─ Faster (fewer conflicts)
   ├─ Weaker consistency guarantee
   ├─ Trade-off acceptable for analytics

2. Monitor long-running queries
   ├─ Alert if Arrow query takes >5 minutes
   ├─ Check if killing query improves JSON latency

3. Scale Arrow separately
   ├─ Dedicated read replica for Arrow queries
   ├─ Primary database for JSON/Delta
   ├─ No cross-plane resource contention
```

### 5.3 Delta Events During Arrow Queries

Events continue to be generated while Arrow query runs:

```
Time T0: Arrow batch starts
  ├─ BEGIN TRANSACTION
  └─ SELECT ... (long-running)

Time T0 + 1 minute: JSON mutation (CreatePost)
  ├─ Commits in database
  ├─ Event published
  ├─ Delta subscribers notified
  └─ Arrow query still running

Arrow query finishes at T0 + 10 minutes:
  ├─ Doesn't see events from (T0 to T0+10min)
  ├─ Sees state as of T0 (immutable snapshot)
  ├─ Events available to Delta subscribers (real-time)
  └─ New Arrow query would see all changes
```

---

## 6. Authorization Consistency Across Planes

### 6.1 Same Authorization Rules

All planes evaluate same authorization rules:

```
Schema:
  @fraiseql.type
  class Post:
    @fraiseql.authorize(rule="published_or_author")
    content: str

User A (author of Post #123):
  ├─ JSON: GetPost #123 → Sees content ✓
  ├─ Arrow: SELECT * FROM v_post → Sees content for Post #123 ✓
  └─ Delta: Subscribe to Post #123 → Receives content field ✓

User B (not author):
  ├─ JSON: GetPost #123 → Sees content if published, else null
  ├─ Arrow: SELECT * FROM v_post → Sees content if published, else null
  └─ Delta: Subscribe to Post #123 → Receives content if published
```

### 6.2 Row-Level Security Across Planes

RLS rules filter results per user:

```
RLS rule: same_organization

User A (org=Acme):
  ├─ JSON: GetUsers → Only sees Acme users
  ├─ Arrow: SELECT * FROM v_user → Only sees Acme users
  └─ Delta: Subscribe to users → Only sees Acme user events

User B (org=Beta):
  ├─ JSON: GetUsers → Only sees Beta users
  ├─ Arrow: SELECT * FROM v_user → Only sees Beta users
  └─ Delta: Subscribe to users → Only sees Beta user events

Database has: [Acme User 1, Acme User 2, Beta User 1, Beta User 2]

Query results vary by user (RLS enforced in all planes)
```

---

## 7. Caching Across Planes

### 7.1 Plane-Specific Caching

Each plane has independent cache:

```
JSON Plane Cache:
  ├─ Cached query results (GraphQL)
  ├─ TTL: 5 minutes default
  ├─ Key: {operation_name, variables, user_id}
  └─ Hit rate: 80%+

Arrow Plane Cache:
  ├─ Cached batch results (Arrow)
  ├─ TTL: 60 minutes (longer, less frequent)
  ├─ Key: {query_hash, user_id, result_set_parameters}
  └─ Hit rate: 40-60% (more varied queries)

Delta Plane Cache:
  ├─ Event buffer (not traditional cache)
  ├─ Capacity: 1000 events per subscription
  ├─ FIFO eviction (oldest dropped if overflow)
  └─ Used for: Catch-up when client reconnects
```

### 7.2 Cache Invalidation Coordination

When data changes, all plane caches affected:

```
Time T0: JSON mutation (UpdatePost #789)
  ├─ Update title
  └─ Commit to database

Time T0 + 1ms: Cache invalidation cascade
  ├─ JSON cache: Invalidate all queries with Post #789
  ├─ Arrow cache: Invalidate batches that include Post #789
  ├─ Delta cache: Include UpdatePost event in event buffer
  └─ Next queries see fresh data

Time T0 + 100ms: First subsequent JSON query
  ├─ JSON cache miss (invalidated)
  ├─ Query database
  ├─ Sees updated post
  ├─ Re-caches for 5 minutes
```

---

## 8. Subscription Composition (Delta Plane Usage)

### 8.1 Simple Subscription

Delta Plane subscription to single type:

```graphql
subscription OnPostCreated {
  postCreated {
    id
    title
    authorId
  }
}
```

**Events received:**

```
Event 1: Post #1 created
  { id: "post-1", title: "First", authorId: "user-123" }

Event 2: Post #2 created
  { id: "post-2", title: "Second", authorId: "user-456" }

...
```

### 8.2 Nested Subscription

Subscription with nested relationships:

```graphql
subscription OnPostCreatedWithAuthor {
  postCreated {
    id
    title
    author {
      id
      name
    }
  }
}
```

**Execution:**

```
Database event: Post created
  ├─ Event payload: { id: "post-789", author_id: "user-456" }
  ├─ Runtime fetches author (authorization check)
  ├─ Author: { id: "user-456", name: "Alice" }
  └─ Send to subscriber: Full nested payload
```

### 8.3 Filtered Subscription

Subscription with WHERE clause:

```graphql
subscription OnMyPostsCreated($userId: ID!) {
  postCreated(where: { authorId: $userId }) {
    id
    title
  }
}
```

**Filtering:**

```
Event: Post created by user-456
  ├─ Check filter: authorId == $userId
  ├─ If match → Send to subscriber
  ├─ If no match → Drop (don't send)

Authorization:
  ├─ User can only subscribe to own posts (owner_only)
  ├─ If filter == current user → Allowed
  ├─ If filter != current user → Denied
```

---

## 9. Error Handling Across Planes

### 9.1 Transient Errors (Retry-able)

```
JSON Plane:
  ├─ Query timeout → Error code E_DB_QUERY_TIMEOUT_302
  ├─ Connection lost → Error code E_DB_CONNECTION_FAILED_301
  ├─ Deadlock → Retry automatically (up to 3 times)

Arrow Plane:
  ├─ Query timeout → Error code E_DB_QUERY_TIMEOUT_302
  ├─ Client disconnection → End batch query
  ├─ Deadlock (rare) → Retry from scratch

Delta Plane:
  ├─ Event delivery failed → Retry up to 3 times
  ├─ Connection lost → Reconnect
  ├─ Buffer overflow → Terminate subscription
```

### 9.2 Authorization Errors (Non-Retryable)

```
JSON Plane:
  ├─ User not authorized → E_AUTH_PERMISSION_401
  ├─ Response: { data: null, error: {...} }

Arrow Plane:
  ├─ User not authorized → E_AUTH_PERMISSION_401
  ├─ Query aborted before execution
  ├─ No partial results

Delta Plane:
  ├─ User not authorized → E_AUTH_PERMISSION_401
  ├─ Subscription denied
  ├─ No events sent
```

---

## 10. Transactions Across Planes

### 10.1 Transaction Isolation

Each plane transaction is independent:

```
Time T0: Arrow query begins
  ├─ BEGIN TRANSACTION (isolation: SERIALIZABLE)
  └─ SELECT * FROM v_product (starts)

Time T0 + 1s: JSON mutation committed
  ├─ Mutation: UpdateProduct price
  ├─ Transaction commits
  ├─ Data persisted
  ├─ Delta event published
  └─ Cache invalidated

Arrow query (still running):
  ├─ Sees old product price (snapshot at T0)
  ├─ Doesn't see update from JSON mutation
  ├─ Consistent snapshot maintained
  ├─ When Arrow finishes, new query sees update
```

### 10.2 No Cross-Plane Transactions

Transactions do NOT span planes:

```
❌ INVALID: Multi-plane transaction
  ├─ Create user (JSON mutation)
  ├─ Immediately run analytics (Arrow query)
  ├─ Cannot guarantee atomicity across

✅ VALID: Single-plane transaction
  ├─ Multiple JSON mutations → Atomic
  ├─ Single Arrow query → Single transaction
  ├─ Atomic within plane
```

---

## 11. Best Practices

### 11.1 Plane Selection Guide

```
Use JSON Plane when:
  ├─ Interactive requests (<100ms latency required)
  ├─ Single user per request
  ├─ Update operations needed
  ├─ Most application requests

Use Arrow Plane when:
  ├─ Batch analytics (OK with multi-minute latency)
  ├─ Large result sets (>1M rows)
  ├─ Columnar format needed
  ├─ Need to minimize JSON serialization overhead

Use Delta Plane when:
  ├─ Real-time notifications needed
  ├─ Event-driven architecture
  ├─ Broadcast to many clients
  ├─ Subscription patterns (WebSocket, webhooks)
```

### 11.2 Avoiding Cross-Plane Issues

```
✅ DO:
  ├─ Design for single plane primary access
  ├─ Use read replicas for Arrow (avoid impacting JSON)
  ├─ Monitor long-running Arrow queries
  ├─ Test authorization in all planes
  ├─ Use consistent cache invalidation

❌ DON'T:
  ├─ Assume Arrow query sees latest JSON mutation immediately
  ├─ Try to join JSON and Arrow results client-side (race conditions)
  ├─ Rely on global event ordering (use per-entity ordering)
  ├─ Run unlimited Arrow queries (resource contention)
  ├─ Ignore authorization differences between planes (none, but verify)
```

---

## 12. Performance Considerations

### 12.1 Peak Performance Scenarios

```
JSON Plane:
  ├─ 5,000 req/sec (simple queries)
  ├─ 1,000 req/sec (complex queries)
  └─ p95 latency: <200ms

Arrow Plane:
  ├─ 1 concurrent batch query (safe)
  ├─ 10 concurrent (resource intensive)
  ├─ Throughput: 1M rows/sec
  └─ Latency: Per-minute scale

Delta Plane:
  ├─ 10,000 events/sec
  ├─ 1000 active subscriptions
  └─ Per-event latency: <100ms
```

### 11.2 Resource Contention Scenarios

```
Scenario: Arrow query on 1GB table + Peak JSON traffic

Result:
  ├─ JSON p95 latency: 50ms → 500ms (10x slower)
  ├─ Arrow query: 5 minutes → 10 minutes (2x slower)
  ├─ Both degraded (shared database resource)

Solution:
  ├─ Route Arrow to read replica (separate hardware)
  ├─ JSON continues at normal performance
  ├─ Arrow takes 5 minutes (as expected)
  ├─ No cross-plane impact
```

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL's three planes are independent but coordinated. Understanding their interactions is essential for optimal performance and correct composition.
