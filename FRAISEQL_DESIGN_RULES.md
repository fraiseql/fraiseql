# FraiseQL-Specific Design Rules

## Philosophy

FraiseQL's design rules are **calibrated to the compilation model**, not generic GraphQL best practices.

- **What FraiseQL Solves Automatically**: n+1 (via JSONB views), query optimization (via compilation), deterministic execution
- **What Requires Architectural Guidance**: How to structure schemas so they compile to optimal JSONB queries

## Core FraiseQL Concepts

### JSONB View Batching
Instead of Apollo Federation's entity resolution, FraiseQL batches related entities into JSONB views at query time.

```sql
-- FraiseQL compiles to:
SELECT users.id, users.name,
  JSONB_AGG(JSONB_BUILD_OBJECT('id', posts.id, 'title', posts.title))
    FILTER (WHERE posts.id IS NOT NULL) AS posts
FROM users
LEFT JOIN posts ON users.id = posts.user_id
GROUP BY users.id
```

### Deterministic Compilation
All queries compile to deterministic SQL plans. No runtime decisions.

```graphql
query GetUserWithPosts($id: ID!) {
  user(id: $id) {           # Compiles to SELECT WHERE id = $1
    id name
    posts(limit: 10) {      # Compiles to LIMIT 10 (no runtime pagination logic)
      id title
    }
  }
}
```

### Subgraph Boundaries = Data Access Boundaries
Subgraphs in FraiseQL don't represent "services" but rather **authorization/data access boundaries**. Cross-subgraph access requires:
1. Auth checks to pass
2. Data fetching to be explicit
3. JSONB view construction to be optimizable

## FraiseQL-Specific Rules

### 1. Federation Rules (JSONB Batching Alignment)

#### ❌ ANTI-PATTERN: Entity in 3+ Subgraphs
**Why it's a problem in FraiseQL:**
- Each subgraph that has the entity = separate JSONB view construction
- Can't batch fetch efficiently across 3+ JSONB views
- Requires multiple round-trips or manual joins

**Example:**
```json
{
  "subgraphs": [
    {"name": "users", "entities": ["User"]},
    {"name": "posts", "entities": ["User", "Post"]},  // User duplicate!
    {"name": "comments", "entities": ["User", "Comment"]}  // User duplicate!
  ]
}
```

**FraiseQL Fix:**
```json
{
  "subgraphs": [
    {"name": "users", "entities": ["User"], "primary": true},  // Primary
    {"name": "posts", "entities": ["Post"],
     "references": [{"type": "User", "via": "users"}]},        // Reference only
    {"name": "comments", "entities": ["Comment"],
     "references": [{"type": "User", "via": "users"}]}         // Reference only
  ]
}
```

#### ❌ ANTI-PATTERN: Circular JSONB Chains
**Why it's a problem in FraiseQL:**
- A → B → A reference chain requires nested JSONB aggregations
- Nested JSONB becomes inefficient fast
- Can't compile to deterministic SQL plan efficiently

**Example:**
```
users-service:
  User -> references posts-service.Post via user_id
posts-service:
  Post -> references users-service.User via author_id
```

This forces: `SELECT users { posts { author { posts { ... } } } }`

**FraiseQL Fix:** Break the cycle by making one reference direction explicit:
```
users-service:
  User { posts: [Post] }  // Forward reference (batched via JSONB)
posts-service:
  Post { id, title, user_id }  // No back-reference; use explicit query if needed
```

#### ❌ ANTI-PATTERN: Missing Entity Type Metadata
**Why it's a problem in FraiseQL:**
- Compiler needs type information to construct optimal JSONB structure
- Missing metadata = compiler must make conservative choices = suboptimal SQL

**FraiseQL Needs:**
- Clear type definitions (can't infer from untyped JSON)
- Primary key designation (for JSONB aggregation join keys)
- Cardinality hints (one-to-many vs many-to-many)
- Index recommendations (for efficient JSONB construction)

**Example:**
```json
{
  "entities": [
    {
      "name": "User",
      "fields": [
        {"name": "id", "type": "ID", "isPrimaryKey": true},
        {"name": "posts", "type": "[Post!]", "cardinality": "one-to-many"}
      ]
    }
  ]
}
```

---

### 2. Cost Rules (Compilation Determinism)

#### ❌ ANTI-PATTERN: Query Depth Without Deterministic Cost
**Why it's a problem in FraiseQL:**
- FraiseQL compiles to deterministic SQL, so cost must be predictable at compile time
- Query depth that varies at runtime = can't pre-compute cost
- Deep nesting in JSONB views = exponential SQL complexity

**Example (PROBLEMATIC):**
```graphql
query {
  users {                    # Cost: N rows
    posts {                  # Cost: M per user = N*M
      comments {            # Cost: P per post = N*M*P
        author {            # Cost: Q per comment = N*M*P*Q
          posts {          # EXPONENTIAL - can't compile efficiently!
            comments { ... }
          }
        }
      }
    }
  }
}
```

**FraiseQL Fix:** Add depth limits that can be compiled:
```graphql
query {
  users(limit: 100) {                    # Limit: 100
    posts(limit: 10) {                   # Cost: 100*10 = 1000 JSONB rows
      comments(limit: 5) {               # Cost: 1000*5 = 5000 in JSONB
        author { ... }                   # Can't nest further
      }
    }
  }
}
```

#### ❌ ANTI-PATTERN: Unbounded Pagination Fields
**Why it's a problem in FraiseQL:**
- Compiler can't pre-compute worst-case cost without default limits
- Runtime pagination decisions break deterministic compilation

**FraiseQL Fix:**
```json
{
  "name": "posts",
  "type": "[Post!]",
  "args": [{"name": "limit", "type": "Int"}],
  "defaultLimit": 50,      // REQUIRED: allows compile-time cost calculation
  "maxLimit": 1000
}
```

#### ❌ ANTI-PATTERN: Field Multipliers (O(n²) in JSONB)
**Why it's a problem in FraiseQL:**
- Lists within lists in JSONB views explode cardinality
- `users.posts[].comments[]` = O(n²) JSONB structure
- Compiler can't generate efficient SQL for this

**Example:**
```graphql
users {
  id
  posts {                    # JSONB array of M posts
    id
    comments {              # JSONB array of P comments per post - O(n²)!
      id
    }
  }
}
```

**FraiseQL Fix:** Use pagination or separate queries:
```graphql
# Option 1: Paginate inner collection
users {
  id
  posts(limit: 5) {         # Max 5 posts
    id
    comments(limit: 3) {    # Max 3 comments - O(1) complexity
      id
    }
  }
}

# Option 2: Separate query for comments
query {
  users(limit: 100) { posts(limit: 5) { id } }
}
# Then separately:
query {
  comments(postIds: [...]) { ... }
}
```

---

### 3. Cache Rules (JSONB Coherency)

#### ❌ ANTI-PATTERN: TTL Mismatches in Batched JSONB
**Why it's a problem in FraiseQL:**
- User cached 5min in users-service, 30min in posts-service JSONB
- When constructing posts JSONB with user data, which TTL wins?
- Incoherent caching = stale data or unnecessary invalidations

**FraiseQL Fix:** Synchronize TTLs across all subgraphs that co-fetch:
```json
{
  "subgraphs": [
    {"name": "users", "entities": [{"name": "User", "cacheTtlSeconds": 300}]},
    {"name": "posts", "entities": [
      {"name": "Post", "cacheTtlSeconds": 600},
      {"name": "User", "cacheTtlSeconds": 300}  // SYNC with primary
    ]}
  ]
}
```

#### ❌ ANTI-PATTERN: Missing @cache on Expensive JSONB Construction
**Why it's a problem in FraiseQL:**
- Expensive JSONB aggregations should be cached
- Without cache directive, compiler assumes every query reconstructs the JSONB
- Defeats the "pre-computed JSONB" optimization

**FraiseQL Fix:**
```graphql
type User {
  id: ID!
  name: String!
  posts: [Post!]! @cache(ttlSeconds: 300)  # Cache the JSONB aggregate
}
```

---

### 4. Authorization Rules (Auth at JSONB Time)

#### ❌ ANTI-PATTERN: Auth Boundary Leak in JSONB Construction
**Why it's a problem in FraiseQL:**
- User.email requires auth scope "user:profile"
- Comments subgraph constructs JSONB with user data, but doesn't check auth
- User email leaked to comments-service without permission

**FraiseQL Fix:**
```json
{
  "subgraphs": [
    {
      "name": "users",
      "entities": [{
        "name": "User",
        "fields": [{
          "name": "email",
          "requiresAuth": true,
          "authScopes": ["user:profile"]
        }]
      }]
    },
    {
      "name": "comments",
      "references": [{
        "type": "User",
        "accessedFields": ["id", "name"],  // NOT email!
        "requiredAuthScopes": ["user:profile"]  // Check auth at JSONB construction
      }]
    }
  ]
}
```

#### ❌ ANTI-PATTERN: Missing @auth on Mutations
**Why it's a problem in FraiseQL:**
- Mutations are compiled deterministic operations
- Unprotected mutations can be called at compile time for cost calculation
- Exposes security decisions to caller

**FraiseQL Fix:**
```graphql
type Mutation {
  createUser(input: CreateUserInput!): User! @auth(requires: "admin:write")
  updatePost(id: ID!, input: UpdatePostInput!): Post! @auth(requires: "user:write")
}
```

---

### 5. Compilation Rules (Type & SQL Suitability)

#### ❌ ANTI-PATTERN: Circular Type Definitions
**Why it's a problem in FraiseQL:**
- FraiseQL compiles types to SQL structures
- Circular type definitions can't be compiled to determinate SQL schema
- Example: `User { posts: [Post] }` and `Post { author: User { posts: ... } }`

**FraiseQL Fix:**
```graphql
# ❌ BAD - Circular
type User {
  id: ID!
  posts: [Post!]!
}
type Post {
  id: ID!
  author: User!
  comments: [Comment!]!
}
type Comment {
  id: ID!
  author: User!
}

# ✅ GOOD - Acyclic (breaks cycle at Comment)
type User {
  id: ID!
  posts: [Post!]!
}
type Post {
  id: ID!
  authorId: ID!  # Reference, not nested type
  comments: [Comment!]!
}
type Comment {
  id: ID!
  authorId: ID!  # Reference, not nested type
}
```

#### ❌ ANTI-PATTERN: Missing Index Hints for JSONB Views
**Why it's a problem in FraiseQL:**
- JSONB view construction uses FOREIGN KEY joins
- Without indexes on FK columns, JSONB aggregation is slow
- Compiler can warn about missing indexes

**FraiseQL Fix:**
```json
{
  "entities": [{
    "name": "Post",
    "fields": [
      {"name": "id", "type": "ID", "isPrimaryKey": true},
      {"name": "userId", "type": "ID", "isForeignKey": true, "referencesTable": "users"}
    ],
    "suggestedIndexes": [
      {"columns": ["userId"], "reason": "JSONB view batching join key"}
    ]
  }]
}
```

---

## Summary: FraiseQL vs Generic GraphQL Rules

| Issue | Generic GraphQL | FraiseQL-Specific |
|-------|-----------------|-------------------|
| Over-federation | "Entity in 3 subgraphs" | "Can't batch in single JSONB view" |
| Circular deps | "Circular reference detected" | "Circular JSONB nesting - inefficient" |
| Query depth | "Query is 10 levels deep" | "Compiled JSONB will be exponential" |
| Unbounded pagination | "No limit specified" | "Compiler can't pre-compute cost" |
| Cache TTL | "TTL mismatch" | "JSONB coherency broken" |
| Auth boundary leak | "Field accessed cross-subgraph" | "Auth not checked at JSONB construction" |
| Missing types | N/A | "Compiler needs type metadata for JSONB optimization" |
| Missing indexes | N/A | "FK indexes needed for JSONB aggregation performance" |

---

## Implementation Priority

### Phase 3.1: Refactor Core Rules (FraiseQL-Calibrated)
- [ ] Federation: JSONB batching alignment checks
- [ ] Cost: Compiled query determinism checks
- [ ] Cache: JSONB coherency guarantees
- [ ] Auth: Auth at JSONB construction time
- [ ] Compilation: Type suitability and SQL compilation

### Phase 3.2: Add New FraiseQL-Specific Modules
- [ ] Compilation rules (type circularity, SQL suitability)
- [ ] JSONB optimization rules (index suggestions, aggregation efficiency)
- [ ] Metadata completeness checks (federation keys, type information)

### Phase 3.3: Calibration Testing
- [ ] Test rules with real FraiseQL schemas
- [ ] Validate rule suggestions are actionable for JSONB optimization
- [ ] Benchmark: well-designed schemas score 85+, poorly-designed score 40-
