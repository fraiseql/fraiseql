<!-- Skip to main content -->
---
title: Designing Schemas for FraiseQL
description: This guide covers design patterns specific to FraiseQL's compilation model. FraiseQL automatically optimizes SQL execution for your schema, but *architectural d
keywords: ["schema"]
tags: ["documentation", "reference"]
---

# Designing Schemas for FraiseQL

## Master the art of architecting GraphQL for compiled SQL execution

This guide covers design patterns specific to FraiseQL's compilation model. FraiseQL automatically optimizes SQL execution for your schema, but *architectural decisions* matter—and matter a lot.

---

## Table of Contents

1. [Core Philosophy](#core-philosophy)
2. [Federation Design Patterns](#federation-design-patterns)
3. [Cost & Complexity Management](#cost--complexity-management)
4. [Cache Coherency Strategies](#cache-coherency-strategies)
5. [Authorization Boundaries](#authorization-boundaries)
6. [Schema Organization](#schema-organization)
7. [Anti-Patterns to Avoid](#anti-patterns-to-avoid)
8. [Real-World Examples](#real-world-examples)

---

## Core Philosophy

FraiseQL separates what it can optimize from what it can't:

### FraiseQL DOES

- ✅ Prevent n+1 queries via JSONB view batching
- ✅ Pre-optimize SQL execution at compile time
- ✅ Generate efficient joins and aggregations
- ✅ Handle query complexity scaling

### FraiseQL CANNOT

- ❌ Fix federation fragmentation (your architectural choice)
- ❌ Auto-correct circular dependency chains
- ❌ Solve worst-case complexity scenarios
- ❌ Enforce cache coherency across subgraphs
- ❌ Guarantee authorization boundaries

**Your job**: Make decisions that don't create these problems.

---

## Federation Design Patterns

### Pattern 1: Consolidated Services (Recommended)

**Goal**: Each business domain owns its types completely.

```graphql
<!-- Code example in GraphQL -->
# users-service (owns User, Account, Profile)
type User @key(fields: "id") {
  id: ID!
  email: String!
  profile: Profile!
  account: Account!
}

type Profile {
  bio: String
  avatar: URL
}

type Account {
  tier: String
  createdAt: DateTime
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  authorId: ID!
  author: User  # Reference to users-service
  content: String!
}
```text
<!-- Code example in TEXT -->

### Why it works for FraiseQL

- Entity lives in one place (no fragmentation)
- Clean batching: `User -> Profile` is local
- Federation joins are explicit cross-service references
- JSONB construction is deterministic

**Score impact**: Federation score: 100

---

### Pattern 2: Smart Federation Keys

**Goal**: Use federation keys that prevent excessive subgraph hops.

❌ **BAD**: User defined in 3+ places

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String!
}

# posts-service (ALSO defines User!)
type User @key(fields: "id") {
  id: ID!
  postCount: Int!
}

# comments-service (ALSO defines User!)
type User @key(fields: "id") {
  id: ID!
  commentCount: Int!
}
```text
<!-- Code example in TEXT -->

✅ **GOOD**: User lives in one place, other services reference it

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String!
  posts: [Post!]!
  comments: [Comment!]!
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: User!  # Reference only
}

# comments-service
type Comment @key(fields: "id") {
  id: ID!
  author: User!  # Reference only
}
```text
<!-- Code example in TEXT -->

### Why it matters for FraiseQL

- JSONB batching works best with consolidated entities
- Reduces resolution chains
- Makes complexity analysis tractable

---

### Pattern 3: Avoiding Circular Chains

**Goal**: Prevent A → B → A federation resolution patterns.

❌ **BAD**: Circular references

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  friends: [User!]!
  organization: Organization  # Reference to org-service
}

# org-service
type Organization @key(fields: "id") {
  id: ID!
  members: [User!]!  # Reference back to users-service
}
```text
<!-- Code example in TEXT -->

When resolving a user's organization's members, you need to go:
`users-service → org-service → users-service`

This creates circular dependency chains that impact both cost and cache coherency.

✅ **GOOD**: Break the cycle with explicit fields

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  organizationId: ID!  # Just the ID, no reference
  organization: Organization
}

# org-service
type Organization @key(fields: "id") {
  id: ID!
  memberIds: [ID!]!  # Just IDs, handled differently
  members: [User!]!
}
```text
<!-- Code example in TEXT -->

**Better yet**: One service owns the many-to-many relationship

```graphql
<!-- Code example in GraphQL -->
# org-service (owns the relationship)
type Organization @key(fields: "id") {
  id: ID!
  members: [User!]!  # Owned, not referenced
}

# users-service
type User @key(fields: "id") {
  id: ID!
  organizations: [Organization!]!  # Could be reference if needed
}
```text
<!-- Code example in TEXT -->

---

## Cost & Complexity Management

### Strategy 1: Pagination Everything That Scales

**Goal**: Prevent worst-case complexity explosions.

❌ **BAD**: Unbounded lists

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!
  posts: [Post!]!  # User can have thousands of posts
  comments: [Comment!]!  # User can have millions of comments
}

type Post @key(fields: "id") {
  id: ID!
  comments: [Comment!]!  # Post can have thousands of comments
}
```text
<!-- Code example in TEXT -->

Query: `user { posts { comments { author { posts { comments } } } } }`

Worst case: 1000 posts × 1000 comments/post × 50 authors × 1000 posts/author = 50B combinations

✅ **GOOD**: Paginate to prevent explosion

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!
  posts(first: 10, after: String): PostConnection!
  comments(first: 100, after: String): CommentConnection!
}

type PostConnection {
  edges: [PostEdge!]!
  pageInfo: PageInfo!
}

type Post @key(fields: "id") {
  id: ID!
  comments(first: 10, after: String): CommentConnection!
}
```text
<!-- Code example in TEXT -->

**FraiseQL calculates**:

- Base: 1 user × 10 posts × 10 comments = 100 (tractable)
- With author resolution: 100 × 1 (author already batched) = 100
- With author's posts: 100 × 10 = 1000 (still reasonable)

**Scoring**: Unbounded collections = Cost violations. Paginated = Clean score.

---

### Strategy 2: Complexity Multipliers

Use metadata to help FraiseQL understand field cost:

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!
  email: String!

  # Cheap: cached aggregation
  postCount: Int! @complexity(value: 1)

  # Medium: requires list construction
  posts(first: 10, after: String): PostConnection! @complexity(value: 5)

  # Expensive: requires deep joins
  friends: [User!]! @complexity(value: 50)

  # Very expensive: requires external API
  sentiment: SentimentAnalysis! @complexity(value: 500)
}
```text
<!-- Code example in TEXT -->

FraiseQL uses these to calculate worst-case scenarios and flag problematic patterns.

---

## Cache Coherency Strategies

### Strategy 1: Unified TTLs Across Federation

**Goal**: Prevent stale data scenarios where one service has fresh data and another doesn't.

❌ **BAD**: Different TTLs

```graphql
<!-- Code example in GraphQL -->
# users-service
directive @cache(maxAge: 300) on FIELD_DEFINITION

type User @key(fields: "id") {
  id: ID!
  email: String! @cache(maxAge: 300)  # 5 minutes
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: User! @cache(maxAge: 3600)  # 1 hour (DIFFERENT!)
}
```text
<!-- Code example in TEXT -->

Scenario: User changes email in users-service, but posts-service still shows old email for 55 more minutes.

✅ **GOOD**: Consistent strategy across federation

```graphql
<!-- Code example in GraphQL -->
# Agreed standard: 5 minutes for user entities
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String! @cache(maxAge: 300)
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: User! @cache(maxAge: 300)  # Same TTL
}
```text
<!-- Code example in TEXT -->

Or use explicit cache groups:

```graphql
<!-- Code example in GraphQL -->
directive @cacheGroup(name: String!) on OBJECT

type User @key(fields: "id") @cacheGroup(name: "user_profile") {
  id: ID!
  email: String!
}

type Post @key(fields: "id") {
  id: ID!
  author: User! # Inherits cacheGroup: user_profile
}
```text
<!-- Code example in TEXT -->

---

### Strategy 2: Cache Directives on Expensive Fields

**Goal**: Cache fields that are expensive to compute.

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!

  # Cheap: just a lookup
  email: String!

  # Expensive: requires aggregation
  postCount: Int! @cache(maxAge: 600)

  # Very expensive: external API
  sentiment: SentimentAnalysis! @cache(maxAge: 3600)

  # User-specific: don't cache
  followingMe: Boolean!  # No @cache
}
```text
<!-- Code example in TEXT -->

---

## Authorization Boundaries

### Pattern 1: Auth at Service Boundaries

**Goal**: Never expose sensitive data across unauthorized service boundaries.

❌ **BAD**: Exposed at federation boundary

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String!  # Sensitive!
  password: String!  # VERY sensitive!
}

# posts-service (can see sensitive user data)
type Post @key(fields: "id") {
  id: ID!
  author: User!  # Full User including sensitive fields
}
```text
<!-- Code example in TEXT -->

Any request to posts-service can now see user emails and passwords!

✅ **GOOD**: Auth-aware federation fields

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!

  email: String! @auth(requires: "authenticated")

  publicProfile: PublicUserProfile!  # Safe subset
}

type PublicUserProfile {
  id: ID!
  displayName: String!
  avatarUrl: String!
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: PublicUserProfile!  # Only public data
}
```text
<!-- Code example in TEXT -->

**Even better**: Scope-based access

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!

  email: String! @auth(scopes: ["user:email"])

  phoneNumber: String! @auth(scopes: ["user:phone"])

  paymentMethods: [PaymentMethod!]! @auth(scopes: ["billing:read"])
}
```text
<!-- Code example in TEXT -->

---

### Pattern 2: Auth Boundaries in Mutations

**Goal**: Ensure only authorized callers can perform mutations.

```graphql
<!-- Code example in GraphQL -->
type Mutation {
  # Public: anyone can create account
  signUp(email: String!, password: String!): AuthPayload!

  # Authenticated: own user only
  updateProfile(input: UpdateProfileInput!): User!
    @auth(requires: "authenticated", scopes: ["profile:write"])

  # Admin: admin-only operations
  deleteUser(userId: ID!): Boolean!
    @auth(requires: "admin", scopes: ["user:delete"])

  # Billing: specific scope required
  updateBillingInfo(input: BillingInput!): User!
    @auth(scopes: ["billing:write"])
}
```text
<!-- Code example in TEXT -->

---

## Schema Organization

### Principle 1: Ownership Clarity

Every type should have a clear owner (service):

```graphql
<!-- Code example in GraphQL -->
# Global schema (for documentation)
schema {
  query: Query
  mutation: Mutation
}

# users-service (owns User, Profile, Account)
type User @key(fields: "id") @ownerService("users-service") { ... }
type Profile @ownerService("users-service") { ... }
type Account @ownerService("users-service") { ... }

# posts-service (owns Post, PostComment)
type Post @key(fields: "id") @ownerService("posts-service") { ... }
type PostComment @ownerService("posts-service") { ... }

# org-service (owns Organization, Team, Role)
type Organization @key(fields: "id") @ownerService("org-service") { ... }
type Team @ownerService("org-service") { ... }
type Role @ownerService("org-service") { ... }
```text
<!-- Code example in TEXT -->

### Benefits

- Clear responsibility
- Easier debugging (which service owns that bug?)
- Facilitates schema ownership policies

---

### Principle 2: Logical Grouping

Group related types together, not alphabetically:

```graphql
<!-- Code example in GraphQL -->
# Bad: Alphabetical
type Account { ... }
type Blog { ... }
type Comment { ... }
type Organization { ... }
type Post { ... }
type User { ... }

# Good: By domain
# User Management Domain
type User { ... }
type Account { ... }
type Profile { ... }
type AuthToken { ... }

# Content Domain
type Post { ... }
type Blog { ... }
type BlogPost { ... }
type Comment { ... }

# Organization Domain
type Organization { ... }
type Team { ... }
type Role { ... }
```text
<!-- Code example in TEXT -->

---

## Anti-Patterns to Avoid

### Anti-Pattern 1: The God Object

❌ **Bad**: One type trying to do everything

```graphql
<!-- Code example in GraphQL -->
type User {
  id: ID!
  email: String!
  posts: [Post!]!
  comments: [Comment!]!
  followers: [User!]!
  following: [User!]!
  organizations: [Organization!]!
  teams: [Team!]!
  roles: [Role!]!
  permissions: [Permission!]!
  settings: UserSettings!
  notifications: [Notification!]!
  activityLog: [Activity!]!
  # ... and 50 more fields
}
```text
<!-- Code example in TEXT -->

### Problems

- Impossible to cache effectively (too many variations)
- Massive JSONB blobs
- Authorization becomes complex
- Cost analysis breaks down

✅ **Solution**: Decompose into focused types

```graphql
<!-- Code example in GraphQL -->
type User @key(fields: "id") {
  id: ID!
  email: String!
  profile: UserProfile!
  content: UserContent!
  social: UserSocial!
  administration: UserAdmin!
}

type UserProfile {
  settings: UserSettings!
  notifications: [Notification!]!
}

type UserContent {
  posts(first: 10): PostConnection!
  comments(first: 10): CommentConnection!
}

type UserSocial {
  followers(first: 10): UserConnection!
  following(first: 10): UserConnection!
}

type UserAdmin {
  organizations: [Organization!]!
  teams: [Team!]!
  roles: [Role!]!
}
```text
<!-- Code example in TEXT -->

---

### Anti-Pattern 2: Circular References

❌ **Bad**: Object references create circular chains

```graphql
<!-- Code example in GraphQL -->
type Author @key(fields: "id") {
  id: ID!
  posts: [Post!]!
}

type Post @key(fields: "id") {
  id: ID!
  author: Author!
  comments: [Comment!]!
}

type Comment @key(fields: "id") {
  id: ID!
  post: Post!
  author: Author!
}
```text
<!-- Code example in TEXT -->

Query: `author { posts { comments { author { posts { comments { ... } } } } } }`

Results in exponential complexity.

✅ **Solution**: Use IDs instead of references for circular paths

```graphql
<!-- Code example in GraphQL -->
type Author @key(fields: "id") {
  id: ID!
  posts(first: 10): PostConnection!
}

type Post @key(fields: "id") {
  id: ID!
  authorId: ID!  # Instead of author reference
  author: Author!  # Only when explicitly requested
  comments(first: 10): CommentConnection!
}

type Comment @key(fields: "id") {
  id: ID!
  postId: ID!  # Just the ID
  authorId: ID!  # Just the ID
}
```text
<!-- Code example in TEXT -->

---

### Anti-Pattern 3: Unbounded Collections

❌ **Bad**: Lists with no pagination

```graphql
<!-- Code example in GraphQL -->
type User {
  id: ID!
  allPosts: [Post!]!  # Could be millions
  allFriends: [User!]!  # Could be hundreds of thousands
  allComments: [Comment!]!  # Unbounded
}
```text
<!-- Code example in TEXT -->

✅ **Good**: Everything paginated

```graphql
<!-- Code example in GraphQL -->
type User {
  id: ID!
  posts(first: 20, after: String): PostConnection!
  friends(first: 20, after: String): UserConnection!
  comments(first: 20, after: String): CommentConnection!
}
```text
<!-- Code example in TEXT -->

---

## Real-World Examples

### Example 1: E-Commerce Platform

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  email: String! @auth(scopes: ["user:email"])
  profile: UserProfile!
  orders(first: 10): OrderConnection!
}

type UserProfile {
  name: String!
  avatar: URL!
  createdAt: DateTime!
}

# orders-service
type Order @key(fields: "id") {
  id: ID!
  userId: ID!
  user: User!
  items(first: 20): OrderItemConnection!
  total: Money!
  createdAt: DateTime!
  status: OrderStatus!
}

type OrderItem {
  product: Product!
  quantity: Int!
  price: Money!
}

# products-service
type Product @key(fields: "id") {
  id: ID!
  name: String!
  price: Money!
  inventory: Int! @cache(maxAge: 60)
}
```text
<!-- Code example in TEXT -->

### Why this design works for FraiseQL

- Clear ownership (each service owns its domain)
- User consolidated in one place (no fragmentation)
- Pagination prevents explosion (orders and items bounded)
- Cache is simple (product inventory fresh every minute)
- Auth clear (email scoped)

---

### Example 2: Social Media Platform

```graphql
<!-- Code example in GraphQL -->
# users-service
type User @key(fields: "id") {
  id: ID!
  username: String!
  profile: PublicProfile!
  private: PrivateUserData!  # Only for self or admins
}

type PublicProfile {
  displayName: String!
  bio: String!
  avatar: URL!
  followerCount: Int!  # Cached aggregation
  followingCount: Int!  # Cached aggregation
}

type PrivateUserData @auth(requires: "authenticated") {
  email: String!
  createdAt: DateTime!
  lastLoginAt: DateTime!
}

# posts-service
type Post @key(fields: "id") {
  id: ID!
  author: PublicProfile!  # Safe subset
  content: String!
  likeCount: Int! @cache(maxAge: 30)
  comments(first: 10): CommentConnection!
  createdAt: DateTime!
}

type Comment {
  author: PublicProfile!
  content: String!
  likeCount: Int! @cache(maxAge: 30)
  createdAt: DateTime!
}

# graphs-service
type UserGraph {
  user: User!
  followers(first: 20): UserConnection!
  following(first: 20): UserConnection!
  recommendations(first: 10): UserConnection!  # Paginated
}
```text
<!-- Code example in TEXT -->

### Why this works

- Public/private data clearly separated
- User relationships paginated (no explosion)
- Aggregations (counts) cached efficiently
- No circular chains (just IDs)
- Auth enforced at boundaries

---

## Summary

Design for FraiseQL by:

1. **Consolidate** - One service per entity type
2. **Paginate** - Bound all collections
3. **Cache consistently** - Same TTLs across federation
4. **Scope auth** - Expose only what's needed
5. **Avoid circles** - Use IDs to break dependency chains
6. **Decompose** - Break god objects into focused types
7. **Own clearly** - Every type has an owner service

These patterns let FraiseQL do what it does best: compile optimal SQL for your GraphQL.

---

**Next**: Check [LINTING_RULES.md](./LINTING_RULES.md) for detailed rule reference and how FraiseQL's linter detects violations of these patterns.
