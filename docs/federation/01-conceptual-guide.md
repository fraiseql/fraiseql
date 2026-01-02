# Federation in FraiseQL: A Conceptual Guide

## Why Federation?

GraphQL federation solves a critical scaling problem: **How do large teams build a single GraphQL API without a monolithic server?**

### The Problem

As your system grows, you face three escalating challenges:

1. **N+1 Queries**: Resolving one entity requires fetching related entities from different services, creating cascading database queries.
2. **Service Ownership**: Multiple teams own different services (Users, Orders, Products). How do they define schema without stepping on each other?
3. **Schema Coordination**: When Services A and B want to reference each other's data, who owns the schema? Who breaks when changes happen?

### Federation's Answer

Federation **decouples schema ownership while maintaining a unified API**:
- Each service owns its entities
- Services reference each other without tight coupling
- A gateway merges all schemas into one
- The gateway handles cross-service resolution

## Why FraiseQL Federation?

FraiseQL's federation approach is different from Apollo Federation's default in three ways:

### 1. Progressive Complexity

FraiseQL federation comes in three tiers:

| Mode | Use Case | Users | Complexity |
|------|----------|-------|-----------|
| **LITE** | Single service or simple federation | 80% | Just `@entity` decorator |
| **STANDARD** | Type extensions and computed fields | 15% | Add `@extend_entity`, `external()`, `@requires` |
| **ADVANCED** | Complex multi-subgraph patterns | 5% | All 18 Apollo Federation directives |

**You start at LITE.** Add complexity only when needed.

### 2. Python-First Decorators

No separate schema syntax. Your Python types ARE your schema:

```python
@entity  # That's it. Auto-detects 'id' as key.
class User:
    id: str
    name: str
    email: str
```

Compare to Apollo Federation (SDL + resolver coordination):
```graphql
type User @key(fields: "id") {
  id: ID!
  name: String!
  email: String!
}
```

FraiseQL: **One truth, one language (Python).**

### 3. Request-Scoped Batching

FraiseQL **automatically batches entity resolution within a single request**:

- Without batching: Query for User(1) → Orders(user_id=1) → Products(ids=[...]) = 3 DB hits
- With FraiseQL: Same query, 1 batched DB query per entity type

This happens **transparently**. No configuration needed.

```python
# You write:
async def resolve_orders(user):
    return await loader.load("Order", "user_id", user.id)

# FraiseQL does:
# - Collects 50 such requests
# - Waits 1ms (batch window)
# - Executes one query: SELECT * FROM orders WHERE user_id IN (...)
# - Distributes results back to resolvers
```

## Core Concepts

### Entities

An **entity** is a type that can be resolved by key from your service.

```python
@entity
class Product:
    id: str        # ← Key (auto-detected)
    name: str
    price: float
```

**During federation:**
- Your service is responsible for resolving Products by ID
- Other services can reference Product by ID
- The gateway calls your `_entities` resolver with a list of Product IDs

### Keys

A **key** uniquely identifies an entity and tells federation how to look it up.

```python
# Auto-detected (80% case)
@entity
class User:
    id: str      # ← Detected as key

# Explicit single key
@entity(key="user_id")
class User:
    user_id: str
    name: str

# Composite key (for complex entities)
@entity(key=["org_id", "user_id"])
class OrgUser:
    org_id: str
    user_id: str
    permissions: list[str]
```

The gateway uses keys to:
1. Route resolution requests to the right service
2. Deduplicate entity requests
3. Cache results within a request

### Extensions

An **extension** adds fields to an entity defined in another service.

```python
# Service A defines User
# Service B wants to add user reviews

@extend_entity(key="id")
class User:
    id: str = external()        # ← From Service A
    name: str = external()      # ← From Service A

    reviews: list[Review]       # ← NEW field in Service B
    average_rating: float = field(init=False)

    @requires("reviews")
    async def compute_rating(self) -> float:
        return sum(r.rating for r in self.reviews) / len(self.reviews)
```

**Key rules:**
- Mark fields from other services with `external()`
- Add new fields without `external()`
- Use `@requires` to depend on external fields
- Extensions must match the parent's key

### Batching & Deduplication

**Batching** reduces database queries.

```
Request starts
  ↓
Resolver needs 50 Users by ID
  ↓
DataLoader.load("User", "id", "1")
DataLoader.load("User", "id", "2")
... (50 times)
  ↓
[Batch window (1ms) expires]
  ↓
SELECT * FROM users WHERE id IN (1, 2, ..., 50)
  ↓
All 50 futures resolve simultaneously
```

**Deduplication** eliminates redundant requests within the same batch window:

```python
loader.load("User", "id", "1")  # Future A created, added to batch
loader.load("User", "id", "1")  # Reuses Future A (dedup hit)
loader.load("User", "id", "1")  # Reuses Future A (dedup hit)

# Result: One query for User 1, three resolvers get the same result
```

**Caching** avoids repeated queries across requests (optional, based on configuration).

## What Happens During a Request?

### Step-by-Step

1. **HTTP request arrives** at your GraphQL server
2. **BatchExecutor created** per request (request-scoped)
3. **GraphQL resolver executes** `_entities` query
4. **Entity resolution**:
   - For each entity, call `loader.load(type, key_field, key_value)`
   - DataLoader adds to pending batch
   - Returns a Future (not yet resolved)
5. **Batch window expires** (default 1ms) or explicit flush
6. **Batch execution**:
   - Group requests by (type, key_field)
   - Execute one query per group
   - Cache results
   - Resolve all pending futures
7. **Results returned** to resolvers
8. **Request completes**, executor torn down

### Timing Example

```
t=0ms:    HTTP request arrives
t=0.1ms:  loader.load("User", "id", "1") → Future A
t=0.2ms:  loader.load("User", "id", "2") → Future B
t=0.3ms:  loader.load("Order", "id", "1") → Future C
t=0.5ms:  [Batch window expires at 1ms]
t=1ms:    Execute: SELECT * FROM users WHERE id IN (1, 2)
t=1.2ms:  Execute: SELECT * FROM orders WHERE id IN (1)
t=1.3ms:  Resolve futures A, B, C
t=1.5ms:  HTTP response sent
```

**Total**: ~1.5ms including batching, vs. 3ms without.

## What Isn't Solved (Explicitly!)

FraiseQL federation solves entity resolution. It does NOT solve:

### 1. Cross-Request Cache Coherence

Caching is **per-request**. If Service A caches a User, Service B won't see that cache.

```python
# Request 1: User(1) cached
# Request 2: User(1) queried again from DB

# Why? Each request has its own executor and cache.
```

**Solution**: Use a distributed cache (Redis) outside FraiseQL if needed.

### 2. Automatic Mutation Invalidation

FraiseQL doesn't know when you mutate data. If you update User(1), the cache won't invalidate.

```python
async def update_user(user_id: str, name: str):
    await db.execute("UPDATE users SET name = ? WHERE id = ?", name, user_id)
    # Cache for User(1) is still in memory if request is still running
    # Good: Request isolation
    # Bad: You must invalidate manually in multi-mutation scenarios
```

**Solution**: Call `loader.clear_cache()` after mutations, or use distributed cache.

### 3. Magic Resolution of Incompatible Schemas

If Service A says `User.id: String!` and Service B says `User.id: Int!`, federation fails.

```python
# Service A
@entity
class User:
    id: str  # String key

# Service B
@extend_entity(key="id")
class User:
    id: int  # ERROR: Type mismatch
```

**Solution**: Ensure all services agree on key types before deploying.

### 4. Automatic Query Cost Analysis

FraiseQL doesn't calculate query cost or prevent resource exhaustion.

```python
query {
  users(first: 1000000) {  # Too many users!
    orders {
      items {
        product {
          reviews {  # Nested resolver explosion
          }
        }
      }
    }
  }
}
```

**Solution**: Implement query depth/cost limits at the gateway level.

### 5. Cross-Subgraph Transactions

FraiseQL doesn't coordinate transactions across services.

**Solution**: Accept eventual consistency, or implement distributed transactions (complex).

---

## Next Steps

- **Quick start**: See [LITE Example](./examples/01-lite-single-service.md)
- **Multi-service**: See [STANDARD Example](./examples/02-standard-multi-subgraph.md)
- **Advanced patterns**: See [ADVANCED Example](./examples/03-advanced-composite-keys.md)
- **Performance tuning**: See [Performance Guide](./performance-tuning.md)
- **Debugging**: See [Error Handling](./error-handling.md)
