# FraiseQL State Management: Caching, Change Data Capture, and State Model

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** System architects, cache engineers, CDC specialists, platform engineers

---

## Executive Summary

FraiseQL state management spans three concerns:

1. **Caching** — Query result caching for performance
2. **Change Data Capture (CDC)** — Event stream from database changes
3. **State Model** — How consistency is maintained across cache + live data

**Core principle**: Database is source of truth; cache is optimization layer; CDC drives everything else.

---

## 1. Caching Architecture

### 1.1 Cache Layers

```
                Application
                    ↓
    ┌───────────────────────────────┐
    │ L1 Cache (In-Memory)          │ <1ms access
    │ • Query results               │ • Per-instance
    │ • Size: 100MB-1GB             │ • Process-local
    └───────────────────────────────┘
                    ↓
    ┌───────────────────────────────┐
    │ L2 Cache (Redis/Memcached)    │ 1-5ms access
    │ • Shared across instances     │ • Cluster-wide
    │ • Size: 1GB-10GB              │ • Network latency
    └───────────────────────────────┘
                    ↓
    ┌───────────────────────────────┐
    │ L3 Cache (Database)           │ 10-50ms access
    │ • PostgreSQL views            │ • Permanent
    │ • Materialized views          │ • Slowest
    └───────────────────────────────┘
```

### 1.2 Cache Key Generation

Cache keys combine operation + context:

```python
# Query: GetUserPosts(userId: "user-456", limit: 20)
cache_key = {
    "operation": "GetUserPosts",
    "user_id": "user-456",
    "variables": { "limit": 20 },
    "version": "2.0.0"
}

# Hash to string
cache_key_string = hash(cache_key)
# "query:GetUserPosts:user-456:limit-20:v2"

# Search in L1/L2 cache
cached_result = cache.get(cache_key_string)
```

### 1.3 Cache TTL (Time To Live)

Different TTL per operation type:

```
Static data (Product catalog):
  ├─ TTL: 1 hour
  ├─ Reason: Changes infrequently
  └─ Miss cost: <1s (reload from DB)

User-specific data (My posts):
  ├─ TTL: 5 minutes
  ├─ Reason: User changes data frequently
  └─ Miss cost: <100ms

Real-time data (Stock prices):
  ├─ TTL: 10 seconds
  ├─ Reason: Changes constantly
  └─ Miss cost: <100ms

Personalized (User feed):
  ├─ TTL: 30 seconds
  ├─ Reason: User + algorithm dependent
  └─ Miss cost: <500ms
```

### 1.4 Cache Invalidation

Invalidation cascade on mutation:

```
Mutation: UpdatePost (post_id=789)
    ↓
1. Query database (write completes)
    ↓
2. Invalidate related queries:
    ├─ GetPost(id=789) → INVALIDATE
    ├─ GetPostsByAuthor(author_id=456) → INVALIDATE
    ├─ GetTrendingPosts() → INVALIDATE
    └─ GetUserFeed(user_id=456) → INVALIDATE
    ↓
3. Next requests:
    ├─ GetPost(id=789) → Cache miss → Query DB
    ├─ GetPostsByAuthor(author_id=456) → Cache miss → Query DB
    └─ Both re-cached for TTL period
```

### 1.5 Cache Warming

Pre-populate cache on startup:

```python
@fraiseql.on_startup
async def warm_cache():
    """Pre-populate cache with frequently accessed data"""
    await cache.preload([
        "GetTrendingPosts",
        "GetPopularUsers",
        "GetProductCatalog",
        "GetFeaturedBrands"
    ])

Result:
  ├─ Startup takes 2 seconds longer
  ├─ But 95%+ hit rate from moment 1
  ├─ Smoother user experience (no initial delays)
  └─ Reduced database load at launch
```

---

## 2. Change Data Capture (CDC)

### 2.1 CDC Architecture

```
Database (PostgreSQL)
    ├─ Trigger on INSERT/UPDATE/DELETE
    ├─ Publishes to LISTEN channel (pg_notify)
    ├─ Event: { type, entity_id, timestamp, payload }
    ↓
FraiseQL Runtime
    ├─ LISTEN on channels
    ├─ Receives events
    ├─ Processes authorization
    ├─ Publishes to subscribers
    ├─ Invalidates caches
    ↓
Subscribers & Caches
    ├─ Subscriptions (WebSocket)
    ├─ Webhooks (HTTP)
    ├─ Message queues (Kafka)
    ├─ Cache invalidation
```

### 2.2 CDC Events

All changes generate events:

```python
# Mutation: CreatePost
mutation CreatePost($title: String!) {
  createPost(input: { title: $title }) {
    id
    title
  }
}

# Database trigger generates:
Event {
    type: "post_created",
    entity_type: "Post",
    entity_id: "post-789",
    timestamp: "2026-01-15T10:30:45Z",
    payload: {
        id: "post-789",
        title: "New Post",
        author_id: "user-456",
        created_at: "2026-01-15T10:30:45Z"
    }
}

# Event triggers:
1. Subscriptions (OnPostCreated)
   ├─ Subscribers notified
   └─ Delta plane events sent
2. Cache invalidation
   ├─ GetTrendingPosts invalidated
   ├─ GetUserPosts(user_id=456) invalidated
3. Webhooks (if configured)
   ├─ POST https://external.com/webhooks/post_created
4. Event stream (Kafka, etc.)
   ├─ Published to post_created topic
```

### 2.3 CDC Event Ordering

**Per-entity ordering guarantee:**

```
Post #123 events (guaranteed ordered):
    1. post_created (T0)
    2. post_updated (T0 + 1s)
    3. post_updated (T0 + 2s)
    4. post_deleted (T0 + 3s)

    Subscribers see in order: 1 → 2 → 3 → 4

Different entity events (may reorder):
    Post #123: post_created (T0)
    User #456: user_updated (T0 + 0.5s)
    Post #123: post_updated (T0 + 1s)

    Subscribers may see:
    ├─ Post #123 created
    ├─ Post #123 updated (both for #123 ordered)
    ├─ User #456 updated (interleaved)

    OR:

    ├─ Post #123 created
    ├─ User #456 updated
    ├─ Post #123 updated (still ordered per entity)
```

### 2.4 CDC Event Deduplication

Events may be delivered multiple times (at-least-once):

```
Event: post_created (id: post-789)

Delivery 1: Client A receives
Delivery 2: Client B receives
Delivery 3: Retry (network error on first attempt)

Client must handle idempotency:
```python
@fraiseql.on_event("post_created")
async def handle_post_created(event):
    # Check if already processed
    if await db.query("SELECT id FROM tb_posts WHERE id = $1", event.entity_id):
        return  # Already processed

    # Process event
    await process_event(event)
```

---

## 3. State Model

### 3.1 CAP Theorem Trade-off

FraiseQL chooses **consistency and partition tolerance** (CP):

```
CAP: Choose 2 of 3
├─ Consistency (all nodes same data)
├─ Availability (always respond)
└─ Partition tolerance (handle network failures)

FraiseQL:
├─ Consistency: ✅ (SERIALIZABLE by default)
├─ Availability: ✅ (read replicas)
└─ Partition tolerance: ✅ (single database, but read replicas)

In practice:
  ├─ Same database: Full CP
  ├─ With replicas: Eventually consistent across replicas
  ├─ Causal consistency for federation
```

### 3.2 Consistency Guarantees

**Single database (SERIALIZABLE isolation):**

```
Query 1: SELECT * FROM users
  ├─ Sees: User A, User B, User C
  └─ Time: T0

Mutation: Create User D
  ├─ Executed: T0 + 1ms
  └─ Committed: T0 + 5ms

Query 2: SELECT * FROM users
  ├─ Sees: User A, User B, User C, User D
  └─ Time: T0 + 10ms

Guarantee: Every query sees consistent, up-to-date state
```

**With read replicas (eventual consistency):**

```
Primary database: Write all mutations
Read replicas (3 copies): Read-only

Mutation: Create User D
  ├─ Written to primary at T0
  ├─ Replicated to replica 1 at T0 + 10ms
  ├─ Replicated to replica 2 at T0 + 20ms
  ├─ Replicated to replica 3 at T0 + 30ms

Query from replica 1 at T0 + 15ms:
  ├─ Sees new user ✓ (already replicated)

Query from replica 3 at T0 + 15ms:
  ├─ Doesn't see new user ✗ (not yet replicated)

Result: Eventual consistency (typically <1 second)
```

### 3.3 Cache + Live Data Consistency

Challenge: Cache can be stale

```
Timeline:
T0:     GetPost(id=789) → Cache miss → Query DB
        ├─ Result: { title: "Original" }
        └─ Cached for 5 minutes

T0+1s:  UpdatePost(id=789, title="Updated")
        ├─ Database updated immediately
        └─ Cache invalidated

T0+2s:  GetPost(id=789) → Cache miss → Query DB
        ├─ Result: { title: "Updated" }
        └─ Cached for 5 minutes

Result: Cache always consistent (invalidated on write)
```

**Edge case: Network partition**

```
Normal:
  Mutation → Update database → Invalidate cache
  Next query: Sees new data

Network partition (Redis unreachable):
  Mutation → Update database → Cache invalidation FAILS
  Database updated: ✓
  Cache invalidation: ✗ (Redis down)
  Next query → Cache hit → Sees OLD data ✗

Recovery:
  ├─ Redis reconnected
  ├─ Cache still has stale data
  ├─ Run: cache.invalidate_all() (flush cache)
  ├─ Or: Wait for TTL expiration (5 minutes)
  ├─ Next query sees fresh data
```

---

## 4. State Consistency Patterns

### 4.1 Read-After-Write (RAW) Consistency

User sees their own writes immediately:

```python
# Cache key includes user_id
cache_key = f"GetUserPosts:{user_id}"

# Mutation by User A
@fraiseql.mutation
async def createPost(input):
    # Write to database
    post = await db.insert(...)

    # Invalidate ONLY User A's cache
    cache.invalidate(f"GetUserPosts:{user_id}")

    return post

# User A's next query
# Cache miss (just invalidated)
# Query database (sees their new post)
# ✓ User A sees their own writes
```

### 4.2 Causal Consistency (for federation)

Events propagate in order:

```
User A: Create Post #1 (T0)
    ├─ Primary database: Post created
    ├─ Event published: post_created
    ├─ Replicas start replication: T0 + 10ms

User B (on different subgraph): Query posts
    ├─ At T0 + 20ms (after replication)
    ├─ Sees Post #1 ✓
    ├─ Causal consistency maintained

User C (on different subgraph): Query posts
    ├─ At T0 + 5ms (before replication)
    ├─ Doesn't see Post #1 (not replicated yet)
    ├─ Re-queries at T0 + 25ms
    ├─ Sees Post #1 ✓
```

### 4.3 Eventual Consistency (multi-database)

Different databases eventually consistent:

```
Database A (Primary for users): User created
Database B (Replica): Gets replicated
Database C (Replica): Gets replicated

Timeline:
  T0: User created in Database A
  T0+10ms: Database B sees user
  T0+20ms: Database C sees user

Queries during replication:
  T0+5ms  from DB C: Doesn't see user (not replicated)
  T0+15ms from DB C: Sees user (replicated)

Consistency model: Eventual (typically <1s)
```

---

## 5. Advanced Caching Patterns

### 5.1 Cache-Aside Pattern

Check cache first, update on miss:

```python
async def get_user_posts(user_id):
    # Check L1 cache
    key = f"posts:{user_id}"
    cached = cache_l1.get(key)
    if cached:
        return cached

    # Check L2 cache
    cached = await cache_l2.get(key)
    if cached:
        cache_l1.set(key, cached)  # Promote to L1
        return cached

    # Miss in both → Query database
    result = await db.query(f"SELECT * FROM v_post WHERE user_id = {user_id}")

    # Populate both caches
    cache_l1.set(key, result, ttl=5*60)
    await cache_l2.set(key, result, ttl=1*60*60)

    return result
```

### 5.2 Write-Through Caching

Update cache on write:

```python
async def create_post(input):
    # Write to database
    post = await db.insert("tb_post", input)

    # Update cache immediately
    user_id = input.user_id
    key = f"posts:{user_id}"

    # Get current cache value
    cached_posts = cache.get(key) or []

    # Add new post
    cached_posts.append(post)

    # Write back to cache
    cache.set(key, cached_posts, ttl=5*60)

    return post
```

### 5.3 Write-Behind Caching

Update cache, then database (async):

```python
async def update_user_profile(user_id, data):
    # Update cache immediately
    cache.set(f"user:{user_id}", data)

    # Update database asynchronously
    asyncio.create_task(
        db.update("tb_user", user_id, data)
    )

    return data

# Client sees update immediately (from cache)
# Database eventually consistent (async write)
```

---

## 6. CDC Integration Patterns

### 6.1 Event-Driven Cache Invalidation

Events drive cache updates:

```python
@fraiseql.on_event("post_updated")
async def on_post_updated(event):
    """When post is updated, invalidate related caches"""

    # Get post details from event
    post_id = event.entity_id
    user_id = event.payload["user_id"]

    # Invalidate caches
    cache.invalidate(f"post:{post_id}")
    cache.invalidate(f"posts:{user_id}")
    cache.invalidate("trending_posts")
    cache.invalidate(f"user_feed:{user_id}")

@fraiseql.on_event("post_deleted")
async def on_post_deleted(event):
    # Similar invalidation
    ...
```

### 6.2 Event Sourcing

Use events as primary store (optional):

```python
# Traditional: Store state
POST v_post: { id, title, content, updated_at }

# Event sourcing: Store history
tb_post_events: [
    { type: "created", timestamp: T0, data: {...} },
    { type: "title_updated", timestamp: T1, data: {...} },
    { type: "content_updated", timestamp: T2, data: {...} }
]

Current state = Apply events from T0 to Tn
```

---

## 7. State Consistency Monitoring

### 7.1 Metrics

```
Cache metrics:
  ├─ Hit rate: Target >80%
  ├─ Invalidation rate: Monitor trends
  ├─ TTL distribution: Ensure balanced
  └─ Size: Monitor growth

Replication metrics:
  ├─ Replication lag: Target <1s
  ├─ Bytes replicated: Monitor bandwidth
  └─ Failed replications: Alert

CDC metrics:
  ├─ Event latency: Target <100ms
  ├─ Event ordering: Verify per-entity
  ├─ Delivery reliability: Target 99.99%
```

### 7.2 Health Checks

```
Cache health:
  ├─ Redis connectivity: UP/DOWN
  ├─ Memcached connectivity: UP/DOWN
  └─ Response time: <5ms

Replication health:
  ├─ Primary-replica lag: <1s?
  ├─ Bytes behind: <100MB?
  └─ Replication errors: 0?

CDC health:
  ├─ LISTEN channels: Connected?
  ├─ Event backlog: <100?
  └─ Trigger functions: Enabled?
```

---

## 8. State Management Configuration

### 8.1 Caching Configuration

```python
fraiseql.state.configure({
    "cache": {
        "l1": {
            "backend": "memory",
            "max_size_mb": 500,
            "eviction": "lru"  # Least recently used
        },
        "l2": {
            "backend": "redis",
            "url": "redis://localhost:6379",
            "max_size_mb": 5000
        },
        "default_ttl_seconds": 300,
        "warmup_on_startup": True
    },

    "invalidation": {
        "strategy": "cascade",  # Cascade invalidation
        "batch_size": 100,
        "delay_ms": 10
    }
})
```

### 8.2 CDC Configuration

```python
fraiseql.state.configure({
    "cdc": {
        "enabled": True,
        "database": "postgresql",
        "triggers": True,  # Use database triggers
        "listen_timeout_seconds": 300,
        "retry_policy": "exponential"
    },

    "replication": {
        "replicas": [
            "postgresql://replica1:5432/fraiseql",
            "postgresql://replica2:5432/fraiseql"
        ],
        "lag_threshold_ms": 1000
    }
})
```

---

## 9. Best Practices

### 9.1 Caching

**DO:**

- ✅ Monitor cache hit rate
- ✅ Adjust TTL based on access patterns
- ✅ Invalidate on all related mutations
- ✅ Use read replicas for analytics (no cache impact)
- ✅ Test cache behavior with failures

**DON'T:**

- ❌ Rely on cache for correctness (optional optimization)
- ❌ Cache without TTL expiration
- ❌ Ignore stale data during network partition
- ❌ Cache before authorization check (must check on every request)

### 9.2 CDC

**DO:**

- ✅ Process events idempotently (handle duplicates)
- ✅ Monitor event lag
- ✅ Test failure scenarios (database down, network partition)
- ✅ Use per-entity ordering guarantee

**DON'T:**

- ❌ Assume global event ordering
- ❌ Lose events (use durable event log)
- ❌ Block event processing (async handling)

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

FraiseQL state management optimizes performance (caching) while maintaining consistency (CDC + immutable source of truth).
