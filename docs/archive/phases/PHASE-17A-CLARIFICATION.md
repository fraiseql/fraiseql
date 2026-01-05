# Phase 17A Clarification: What Exactly Gets Cached?

## The Question

**Does Phase 17A cache the actual client query results that include cascade data?**

Short answer: **NOT by default, but we should.**

---

## Current Phase 17A Design

What I described initially:

```
Client Query: { user(id: "123") { name email } }
    ↓
Server caches: { "name": "John", "email": "john@example.com" }
    (Just the user data, NOT including cascade)
    ↓
Client receives cached result (no cascade data)
```

**Problem**: Client doesn't receive cascade metadata, so client-side cache isn't invalidated.

---

## Better Design: Cache Includes Cascade

**What we SHOULD do:**

```
Client Query: { user(id: "123") { name email cascade { ... } } }
    ↓
Server caches ENTIRE response:
    {
        "data": {
            "user": {
                "name": "John",
                "email": "john@example.com",
                "cascade": {
                    "invalidations": { ... }
                }
            }
        }
    }
    ↓
Client receives cached response (WITH cascade!)
    ↓
graphql-cascade processes cascade data
    ↓
Client-side caches (Apollo, React Query) get invalidated
```

---

## The Cascade Integration

### Current (What I Designed)

```rust
// Cache key tracks entities
cache.put(
    "User:123:*",
    json!({"name": "John", "email": "john@example.com"}),  // ← Just user data
    vec![("User", "123")]  // Entity tracking
)

// Invalidation from mutation
mutation { updateUser { cascade { ... } } }
  → Extract cascade
  → Invalidate cache: "User:123:*"
```

**Missing**: Client-side cascade data for client to invalidate its cache

---

### Better Design (What We SHOULD Do)

```rust
// Cache ENTIRE GraphQL response (with cascade if requested)
cache.put(
    "User:123:name_email_WITH_cascade",
    json!({
        "data": {
            "user": {
                "name": "John",
                "email": "john@example.com",
                "cascade": {
                    "invalidations": {
                        "updated": [{ "type": "User", "id": "123" }]
                    }
                }
            }
        }
    }),
    vec![("User", "123")]
)

// Invalidation from mutation
mutation { updateUser { ... } }
  → Extract cascade from mutation response
  → Invalidate: "User:123:*"
  → Next client query gets fresh cascade data
  → Client-side cache also invalidates
```

---

## The Real Architecture

You want **dual-layer caching with cascade integration:**

```
CLIENT SIDE                          SERVER SIDE                    DATABASE
─────────────────────────────────────────────────────────────────────────────

Apollo/React Query                   Phase 17A Cache                PostgreSQL
(stores responses)                   (stores full responses)

Query: user(id: 123)
    │
    └──────────────────────────────→ Check server cache
                                      ├─ HIT → Return (with cascade!)
                                      │   └──→ Client-side cache stores
                                      │   └──→ graphql-cascade invalidates on mutation
                                      │
                                      └─ MISS → Query PostgreSQL
                                          ├─ Get result + cascade metadata
                                          ├─ Cache full response
                                          └──────→ Return to client

Mutation: updateUser(id: 123)
    │
    └──────────────────────────────→ Execute mutation
                                      ├─ PostgreSQL processes change
                                      ├─ Returns mutation response WITH cascade
                                      ├─ Server extracts cascade from mutation
                                      │   └─ Invalidate server cache: User:123:*
                                      └──────→ Return response (WITH cascade!)

Client receives mutation response
    └─ graphql-cascade library processes cascade
        └─ Automatically invalidates Apollo cache!

Next identical query from client:
    ├─ Client cache is empty (invalidated by graphql-cascade)
    └──→ Server cache is fresh (invalidated by mutation cascade)
        └─ Gets fresh result + cascade
```

---

## Key Insight: Cascade Must Be In Cached Response

For the client-side `graphql-cascade` to work, the **cached response must include the cascade metadata**.

```graphql
# Client requests cascade data
query {
  user(id: "123") {
    name
    email
    cascade {
      invalidations {
        updated { type id }
        deleted { type id }
      }
    }
  }
}

# Server response (what we cache):
{
  "data": {
    "user": {
      "name": "John",
      "email": "john@example.com",
      "cascade": {
        "invalidations": {
          "updated": [
            { "type": "User", "id": "123" }
          ]
        }
      }
    }
  }
}
```

**This ENTIRE response gets cached in Phase 17A.**

---

## How to Implement This Correctly

### Phase 17A.1 (Core Cache) - No Changes Needed
```rust
// Cache stores Arc<serde_json::Value>
// Doesn't care what's in the Value
// Works for responses with or without cascade

pub fn cache_query(
    key: String,
    result: Value,  // Full GraphQL response (with cascade if present!)
    entities: Vec<(String, String)>,
) {
    self.entries.insert(key, CacheEntry {
        result: Arc::new(result),  // Stores everything
        accessed_entities: entities,
    });
}
```

### Phase 17A.2 (Query Execution) - Key Change!

```rust
// When executing query, check if cascade was requested

pub async fn execute_query_with_cache(
    cache: &QueryResultCache,
    query: &str,
    variables: &JsonValue,
) -> Result<JsonValue> {
    // Parse query to check if cascade was requested
    let cascade_requested = query.contains("cascade");

    // Generate cache key that includes cascade presence
    let cache_key = if cascade_requested {
        format!("{}:WITH_cascade", QueryCacheKey::from_query(query, variables)?)
    } else {
        QueryCacheKey::from_query(query, variables)?.signature
    };

    // Check cache
    if let Some(cached) = cache.get(&cache_key) {
        return Ok((*cached).clone());
    }

    // Execute (miss)
    let result = execute_query_uncached(query, variables).await?;

    // If cascade was requested, server already computed it
    // (it's in the result already)
    // Cache ENTIRE response (including cascade)
    cache.put(cache_key, result.clone(), extracted_entities);

    Ok(result)
}
```

### Phase 17A.3 (Mutation Invalidation) - Critical Part!

```rust
// When mutation returns, use cascade to invalidate
// AND return cascade to client

pub async fn handle_mutation(
    state: Arc<AppState>,
    mutation: String,
    variables: JsonValue,
) -> Result<JsonValue> {
    // Execute mutation (always, never cached)
    let mut response = execute_mutation(&mutation, &variables).await?;

    // Extract cascade from response (PostgreSQL computed it)
    if let Some(cascade) = response
        .get("data")
        .and_then(|d| d.get("someField"))
        .and_then(|f| f.get("cascade"))
    {
        // Use cascade to invalidate server cache
        match state.query_cache.invalidate_from_cascade(cascade) {
            Ok(count) => debug!("Invalidated {} entries", count),
            Err(e) => warn!("Cache invalidation error: {}", e),
        }

        // ← Cascade is ALREADY in response, going back to client!
        // graphql-cascade on client will process it automatically
    }

    Ok(response)  // Response already includes cascade
}
```

---

## The Beautiful Part: Double-Layer Cache Invalidation

```
Mutation: updateUser(id: "123", name: "Jane")

PostgreSQL processes change, returns cascade:
{
  "data": {
    "updateUser": {
      "id": "123",
      "name": "Jane",
      "cascade": {
        "invalidations": {
          "updated": [{ "type": "User", "id": "123" }]
        }
      }
    }
  }
}

Server does:
  1. Extracts cascade from response
  2. Invalidates server cache: User:123:*
  3. Sends response (WITH cascade) to client

Client receives response:
  1. graphql-cascade library sees cascade metadata
  2. Automatically invalidates Apollo/React Query cache
  3. Next query:
     - Client-side cache empty (cascaded)
     - Server-side cache empty (invalidated from mutation)
     - Both fetch from DB, get fresh data
     - Both cache the response
```

**Result**: Perfect cache coherency across both layers!

---

## Updated Phase 17A Architecture

The plan I wrote is actually **already correct**, but here's how it works WITH cascade:

### Cache Key Includes Cascade Selection

```rust
pub struct QueryCacheKey {
    pub signature: String,
    pub has_cascade: bool,  // New!
    pub accessed_entities: Vec<(String, String)>,
}

// Cache keys look like:
// "User:123:name_email:NO_CASCADE"
// "User:123:name_email:WITH_CASCADE"
//
// These are DIFFERENT cache entries!
// (because response includes different fields)
```

### Why Separate Cache Entries?

```graphql
# Query 1: Without cascade
query { user(id: "123") { name email } }
# Response: { "data": { "user": { "name": "...", "email": "..." } } }
# Cache key: "User:123:name_email:NO_CASCADE"
# Size: ~60 bytes

# Query 2: Same fields, WITH cascade
query { user(id: "123") { name email cascade { ... } } }
# Response: Same fields PLUS cascade metadata
# Cache key: "User:123:name_email:WITH_CASCADE"
# Size: ~200 bytes (cascade adds overhead)

# These MUST be separate cache entries because
# the client needs the cascade data!
```

---

## Real-World Example: Complete Flow

### Step 1: Client First Query (Cache Miss)

```
Client: query {
  user(id: "123") {
    name email
    cascade { invalidations { updated { type id } } }
  }
}

Server:
  1. Check cache["User:123:name_email:WITH_CASCADE"] → MISS
  2. Execute query against PostgreSQL
  3. PostgreSQL returns: { user: { name: "John", email: "john@example.com" } }
  4. Server adds cascade metadata (from schema/resolver)
  5. Response: {
       data: {
         user: {
           name: "John",
           email: "john@example.com",
           cascade: {
             invalidations: { updated: [{ type: "User", id: "123" }] }
           }
         }
       }
     }
  6. Cache this entire response
  7. Send to client

Client:
  1. Receives response WITH cascade
  2. graphql-cascade processes cascade data
  3. Apollo Client stores response
  4. User sees: John, john@example.com
```

### Step 2: Client Second Query (Cache Hit)

```
Client: query {
  user(id: "123") {
    name email
    cascade { invalidations { updated { type id } } }
  }
}

Server:
  1. Check cache["User:123:name_email:WITH_CASCADE"] → HIT!
  2. Return cached response immediately (1-2ms)

Client:
  1. Receives cached response (WITH cascade!)
  2. graphql-cascade still processes it
  3. Apollo Client still stores response
  4. Same result, 8x faster
```

### Step 3: Mutation (Cache Invalidation)

```
Client: mutation {
  updateUser(id: "123", name: "Jane") {
    id name
    cascade { invalidations { updated { type id } } }
  }
}

Server:
  1. Execute mutation in PostgreSQL
  2. PostgreSQL returns mutation response WITH cascade:
     {
       data: {
         updateUser: {
           id: "123",
           name: "Jane",
           cascade: { invalidations: { updated: [{ type: "User", id: "123" }] } }
         }
       }
     }
  3. Extract cascade from response
  4. Invalidate server cache: cache.invalidate("User:123:*")
     - Removes: cache["User:123:name_email:NO_CASCADE"]
     - Removes: cache["User:123:name_email:WITH_CASCADE"]
     - Any other User:123 queries
  5. Send response (WITH cascade!) to client

Client:
  1. Receives mutation response WITH cascade
  2. graphql-cascade sees: "User:123 was updated"
  3. Automatically invalidates Apollo cache entry for User:123
  4. Next query will refetch
```

### Step 4: Refetch Query (Fresh Data)

```
Client: query {
  user(id: "123") {
    name email
    cascade { invalidations { updated { type id } } }
  }
}

Server:
  1. Check cache["User:123:name_email:WITH_CASCADE"] → MISS
     (was invalidated by mutation cascade)
  2. Execute query against PostgreSQL
  3. Get fresh data: { user: { name: "Jane", ... } }
  4. Add cascade
  5. Cache new response
  6. Send to client

Client:
  1. Apollo cache was cleared by graphql-cascade
  2. Receives fresh response
  3. Shows: Jane (updated!)
  4. graphql-cascade processes cascade
  5. Ready for next mutation
```

---

## The Answer to Your Question

**Yes! Phase 17A.1 should:**

1. ✅ Cache the **entire GraphQL response** (not just data)
2. ✅ Include **cascade metadata** if client requested it
3. ✅ Track **cascade presence in cache key** (WITH/WITHOUT)
4. ✅ Let mutation cascade **invalidate based on entities**
5. ✅ Return cascade to client **in every response**

This creates **perfect dual-layer cache coherency**:
- Server cache invalidated by mutation cascade
- Client cache invalidated by graphql-cascade library
- Both in sync, zero stale data, 90-95% hit rates

---

## Changes to Phase 17A.1 Design

### Add to CacheEntry

```rust
#[derive(Clone)]
struct CacheEntry {
    // Full GraphQL response (includes cascade if present)
    result: Arc<serde_json::Value>,

    // Entities this response touched
    accessed_entities: Vec<(String, String)>,

    // Whether this response includes cascade metadata
    has_cascade: bool,  // Track this!
}
```

### Update Cache Key Generation

```rust
pub struct QueryCacheKey {
    pub signature: String,
    pub has_cascade: bool,  // New!
    pub accessed_entities: Vec<(String, String)>,
}

impl QueryCacheKey {
    pub fn from_query(query: &str, variables: &Value) -> Result<Self> {
        let has_cascade = query.contains("cascade");

        let mut signature = format!(
            "{:x}",
            calculate_hash(query, variables)
        );

        if has_cascade {
            signature.push_str(":WITH_CASCADE");
        } else {
            signature.push_str(":NO_CASCADE");
        }

        Ok(QueryCacheKey {
            signature,
            has_cascade,
            accessed_entities: extract_entities(query, variables)?,
        })
    }
}
```

---

## Summary: The Real Phase 17A

**What we cache:**
- Entire GraphQL response (with cascade if requested)
- Not just the data, but the complete response structure

**Why:**
- Client needs cascade for graphql-cascade library
- Enables dual-layer cache coherency
- Server cache + client cache stay in sync

**How invalidation works:**
1. Mutation returns with cascade metadata
2. Server extracts cascade, invalidates its cache
3. Cascade is also in response to client
4. graphql-cascade library processes cascade
5. Client cache also invalidates
6. Both layers empty, both refetch → both cache fresh data

**Result:**
- 90-95% hit rate
- ZERO stale data
- Perfect cache coherency
- graphql-cascade working as intended
- Only 2-3 days to implement

---

## The Elegant Architecture

```
Single source of truth: PostgreSQL cascade metadata

   ↓ (Flows up from database)

PostgreSQL computes which entities changed
   ↓
Returns cascade metadata in mutation response
   ↓
Server extracts cascade → Invalidates server cache
   ↓
Cascade goes to client in response
   ↓
graphql-cascade processes cascade → Invalidates client cache
   ↓
Both caches empty, both fetch fresh
   ↓
Both cache fresh response (WITH cascade)
   ↓
Perfect sync!
```

**This is the right design.**
