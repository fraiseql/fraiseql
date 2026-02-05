<!-- Skip to main content -->
---
title: FraiseQL Linting Rules Reference
description: Each rule detects patterns that conflict with FraiseQL's compilation model. Violations don't prevent compilation—they reduce design quality scores and flag pote
keywords: []
tags: ["documentation", "reference"]
---

# FraiseQL Linting Rules Reference

## Comprehensive guide to FraiseQL's design quality rules

Each rule detects patterns that conflict with FraiseQL's compilation model. Violations don't prevent compilation—they reduce design quality scores and flag potential issues.

---

## Table of Contents

- [Federation Rules](#federation-rules)
- [Cost Rules](#cost-rules)
- [Cache Rules](#cache-rules)
- [Authorization Rules](#authorization-rules)
- [Compilation Rules](#compilation-rules)
- [Severity Levels](#severity-levels)

---

## Federation Rules

Federation rules detect patterns that prevent efficient JSONB batching across subgraphs.

### FED-001: Over-Federation (Entity in 3+ Subgraphs)

**Severity**: Warning

**Score Impact**: -10 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") { id: ID! }

# posts-service
type User @key(fields: "id") { id: ID!, postCount: Int! }

# comments-service
type User @key(fields: "id") { id: ID!, commentCount: Int! }
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- JSONB batching works by constructing a single result per entity
- When User exists in 3 places, FraiseQL can't predict which to use
- Results in non-deterministic compilation or runtime resolution

**How to Fix**:

1. Consolidate User in one service (users-service)
2. Other services reference User, don't redefine it
3. Add fields to User for cross-domain data:

```graphql
<!-- Code example in GraphQL -->
# users-service (single owner)
type User @key(fields: "id") {
  id: ID!
  email: String!
  postCount: Int!  # Moved here from posts-service
  commentCount: Int!  # Moved here from comments-service
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: User!  # Reference only
}
```text
<!-- Code example in TEXT -->

**Real-World Example**:
A 3-subgraph platform (users, content, analytics) had User in all three. Solution: User owns profile + identity, content owns content stats separately.

---

### FED-002: Circular Dependency Chain

**Severity**: Warning

**Score Impact**: -15 points per violation

**What It Detects**:

```text
<!-- Code example in TEXT -->
users-service → posts-service → comments-service → users-service
A → B → A (simplest case)
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Creates unbounded resolution paths
- Impossible to calculate worst-case complexity accurately
- JSONB construction becomes non-deterministic
- Federation joins loop infinitely

**How to Fix**:

1. Identify the circular reference:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  organizations: [Organization!]!  # Reference to org-service
}

# org-service
type Organization @key(fields: "id") {
  members: [User!]!  # Reference back to users-service (CYCLE!)
}
```text
<!-- Code example in TEXT -->

1. Break the cycle by using IDs instead of references:

```graphql
<!-- Code example in GraphQL -->
# org-service (owns the relationship)
type Organization @key(fields: "id") {
  id: ID!
  memberIds: [ID!]!  # IDs only
  members: [User!]!  # Optional: fetch separately if needed
}

# users-service
type User @key(fields: "id") {
  id: ID!
  organizationIds: [ID!]!  # IDs only
}
```text
<!-- Code example in TEXT -->

Or consolidate the relationship in one service:

```graphql
<!-- Code example in GraphQL -->
# org-service (owns relationship)
type Organization {
  members: [User!]!  # Managed relationship
}

# users-service
type User {
  # No reference back to organizations
  organizationId: ID!  # Just metadata
}
```text
<!-- Code example in TEXT -->

**Real-World Example**:
A platform had User ↔ Organization ↔ Team circular references. Solution: org-service owns relationships, users-service only stores IDs.

---

### FED-003: Missing Federation Key

**Severity**: Critical

**Score Impact**: -20 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# users-service (defines User)
type User @key(fields: "id") { ... }

# posts-service (uses User)
type Post {
  author: User!  # User referenced but has no @key in posts-service
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- FraiseQL can't resolve cross-subgraph references without a key
- Compilation fails or produces incorrect SQL
- Runtime federation resolution is impossible

**How to Fix**:

1. Ensure entity has @key in all subgraphs where it's defined:

```graphql
<!-- Code example in GraphQL -->
# posts-service
type User @key(fields: "id") {
  id: ID!
}

type Post {
  author: User!
}
```text
<!-- Code example in TEXT -->

1. Or, only define in one subgraph:

```graphql
<!-- Code example in GraphQL -->
# posts-service (reference only)
type Post {
  author: User!  # Defined in users-service, just referenced here
}
```text
<!-- Code example in TEXT -->

---

### FED-004: Inefficient Reference Pattern

**Severity**: Info

**Score Impact**: -5 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# posts-service
type Post @key(fields: "id") {
  author: User!  # Must resolve User
  authorId: ID!  # Also stores ID
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Storing both reference and ID is redundant
- Increases JSONB size without benefit
- Creates potential for data inconsistency

**How to Fix**:
Choose one pattern:

**Pattern A**: Store ID only, resolve on demand

```graphql
<!-- Code example in GraphQL -->
type Post {
  authorId: ID!
  author: User  # Optional: expensive to resolve
}
```text
<!-- Code example in TEXT -->

**Pattern B**: Store reference, derive ID when needed

```graphql
<!-- Code example in GraphQL -->
type Post {
  author: User!
  # authorId can be extracted from User.id in resolvers
}
```text
<!-- Code example in TEXT -->

---

## Cost Rules

Cost rules detect patterns that lead to worst-case complexity explosions.

### COST-001: Unbounded Collection

**Severity**: Warning

**Score Impact**: -8 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  posts: [Post!]!  # No pagination - could be millions
  comments: [Comment!]!  # Unbounded
}

type Post @key(fields: "id") {
  comments: [Comment!]!  # Unbounded
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Unbounded lists create n+1-like problems in JSONB
- Worst case: User with 1M posts × 1M comments = 1T JSONB records
- Compilation can't calculate safe complexity bounds

**How to Fix**:

1. Add pagination to all collections:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  posts(first: 20, after: String): PostConnection!
  comments(first: 20, after: String): CommentConnection!
}

type PostConnection {
  edges: [PostEdge!]!
  pageInfo: PageInfo!
}

type PageInfo {
  hasNextPage: Boolean!
  endCursor: String!
}
```text
<!-- Code example in TEXT -->

1. Or limit with maxItems:

```graphql
<!-- Code example in GraphQL -->
type User {
  topPosts(limit: 10): [Post!]!  # Hard limit
  recentComments(days: 7): [Comment!]!
}
```text
<!-- Code example in TEXT -->

---

### COST-002: Worst-Case Complexity Scenario

**Severity**: Critical

**Score Impact**: -20+ points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type Query {
  users(first: 100): [User!]!  # 100 users
}

type User {
  posts(first: 100): [Post!]!  # × 100 posts each = 10K
}

type Post {
  comments(first: 100): [Comment!]!  # × 100 comments = 1M records
}

type Comment {
  author: User!  # × 1 (batched) = 1M
}
```text
<!-- Code example in TEXT -->

Query: `{ users { posts { comments { author { posts { comments } } } } } }`

Worst case: 100 × 100 × 100 × 100 = **100M JSONB records**

**Why It Matters**:

- This query would consume all memory and hang
- Compiler can't catch this without complexity hints
- FraiseQL can't protect against runaway queries

**How to Fix**:

1. Add complexity directives:

```graphql
<!-- Code example in GraphQL -->
type User {
  posts(first: 20): PostConnection! @complexity(value: 5)
}

type Post {
  comments(first: 10): CommentConnection! @complexity(value: 3)
}

type Comment {
  author: User! @complexity(value: 1)
}
```text
<!-- Code example in TEXT -->

1. Reduce pagination limits:

```graphql
<!-- Code example in GraphQL -->
type Query {
  users(first: 10): [User!]!  # Reduced from 100
}

type User {
  posts(first: 10): [Post!]!  # Reduced from 100
}

type Post {
  comments(first: 10): [Comment!]!  # Reduced from 100
}
```text
<!-- Code example in TEXT -->

1. Disable nesting on expensive fields:

```graphql
<!-- Code example in GraphQL -->
type Post {
  comments(first: 100): [Comment!]!  # Many, but not nested further

  # Don't allow this:
  # { posts { comments { author { posts { ... } } } } }
}
```text
<!-- Code example in TEXT -->

---

### COST-003: Missing Complexity Hints

**Severity**: Info

**Score Impact**: -3 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type User {
  # No @complexity directive
  friends: [User!]!
  posts: [Post!]!
  recommendations: [User!]!  # Expensive aggregation
  sentiment: SentimentScore!  # External API call
}
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- FraiseQL assumes all fields cost the same
- Can't accurately predict expensive queries
- May allow queries that should be limited

**How to Fix**:
Add complexity hints to expensive fields:

```graphql
<!-- Code example in GraphQL -->
type User {
  friends: [User!]! @complexity(value: 10)  # Not too bad
  posts: [Post!]! @complexity(value: 5)  # Moderate
  recommendations: [User!]! @complexity(value: 50)  # Expensive aggregation
  sentiment: SentimentScore! @complexity(value: 100)  # External API
}
```text
<!-- Code example in TEXT -->

---

## Cache Rules

Cache rules ensure cached data stays coherent across subgraphs.

### CACHE-001: Inconsistent TTL Across Subgraphs

**Severity**: Warning

**Score Impact**: -6 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @cache(maxAge: 300) {  # 5 minutes
  id: ID!
  email: String!
}

# posts-service
type User @cache(maxAge: 3600) {  # 1 hour (INCONSISTENT!)
  id: ID!
  postCount: Int!
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Stale data scenario: email changes, but posts-service shows old email for 55 more minutes
- JSONB caches independently, increasing staleness window
- Users see conflicting data across requests

**How to Fix**:

1. Define federation-wide cache policy:

```graphql
<!-- Code example in GraphQL -->
# Agreed: All user data cached 5 minutes
type User @cache(maxAge: 300) {
  id: ID!
  email: String!
  postCount: Int!  # Even cross-service refs
}
```text
<!-- Code example in TEXT -->

1. Use cache groups for related entities:

```graphql
<!-- Code example in GraphQL -->
directive @cacheGroup(group: String!) on OBJECT

type User @cacheGroup(group: "user_profile") {
  id: ID!
  email: String!
}

type Post @cache(maxAge: 300) {  # Same as user_profile group
  author: User!
}
```text
<!-- Code example in TEXT -->

1. Or separate by mutability:

```graphql
<!-- Code example in GraphQL -->
type User {
  # Static data: longer TTL
  id: ID! @cache(maxAge: 3600)
  createdAt: DateTime! @cache(maxAge: 3600)

  # Dynamic data: shorter TTL
  email: String! @cache(maxAge: 300)
  lastLogin: DateTime! @cache(maxAge: 60)
}
```text
<!-- Code example in TEXT -->

---

### CACHE-002: Missing Cache Directive on Expensive Field

**Severity**: Info

**Score Impact**: -2 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type User {
  email: String!  # Cheap, doesn't need cache

  postCount: Int!  # Expensive aggregation, no @cache!
  friends: [User!]!  # Expensive calculation, no @cache!
  recommendations: [User!]!  # Very expensive, no @cache!
}
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- Expensive fields are recalculated every request
- JSONB reconstruction is wasteful
- Database load spikes for popular entities

**How to Fix**:
Add cache directives to expensive computations:

```graphql
<!-- Code example in GraphQL -->
type User {
  email: String!  # Not cached (cheap)

  postCount: Int! @cache(maxAge: 600)  # Cache 10 min
  friends: [User!]! @cache(maxAge: 300)  # Cache 5 min
  recommendations: [User!]! @cache(maxAge: 3600)  # Cache 1 hour
}
```text
<!-- Code example in TEXT -->

---

### CACHE-003: Variable Lifespan Data

**Severity**: Info

**Score Impact**: -1 point per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type Order {
  id: ID!
  items: [OrderItem!]! @cache(maxAge: 3600)  # Always same
  status: OrderStatus! @cache(maxAge: 3600)  # Changes frequently
  createdAt: DateTime! @cache(maxAge: 3600)  # Never changes
}
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- createdAt should cache forever (never changes)
- status should cache 60s (changes frequently)
- Same TTL for all wastes cache efficiency

**How to Fix**:
Match TTL to data mutability:

```graphql
<!-- Code example in GraphQL -->
type Order {
  id: ID! @cache(maxAge: 0)  # Permanent key
  createdAt: DateTime! @cache(maxAge: 31536000)  # Cache forever
  items: [OrderItem!]! @cache(maxAge: 3600)  # Cache 1 hour
  status: OrderStatus! @cache(maxAge: 60)  # Cache 1 minute
}
```text
<!-- Code example in TEXT -->

---

## Authorization Rules

Authorization rules ensure sensitive data doesn't leak across service boundaries.

### AUTH-001: Exposed Sensitive Field

**Severity**: Critical

**Score Impact**: -20 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String!  # Sensitive
  password: String!  # VERY sensitive
  passwordHash: String!  # Backend implementation detail
  ssn: String!  # PII
}

# posts-service (can see all)
type Post {
  author: User!  # Full User exposed
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Any query to posts-service now exposes sensitive user data
- Violates principle of least privilege
- Creates data leakage vulnerabilities
- Compliance violation (GDPR, HIPAA, etc.)

**How to Fix**:

1. Create public view of user:

```graphql
<!-- Code example in GraphQL -->
type PublicUserProfile {
  id: ID!
  displayName: String!
  avatarUrl: String!
  createdAt: DateTime!
}

type User @key(fields: "id") {
  id: ID!
  # Sensitive fields here, protected in users-service
  email: String! @auth(requires: "authenticated")
  publicProfile: PublicUserProfile!
}

# posts-service
type Post {
  author: PublicUserProfile!  # Safe, public data only
}
```text
<!-- Code example in TEXT -->

1. Or use scopes:

```graphql
<!-- Code example in GraphQL -->
type User {
  id: ID!
  email: String! @auth(scopes: ["user:email"])
  phone: String! @auth(scopes: ["user:phone"])
  password: String! @auth(scopes: ["user:password_reset"])
  # Only exposed if caller has scope
}
```text
<!-- Code example in TEXT -->

---

### AUTH-002: Missing Auth Directive on Sensitive Mutation

**Severity**: Critical

**Score Impact**: -20 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type Mutation {
  # Anyone can do this!
  deleteUser(id: ID!): Boolean!

  # Anyone can change password!
  updatePassword(userId: ID!, newPassword: String!): Boolean!

  # Admin operations without guard
  grantRole(userId: ID!, role: String!): Boolean!
}
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- Unauthenticated callers can modify critical data
- CRITICAL security vulnerability
- Compilation doesn't prevent this

**How to Fix**:

1. Add authentication requirements:

```graphql
<!-- Code example in GraphQL -->
type Mutation {
  # Only authenticated users can delete their own account
  deleteUser(id: ID!): Boolean! @auth(requires: "authenticated")

  # Only authenticated users can change their password
  updatePassword(userId: ID!, newPassword: String!): Boolean!
    @auth(requires: "authenticated", scopes: ["user:password_write"])

  # Only admins can grant roles
  grantRole(userId: ID!, role: String!): Boolean!
    @auth(requires: "admin", scopes: ["admin:grant_role"])
}
```text
<!-- Code example in TEXT -->

1. Add ownership validation in resolver:

```graphql
<!-- Code example in GraphQL -->
type Mutation {
  deleteUser(id: ID!): Boolean!
    @auth(requires: "authenticated")
    # Resolver must verify id == currentUser.id
}
```text
<!-- Code example in TEXT -->

---

### AUTH-003: Cross-Subgraph Auth Boundary Leak

**Severity**: Warning

**Score Impact**: -10 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @auth(requires: "authenticated") {
  id: ID!
  email: String! @auth(requires: "authenticated")
}

# posts-service (not authenticated, but can access User)
type Post @key(fields: "id") {
  id: ID!
  author: User!  # Unauthenticated service can fetch User
}
```text
<!-- Code example in TEXT -->

Scenario: Anonymous user queries posts-service → sees User.email

**Why It Matters**:

- Auth enforced in one service, bypassed in another
- Defeats authorization strategy

**How to Fix**:

1. Remove sensitive data from federation references:

```graphql
<!-- Code example in GraphQL -->
# users-service
type User {
  email: String! @auth(requires: "authenticated")
  publicProfile: PublicUserProfile!  # Auth not required
}

# posts-service
type Post {
  author: PublicUserProfile!  # Not sensitive
}
```text
<!-- Code example in TEXT -->

1. Or enforce auth in post-service too:

```graphql
<!-- Code example in GraphQL -->
# posts-service
type Post @auth(requires: "authenticated") {
  author: User!
}
```text
<!-- Code example in TEXT -->

---

## Compilation Rules

Compilation rules ensure your schema can be compiled to deterministic SQL.

### COMP-001: Circular Type Definition

**Severity**: Critical

**Score Impact**: -25 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type Author {
  books: [Book!]!
}

type Book {
  author: Author!
}
```text
<!-- Code example in TEXT -->

**Why It Matters for FraiseQL**:

- Creates infinite type graph
- JSONB compilation can't determine base structure
- May cause stack overflow during compilation

**How to Fix**:

1. Break cycle with IDs:

```graphql
<!-- Code example in GraphQL -->
type Author {
  id: ID!
  books: [Book!]!
}

type Book {
  id: ID!
  authorId: ID!
  author: Author  # Optional, resolved separately
}
```text
<!-- Code example in TEXT -->

1. Or use separate query:

```graphql
<!-- Code example in GraphQL -->
type Author {
  id: ID!
  bookIds: [ID!]!  # Just IDs
}

type Query {
  books(ids: [ID!]!): [Book!]!
}
```text
<!-- Code example in TEXT -->

---

### COMP-002: Missing Primary Key

**Severity**: Critical

**Score Impact**: -15 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {  # @key present
  name: String!
  # But missing actual 'id' field!
}
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- FraiseQL needs primary key to construct JSONB
- Can't generate deterministic SQL without identity
- Runtime failures when de-duplicating results

**How to Fix**:

1. Add the primary key field:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!  # Must be declared
  name: String!
}
```text
<!-- Code example in TEXT -->

1. Or change @key to use existing field:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "email") {
  email: String!  # Use email as key
  name: String!
}
```text
<!-- Code example in TEXT -->

---

### COMP-003: Missing Cardinality Hint

**Severity**: Info

**Score Impact**: -2 points per violation

**What It Detects**:

```graphql
<!-- Code example in GraphQL -->
type Post {
  comments: [Comment!]!  # Could be 0 or 1M, unknown
}

# No hint about expected cardinality
```text
<!-- Code example in TEXT -->

**Why It Matters**:

- Compiler allocates JSONB space based on cardinality
- Unknown cardinality = conservative estimate (wastes memory)
- Query performance suffers

**How to Fix**:
Add cardinality directives:

```graphql
<!-- Code example in GraphQL -->
type Post {
  comments: [Comment!]! @cardinality(estimate: "many")
  author: User! @cardinality(estimate: "one")
  tags: [String!]! @cardinality(estimate: "few")
}
```text
<!-- Code example in TEXT -->

---

## Severity Levels

### Critical (Score: -20 to -25)

Prevents compilation or causes runtime failures. Fix immediately.

- Missing primary key
- Circular definitions
- Over-federation (3+ subgraphs)
- Exposed sensitive data
- Unprotected sensitive mutations

### Warning (Score: -5 to -15)

Reduces quality, enables problematic patterns. Should be fixed.

- Unbounded collections
- Inconsistent cache TTLs
- Circular dependency chains
- Missing complexity hints
- Cross-subgraph auth leaks

### Info (Score: -1 to -3)

Minor issues, doesn't prevent compilation. Nice to fix.

- Missing cache directives
- Inefficient references
- Variable lifespan data
- Missing cardinality hints

---

## Scoring Examples

### Example 1: E-Commerce Platform

```text
<!-- Code example in TEXT -->
Base: 100
- Unbounded `User.posts`: -8
- Missing `Post.comments` complexity: -3
- Missing cache on `Product.inventory`: -2
- Inconsistent TTL (5min vs 1hr): -6

= 81/100 (Good)
```text
<!-- Code example in TEXT -->

### Example 2: Social Media

```text
<!-- Code example in TEXT -->
Base: 100
- User exposed across 3 subgraphs: -10
- Circular User ↔ Organization: -15
- Unbounded feeds: -8
- Missing auth on delete mutations: -20
- Missing complexity hints: -5

= 42/100 (Poor)
```text
<!-- Code example in TEXT -->

### Example 3: Well-Designed

```text
<!-- Code example in TEXT -->
Base: 100
- Proper pagination: 0
- Consolidated entities: 0
- Clear auth boundaries: 0
- Consistent cache strategy: 0
- Complexity hints present: 0

= 100/100 (Excellent)
```text
<!-- Code example in TEXT -->

---

**Next**: Check [CI_CD_INTEGRATION.md](./CI_CD_INTEGRATION.md) for how to integrate design quality checks into your development workflow.
