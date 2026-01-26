# Core Concepts - Understanding FraiseQL

**Duration**: 1-2 hours
**Outcome**: Understand how FraiseQL works internally
**Prerequisites**: Completed [GETTING_STARTED.md](GETTING_STARTED.md)

---

## Part 1: GraphQL Basics (5 minutes)

### What is GraphQL?

GraphQL is a query language for APIs that lets clients request exactly the data they need:

```graphql
query GetUser {
  user(id: "123") {
    name
    email
    # We only ask for the fields we need
  }
}
```

**Compared to REST**:

| REST | GraphQL |
|------|---------|
| Fixed endpoints return fixed data | Single endpoint returns requested data |
| Over-fetching (extra fields) | Exact fields requested |
| Under-fetching (multiple requests) | Single request gets all data |
| Multiple endpoints | Single endpoint |

### Why Use GraphQL?

1. **Precision**: Clients get exactly what they ask for
2. **Efficiency**: Single query instead of multiple REST calls
3. **Strong typing**: Schema defines all possible data
4. **Self-documenting**: Schema is the documentation
5. **Real-time capable**: Subscriptions for live updates

### Common Misconceptions

❌ **"GraphQL is only for JavaScript"** - False. GraphQL is language-agnostic. FraiseQL is Rust.

❌ **"GraphQL replaces all databases"** - False. GraphQL is a query API layer on top of databases.

❌ **"GraphQL is slow"** - False. FraiseQL compiles to optimized SQL at build-time, eliminating runtime overhead.

❌ **"GraphQL is only for complex queries"** - False. Works great for simple APIs too.

---

## Part 2: FraiseQL Design Philosophy (10 minutes)

### The FraiseQL Difference

Most GraphQL servers interpret schema and queries **at runtime**:

```
Request → Parse → Validate → Plan → Execute → Response
                     ↑ Overhead for every request
```

FraiseQL compiles schema and queries **at build-time**:

```
Schema → Compile → Optimized SQL (checked in)
Query → Check against compiled schema → Execute → Response
        ↑ Zero overhead at runtime
```

### Three-Layer Architecture

#### 1. **Authoring Layer** (Optional)
Use Python or TypeScript to define your schema:

```python
from fraiseql import type, query

@type
class User:
    id: int
    name: str
    email: str

@query
def users() -> list[User]:
    pass

@query
def user(id: int) -> User:
    pass
```

Generates `schema.json`

#### 2. **Compilation Layer**
```bash
fraiseql-cli compile schema.json --output schema.compiled.json
```

- Validates schema structure
- Generates optimized SQL templates
- Outputs `schema.compiled.json` (checked into git)

#### 3. **Runtime Layer**
```rust
let schema = CompiledSchema::from_file("schema.compiled.json")?;
let result = schema.execute(query).await?;
```

- Load pre-compiled schema (microseconds)
- Execute optimized SQL (milliseconds)
- Return typed results (zero serialization overhead)

### Why Three Layers?

| Layer | Benefit |
|-------|---------|
| Authoring | Ergonomic - write in language you know |
| Compilation | Validation - catch errors early, not in production |
| Runtime | Performance - zero interpretation overhead |

---

## Part 3: Data Flow (15 minutes)

### The Journey of a GraphQL Query

Let's trace how a query flows through FraiseQL:

```
┌──────────────────────────┐
│  1. Client sends query   │
│  query {                 │
│    user(id: "123") {     │
│      name                │
│      email               │
│    }                     │
│  }                       │
└────────────┬─────────────┘
             │
┌────────────▼──────────────────────┐
│  2. Schema validates query        │
│  ✓ "user" query exists            │
│  ✓ "id" argument is correct type  │
│  ✓ "name" field exists on User    │
│  ✓ "email" field exists on User   │
└────────────┬──────────────────────┘
             │
┌────────────▼──────────────────────────────┐
│  3. Runtime selects optimized SQL         │
│  Pre-compiled: "SELECT name, email       │
│               FROM users WHERE id = $1"  │
└────────────┬──────────────────────────────┘
             │
┌────────────▼──────────────────────────────┐
│  4. Execute SQL with parameters           │
│  Connection Pool → Database               │
│  Return: [{name: "Alice", email: "..."}] │
└────────────┬──────────────────────────────┘
             │
┌────────────▼──────────────────────────────┐
│  5. Format response                       │
│  {                                        │
│    "data": {                              │
│      "user": {                            │
│        "name": "Alice",                   │
│        "email": "alice@example.com"       │
│      }                                    │
│    }                                      │
│  }                                        │
└──────────────────────────────────────────┘
```

### Query Execution Timing

```
Total time per request: ~5-50ms (depending on query)

Parse & validate: ~0.1ms (pre-compiled, just checking)
Execute SQL:      ~2-40ms (depends on database query)
Format response:  ~0.1ms (direct serialization)
Network round-trip: ~2-10ms (varies by client location)
```

### Type System

FraiseQL's type system ensures safety at every level:

```graphql
type User {
  id: ID!        # Non-null (always has value)
  name: String!  # String, required
  email: String! # String, required
  age: Int       # Optional integer
}

query user(id: ID!): User {
  # Returns a User, must provide id
}

query users: [User!]! {
  # Returns non-null array of non-null Users
}
```

**Type Safety Benefits**:
- ✅ Compile-time validation of queries
- ✅ Automatic SQL type conversion
- ✅ No runtime type errors
- ✅ IDE auto-completion works perfectly

---

## Part 4: Schema Definition (30 minutes)

### Schema Structure

A FraiseQL schema defines types and queries:

```json
{
  "types": [
    {
      "name": "User",
      "fields": [
        { "name": "id", "type": "ID", "nonNull": true },
        { "name": "name", "type": "String", "nonNull": true },
        { "name": "email", "type": "String", "nonNull": true }
      ]
    }
  ],
  "queries": [
    {
      "name": "users",
      "returnType": "User",
      "isList": true,
      "args": []
    },
    {
      "name": "user",
      "returnType": "User",
      "isList": false,
      "args": [
        { "name": "id", "type": "ID", "nonNull": true }
      ]
    }
  ]
}
```

### Scalar Types

Built-in types provided by FraiseQL:

| Type | Example | Database | Notes |
|------|---------|----------|-------|
| `ID` | `"123"` | `BIGINT` | Unique identifier |
| `String` | `"hello"` | `VARCHAR` | Text data |
| `Int` | `42` | `INTEGER` | -2³¹ to 2³¹-1 |
| `Float` | `3.14` | `FLOAT` | 64-bit IEEE floating point |
| `Boolean` | `true` | `BOOLEAN` | true or false |
| `DateTime` | `"2026-01-26T10:30:00Z"` | `TIMESTAMP` | ISO 8601 format |
| `JSON` | `{"key": "value"}` | `JSONB` | Arbitrary JSON |

### Composite Types

Combine scalars into types:

```json
{
  "name": "BlogPost",
  "fields": [
    { "name": "id", "type": "ID", "nonNull": true },
    { "name": "title", "type": "String", "nonNull": true },
    { "name": "content", "type": "String", "nonNull": true },
    { "name": "author", "type": "User", "nonNull": true },
    { "name": "published", "type": "Boolean", "nonNull": false },
    { "name": "createdAt", "type": "DateTime", "nonNull": true }
  ]
}
```

### Querying Nested Types

```graphql
query GetPost {
  post(id: "1") {
    title
    content
    author {
      name
      email
    }
    createdAt
  }
}
```

Response:
```json
{
  "data": {
    "post": {
      "title": "Getting Started",
      "content": "First steps with FraiseQL...",
      "author": {
        "name": "Alice",
        "email": "alice@example.com"
      },
      "createdAt": "2026-01-26T10:30:00Z"
    }
  }
}
```

### Mutations

Mutations modify data (create, update, delete):

```json
{
  "mutations": [
    {
      "name": "createUser",
      "args": [
        { "name": "name", "type": "String", "nonNull": true },
        { "name": "email", "type": "String", "nonNull": true }
      ],
      "returnType": "User",
      "isList": false
    },
    {
      "name": "updateUser",
      "args": [
        { "name": "id", "type": "ID", "nonNull": true },
        { "name": "name", "type": "String", "nonNull": false },
        { "name": "email", "type": "String", "nonNull": false }
      ],
      "returnType": "User",
      "isList": false
    }
  ]
}
```

Query:
```graphql
mutation CreateUser {
  createUser(name: "Bob", email: "bob@example.com") {
    id
    name
    email
  }
}
```

---

## Part 5: Query Execution (30 minutes)

### Query Syntax

#### Simple Query

```graphql
query {
  users {
    id
    name
  }
}
```

#### Query with Arguments

```graphql
query {
  user(id: "123") {
    id
    name
    email
  }
}
```

#### Multiple Queries (Batching)

```graphql
query {
  alice: user(id: "1") {
    name
  }
  bob: user(id: "2") {
    name
  }
}
```

Response:
```json
{
  "data": {
    "alice": { "name": "Alice" },
    "bob": { "name": "Bob" }
  }
}
```

### Variables

Use variables to parameterize queries:

```graphql
query GetUser($userId: ID!) {
  user(id: $userId) {
    id
    name
    email
  }
}
```

Execute with:
```json
{
  "query": "query GetUser($userId: ID!) { user(id: $userId) { id name email } }",
  "variables": { "userId": "123" }
}
```

**Benefits**:
- ✅ Prevents SQL injection (parameters are typed)
- ✅ Queries are cacheable (same query structure)
- ✅ Client libraries handle variables automatically

### Error Handling

FraiseQL returns structured errors:

```json
{
  "errors": [
    {
      "message": "Field 'unknownField' not found on type User",
      "path": ["user", "unknownField"],
      "extensions": {
        "code": "FIELD_NOT_FOUND",
        "line": 3,
        "column": 5
      }
    }
  ]
}
```

Common error codes:
- `PARSE_ERROR` - Query syntax invalid
- `VALIDATION_ERROR` - Query invalid for schema
- `FIELD_NOT_FOUND` - Field doesn't exist on type
- `ARGUMENT_ERROR` - Argument type mismatch
- `DATABASE_ERROR` - Query execution failed
- `AUTHENTICATION_ERROR` - Not authenticated
- `AUTHORIZATION_ERROR` - Not authorized

### Pagination

For large result sets, use pagination:

```graphql
query GetUsers {
  users(first: 10, after: "cursor123") {
    edges {
      node {
        id
        name
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

See [PATTERNS.md](PATTERNS.md) for implementation details.

### Aliases

Execute the same query multiple times with different arguments:

```graphql
query GetMultipleUsers {
  activeUsers: users(status: "active") {
    id
    name
  }
  inactiveUsers: users(status: "inactive") {
    id
    name
  }
}
```

---

## Part 6: Performance Tips (15 minutes)

### Query Complexity Limits

FraiseQL prevents overly complex queries that could DoS the server:

```graphql
# This query is TOO complex - requests 1,000,000 nested fields
query {
  users {
    posts {
      comments {
        replies {
          reactions {
            # ... deeply nested
          }
        }
      }
    }
  }
}
```

FraiseQL will reject with:
```
Query complexity (1000000) exceeds maximum allowed (5000)
```

**Configure limits** (in compiled schema):
```json
{
  "limits": {
    "maxQueryComplexity": 5000,
    "maxDepth": 15,
    "maxAliases": 10
  }
}
```

### Avoiding N+1 Queries

❌ **Bad Pattern** (Causes N+1 problem):

```graphql
query GetPosts {
  posts {
    id
    title
    author {
      name  # Separate database query for each post!
    }
  }
}
```

This causes:
- 1 query to fetch posts
- N queries to fetch author for each post
- Total: N+1 queries

✅ **Good Pattern** (Batch fetching):

FraiseQL automatically batches related queries:

```graphql
query GetPosts {
  posts {
    id
    title
    author {
      name  # Fetched in single batch query
    }
  }
}
```

FraiseQL generates optimized SQL:
```sql
SELECT posts.id, posts.title, posts.author_id
FROM posts;

-- Single query for all authors (batch)
SELECT id, name
FROM users
WHERE id IN (SELECT author_id FROM posts)
```

### Connection Pooling

FraiseQL uses connection pooling to reuse database connections:

```rust
let pool = ConnectionPool::new(
    "postgresql://user:pass@localhost/db",
    PoolConfig {
        min_connections: 5,
        max_connections: 20,
        connection_timeout: Duration::from_secs(5),
    }
).await?;
```

**Benefits**:
- ✅ Reduced connection overhead
- ✅ Faster query execution
- ✅ Better resource utilization
- ✅ Handles connection failures gracefully

### Caching

FraiseQL supports query result caching:

```graphql
query GetUser @cached(ttl: 300) {
  user(id: "123") {
    id
    name
    email
  }
}
```

Cache hits: <1ms (in-memory)
Cache misses: Normal query time (then stored)

See [PATTERNS.md](PATTERNS.md) for caching strategies.

### Monitoring Query Performance

Enable query logging:

```rust
let schema = CompiledSchema::from_file("schema.compiled.json")?;
schema.enable_query_logging();

let result = schema.execute(query).await?;
// Logs:
// Query execution time: 12.3ms
// Database query time: 10.1ms
// Result serialization time: 0.2ms
```

### Query Optimization Checklist

- [ ] Only request fields you need
- [ ] Use pagination for large result sets
- [ ] Add indexes to frequently filtered fields
- [ ] Monitor slow queries with logs
- [ ] Keep nested queries to <5 levels
- [ ] Use aliases to batch similar queries
- [ ] Enable caching for static queries

---

## Summary

You now understand:

✅ What GraphQL is and why it's useful
✅ How FraiseQL differs (compile-time optimization)
✅ The three-layer architecture (authoring → compilation → runtime)
✅ How queries flow through the system
✅ How to define schemas with types and fields
✅ How to write queries and mutations
✅ How to handle errors and pagination
✅ Performance optimization techniques

## Next Steps

- **Ready for patterns?** → [PATTERNS.md](PATTERNS.md) (Learn 6 real-world patterns)
- **Ready to deploy?** → [DEPLOYMENT.md](DEPLOYMENT.md) (Get to production)
- **Questions?** → [TROUBLESHOOTING.md](TROUBLESHOOTING.md) (Common problems & FAQ)

---

**Questions?** See [FAQ.md](FAQ.md) or open an issue on [GitHub](https://github.com/fraiseql/fraiseql-v2).
