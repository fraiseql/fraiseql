# FraiseQL Anti-Patterns: What NOT to Do

**Date:** January 2026
**Status:** Complete System Specification
**Audience:** Developers, architects, technical leads

---

## Executive Summary

This document catalogs anti-patterns—designs that seem reasonable but lead to problems in practice. Learning what NOT to do is as important as learning what TO do.

Each anti-pattern includes:

- **Problem**: Why it's wrong
- **Symptoms**: How you'll know you're doing it
- **Solution**: Correct approach
- **Cost of ignoring**: Real consequences

---

## 1. Query & Mutation Anti-Patterns

### 1.1 Deep Nested Queries (N+1 Problem)

**Anti-pattern**: Design without considering query depth

```graphql
# ❌ WRONG: Dangerous nesting depth
query GetUserWithEverything {
  user(id: "user-1") {
    id
    name
    posts {                    # 1 JOIN
      id
      comments {              # 1 JOIN per post (N+1!)
        id
        author {              # 1 JOIN per comment (N+1+1!)
          id
          name
        }
      }
    }
  }
}
```

**Problem:**

- For 10 posts with 5 comments each = 50 database queries
- Exponential explosion with deeper nesting
- Can timeout or crash database

**Symptoms:**

- Queries timeout despite small result set
- Database CPU spikes with simple queries
- "Query too complex" errors

**Solution**: Limit query depth

```graphql
# ✅ CORRECT: Controlled nesting (2-3 levels max)
query GetUserWithPosts {
  user(id: "user-1") {
    id
    name
    posts(limit: 20) {
      id
      title
      commentCount  # Aggregated, not nested
    }
  }
}

# Separate query for comments if needed
query GetPostComments($postId: ID!) {
  post(id: $postId) {
    id
    comments(limit: 50) {
      id
      content
    }
  }
}
```

**Implementation**: Set max query depth at compile time

```python
@fraiseql.schema_rule(max_query_depth=3)
class MySchema:
    pass
```

---

### 1.2 Unbounded Result Sets

**Anti-pattern**: Queries without LIMIT

```graphql
# ❌ WRONG: No limit, returns all rows
query GetAllUsers {
  users {
    id
    name
    email
  }
}
```

**Problem:**

- Returns millions of rows
- Timeouts, memory exhaustion
- Network overload (10GB+ response)

**Symptoms:**

- Server runs out of memory
- Client connection hangs
- Database performance tanks

**Solution**: Always use pagination

```graphql
# ✅ CORRECT: Paginated with cursor
query GetUsers($first: Int!, $after: String) {
  users(first: $first, after: $after) {
    edges {
      cursor
      node { id name email }
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}
```

---

### 1.3 Synchronous Side Effects in Mutations

**Anti-pattern**: Block mutation completion on side effects

```python
# ❌ WRONG: Mutation waits for external service
@fraiseql.mutation
def create_order(input: OrderInput) -> Order:
    # Create order
    order = db.insert("orders", input)

    # Wait for email service (blocking)
    email_service.send_confirmation(order.id)  # 2 seconds!

    # Wait for analytics (blocking)
    analytics.log_event("order_created", order)  # 1 second!

    # Total: 3 seconds (should be 50ms)
    return order
```

**Problem:**

- Mutation latency includes side effect latency
- External service slowness delays user feedback
- Failed external services block entire operation

**Symptoms:**

- Mutations taking 1-5 seconds
- User sees spinning wheel
- Dependent systems failure crashes mutations

**Solution**: Make side effects async

```python
# ✅ CORRECT: Async side effects
@fraiseql.mutation
async def create_order(input: OrderInput) -> Order:
    # Create order (synchronous, fast)
    order = await db.insert("orders", input)

    # Schedule side effects (fire and forget)
    asyncio.create_task(
        email_service.send_confirmation(order.id)
    )
    asyncio.create_task(
        analytics.log_event("order_created", order)
    )

    # Return immediately
    return order

# Mutation latency: 50ms (as it should be)
```

---

## 2. Authorization Anti-Patterns

### 2.1 Authorization in Application Code (Not Schema)

**Anti-pattern**: Put authorization logic in business logic

```python
# ❌ WRONG: Authorization scattered in code
@fraiseql.query
def get_user(id: ID) -> User:
    user = db.query_one("SELECT * FROM tb_user WHERE id = $1", [id])

    # Check authorization in code
    if user.created_by_user_id != current_user_id:
        if not current_user.is_admin:
            raise PermissionError("Not authorized")

    return user

@fraiseql.mutation
def delete_user(id: ID) -> bool:
    # Different check in different place
    user = db.query_one("SELECT * FROM tb_user WHERE id = $1", [id])
    if user.id != current_user_id and not current_user.is_admin:
        raise PermissionError("Not authorized")

    db.delete("tb_user", id)
    return True
```

**Problem:**

- Authorization rules scattered everywhere
- Impossible to audit (where are all checks?)
- Easy to forget authorization (security hole)
- Hard to maintain (change rule = find all places)

**Symptoms:**

- Authorization inconsistency between operations
- Security audits find missing checks
- Accidental exposure of sensitive data

**Solution**: Declare authorization in schema

```python
# ✅ CORRECT: Authorization in schema (compile-time checked)
@fraiseql.type
@fraiseql.authorize(rule="owner_or_admin")
class User:
    id: ID
    email: str
    created_by_user_id: str

@fraiseql.query
@fraiseql.authorize(rule="owner_or_admin")
def get_user(id: ID) -> User:
    # Authorization already checked (compile-time)
    # Code is clean, authorization is auditable
    return db.query_one(..., [id])

@fraiseql.mutation
@fraiseql.authorize(rule="owner_or_admin")
def delete_user(id: ID) -> bool:
    # Same rule everywhere (consistent)
    db.delete("tb_user", id)
    return True
```

---

### 2.2 Trusting Client-Provided Authorization

**Anti-pattern**: Trust user role from GraphQL input

```python
# ❌ WRONG: Client provides their role (can be forged)
@fraiseql.query
def get_admin_panel() -> AdminPanel:
    # Check role from GraphQL input (client can lie!)
    if input.user_role == "admin":
        return admin_panel_data

    raise PermissionError()
```

**Problem:**

- Client can modify GraphQL to claim any role
- Security boundary is client-side (non-existent)
- Any user can become admin

**Symptoms:**

- Users access restricted data
- Audit shows unauthorized access
- Security breach

**Solution**: Derive authorization from verified token

```python
# ✅ CORRECT: Server derives role from verified token
@fraiseql.query
@fraiseql.authorize(rule="admin_only")
def get_admin_panel() -> AdminPanel:
    # Authorization derived from verified JWT
    # Client cannot forge role
    return admin_panel_data
```

---

## 3. Caching Anti-Patterns

### 3.1 Cache Without Invalidation

**Anti-pattern**: Cache but never invalidate

```python
# ❌ WRONG: Cache set to 1 hour, never invalidated
fraiseql.cache.set(
    f"product:{product_id}",
    product_data,
    ttl=3600  # 1 hour
)

# User updates product price
mutation UpdateProduct {
  updateProduct(id: "product-1", price: 150) {
    price
  }
}

# Cache not invalidated!
# Next query still sees old price (for up to 1 hour)
```

**Problem:**

- Stale data served to users
- Data inconsistency between instances
- User sees old data, confused

**Symptoms:**

- Users report stale data
- Updates not visible immediately
- Cache hits show old values

**Solution**: Invalidate on write

```python
# ✅ CORRECT: Invalidate on mutation
@fraiseql.mutation
async def update_product(id: ID, input: UpdateInput) -> Product:
    # Update database
    product = await db.update("products", id, input)

    # Invalidate related caches immediately
    cache.invalidate(f"product:{id}")
    cache.invalidate("featured_products")
    cache.invalidate(f"products_by_category:{product.category}")

    return product
```

---

### 3.2 Caching Sensitive Data

**Anti-pattern**: Cache PII without safeguards

```python
# ❌ WRONG: Cache user email (PII)
cache.set(f"user:{user_id}", user_data)  # Contains email!

# In multi-tenant system, another tenant might hit same cache
# if they guess the key (or keys are leaked in logs)
```

**Problem:**

- PII leakage between users
- Privacy violation, compliance issue
- Cannot be forgotten (cache persists)

**Symptoms:**

- Audit findings of PII in cache
- GDPR/privacy violations
- Users see other users' emails

**Solution**: Don't cache PII (or cache carefully)

```python
# ✅ CORRECT: Cache public data only
cache.set(f"user:{user_id}", {
    "id": user_id,
    "name": "Alice",
    "avatar_url": "https://...",
    # NO email, no phone, no sensitive data
})

# Sensitive data fetched separately (not cached)
# Or cached with very short TTL (30 seconds)
```

---

## 4. Performance Anti-Patterns

### 4.1 Premature Optimization

**Anti-pattern**: Optimize before measuring

```python
# ❌ WRONG: Complex optimization before profiling
@fraiseql.query
def get_users():
    # Manual batch loading (complex)
    batch_size = 1000
    users = []
    for i in range(0, total_users, batch_size):
        users.extend(
            db.query(f"SELECT * FROM users LIMIT {batch_size} OFFSET {i}")
        )
    return users

# Measurements show: This is SLOWER than simple query
# Because: Offset pagination is O(n), not O(1)
# Real fix: Use keyset pagination (1 query, not n queries)
```

**Problem:**

- Wasted effort on wrong bottleneck
- Complex code harder to maintain
- Solution might be slower than original

**Symptoms:**

- Optimization makes things slower
- Complexity increases without benefit
- Time wasted on wrong problems

**Solution**: Profile first, optimize based on data

```python
# ✅ CORRECT: Profile, identify bottleneck, optimize
# Profile shows: Database query taking 95% of time
# Root cause: Missing index on filter column
# Fix: Add index (simple, high impact)

CREATE INDEX idx_users_status ON tb_user(status);

# Speedup: 500ms → 50ms (10x faster, 1 line of SQL)
```

---

### 4.2 Over-Replication

**Anti-pattern**: Replicate data everywhere

```python
# ❌ WRONG: Replicate same data to 10 read replicas
Configuration:
  ├─ Primary database (writes)
  ├─ Read replica 1
  ├─ Read replica 2
  ├─ Read replica 3
  ├─ Read replica 4
  ├─ Read replica 5
  ├─ Read replica 6
  ├─ Read replica 7
  ├─ Read replica 8
  ├─ Read replica 9
  └─ Read replica 10

# Problem: Replication lag, storage bloat, complexity
```

**Problem:**

- Replication lag grows with replicas
- Storage cost multiplied
- Complexity in managing 11 instances
- Diminishing returns (beyond 3-5 replicas)

**Symptoms:**

- High replication lag (>10 seconds)
- Huge storage costs
- Difficult operational overhead

**Solution**: Right-size replica count

```python
# ✅ CORRECT: Minimal replicas for needs
Configuration:
  ├─ Primary database (writes): Handles mutations
  ├─ Read replica 1: Handles queries (local datacenter)
  ├─ Read replica 2: Handles Arrow analytics (separate)
  └─ Read replica 3: Disaster recovery (warm standby)

# Replication lag: <1 second
# Storage: Manageable
# Complexity: Operationally reasonable
```

---

## 5. Data Modeling Anti-Patterns

### 5.1 Storing Derived Data Without Updates

**Anti-pattern**: Calculate once, store forever

```python
# ❌ WRONG: Calculate user score, store, never update
user = {
    "id": "user-1",
    "name": "Alice",
    "score": 100  # Calculated once, never updated
}

# User does activities
# Score should update
# But it's stale in database

# Query assumes score is current (it's not!)
```

**Problem:**

- Stale derived data
- Inconsistency with calculated value
- Hard to know when last updated

**Symptoms:**

- User score doesn't match their activities
- Reports show wrong aggregates
- Users confused by stale data

**Solution**: Use database view or trigger

```python
# ✅ CORRECT: Calculate on-demand (view) or auto-update (trigger)

# Option 1: View (calculate on read)
CREATE VIEW v_user_stats AS
SELECT
    u.id,
    u.name,
    COUNT(a.id) as score
FROM tb_user u
LEFT JOIN tb_activity a ON u.id = a.user_id
GROUP BY u.id;

# Option 2: Trigger (update on write)
CREATE TRIGGER update_user_score
AFTER INSERT ON tb_activity
FOR EACH ROW
BEGIN
    UPDATE tb_user
    SET score = (SELECT COUNT(*) FROM tb_activity WHERE user_id = NEW.user_id)
    WHERE id = NEW.user_id;
END;
```

---

## 6. Concurrency Anti-Patterns

### 6.1 Optimistic Locking Without Version Check

**Anti-pattern**: Update without checking version

```python
# ❌ WRONG: Race condition possible
# Thread 1: Read user (version: 1)
user = db.query_one("SELECT * FROM tb_user WHERE id = $1", [user_id])

# Thread 2: Update user (version: 1 → 2)
db.update("tb_user", user_id, {"name": "Bob", "version": 2})

# Thread 1: Update without checking version
db.update("tb_user", user_id, {"email": "alice@example.com", "version": 1})

# Result: Race condition, version conflict
```

**Problem:**

- Lost updates (Thread 1 overwrites Thread 2)
- Data corruption
- No conflict detection

**Symptoms:**

- Users report updates being lost
- Data inconsistency
- Stale data overwrites newer data

**Solution**: Check version before update

```python
# ✅ CORRECT: Optimistic locking with version check
user = db.query_one(
    "SELECT id, name, email, version FROM tb_user WHERE id = $1",
    [user_id]
)

# Update with version check
result = db.execute(
    "UPDATE tb_user SET email = $1, version = version + 1 "
    "WHERE id = $2 AND version = $3",
    ["newemail@example.com", user_id, user.version]
)

if result.rowcount == 0:
    raise ConflictError("Version mismatch, refresh and retry")
```

---

## 7. Subscription Anti-Patterns

### 7.1 Subscribing to All Events

**Anti-pattern**: Get all events, filter on client

```graphql
# ❌ WRONG: Subscribe to ALL events, filter on client
subscription OnAllEvents {
  events {
    type
    timestamp
    data
  }
}

# Client filters
if (event.type === "order_created") {
  handleOrderCreated(event);
}
```

**Problem:**

- Network bloat (receiving events you don't care about)
- Buffer overflow (too many events)
- Wasted bandwidth

**Symptoms:**

- WebSocket connection drops (buffer full)
- Network usage very high
- Client CPU high (filtering all events)

**Solution**: Filter on server

```graphql
# ✅ CORRECT: Subscribe to specific events (filtered on server)
subscription OnOrderCreated {
  orderCreated {
    id
    total
    user { id name }
  }
}

# Server only sends order created events
# No buffer overflow
# Network efficient
```

---

### 7.2 No Heartbeat Handling

**Anti-pattern**: Assume subscription always connected

```python
# ❌ WRONG: No heartbeat, connection silently dies
subscription = await client.subscribe(query)

async for event in subscription:
    process_event(event)  # Dies silently if disconnected
```

**Problem:**

- Connection dies silently
- Client thinks still connected
- Missed events

**Symptoms:**

- Subscription appears active but receives no events
- User doesn't know they missed events
- Must manually refresh

**Solution**: Implement heartbeat/ping

```python
# ✅ CORRECT: Heartbeat keeps connection alive
subscription = await client.subscribe(query)

async def monitor_subscription():
    while subscription.is_active:
        if time.time() - subscription.last_message > 30:
            # No message in 30 seconds, send ping
            subscription.ping()
        await asyncio.sleep(10)

async for event in subscription:
    subscription.last_message = time.time()
    process_event(event)
```

---

## 8. Testing Anti-Patterns

### 8.1 Testing Without Authorization

**Anti-pattern**: Test queries/mutations but skip authorization

```python
# ❌ WRONG: Test query without checking authorization
def test_get_user():
    result = query(GetUserQuery, variables={"id": "user-1"})
    assert result.user.name == "Alice"
    # Missing: Authorization test!

def test_delete_user():
    result = mutation(DeleteUserMutation, variables={"id": "user-1"})
    assert result.success == True
    # Missing: Test that non-owner cannot delete!
```

**Problem:**

- Authorization bypassed in tests
- Security hole not caught
- Test passes but production fails

**Symptoms:**

- Tests pass but users report access denied
- Security audit finds authorization not tested
- Production bugs

**Solution**: Test authorization explicitly

```python
# ✅ CORRECT: Test with and without authorization
def test_get_user_authorized():
    # User can access their own user
    result = query(GetUserQuery, user_id="user-1", variables={"id": "user-1"})
    assert result.user.name == "Alice"

def test_get_user_unauthorized():
    # User cannot access other user
    result = query(GetUserQuery, user_id="user-1", variables={"id": "user-2"})
    assert result.errors[0].code == "E_AUTH_PERMISSION_401"

def test_delete_user_admin():
    # Admin can delete any user
    result = mutation(DeleteUserMutation, user_role="admin", variables={"id": "user-1"})
    assert result.success == True

def test_delete_user_non_owner():
    # Non-owner cannot delete
    result = mutation(DeleteUserMutation, user_id="user-1", variables={"id": "user-2"})
    assert result.errors[0].code == "E_AUTH_PERMISSION_401"
```

---

## 9. Deployment Anti-Patterns

### 9.1 Schema Mismatch Between Instances

**Anti-pattern**: Different instances using different schema versions

```
Production deployment:
├─ Instance 1: CompiledSchema v2.0.0
├─ Instance 2: CompiledSchema v2.0.0
├─ Instance 3: CompiledSchema v2.0.1 (oops!)
└─ Load balancer: Routes to all 3

Problem: Instance 3 has different behavior
```

**Problem:**

- Different query plans per instance
- Non-deterministic results
- Hard to debug (works on some instances, fails on others)

**Symptoms:**

- Some instances work, others fail
- Errors are random (instance-dependent)
- User sees different results on refresh

**Solution**: Atomic deployments

```
Correct deployment:

1. Deploy new code version
2. Wait for ready signal (health check)
3. Verify schema matches
4. Then route traffic

All instances have identical schema
All instances behave identically
```

---

## Summary: Anti-Pattern Checklist

Before shipping code, check:

- ❌ Deep query nesting? (max 3 levels)
- ❌ Unbounded result sets? (always paginate)
- ❌ Synchronous side effects? (make async)
- ❌ Authorization in code? (declare in schema)
- ❌ Trusting client input? (verify server-side)
- ❌ Cache without invalidation? (invalidate on write)
- ❌ Caching sensitive data? (don't)
- ❌ Optimizing before profiling? (profile first)
- ❌ Subscription to all events? (filter server-side)
- ❌ Tests without authorization? (test both cases)
- ❌ Schema mismatch across instances? (atomic deployments)

---

**Document Version**: 1.0.0
**Last Updated**: January 2026
**Status**: Complete specification for framework v2.x

Learn from others' mistakes. Avoid these patterns and your application will be safer and faster.
