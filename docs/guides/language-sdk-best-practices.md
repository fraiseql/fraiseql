# Language SDK Best Practices

**Status:** ✅ Production Ready
**Audience:** Developers (all languages)
**Reading Time:** 20-25 minutes per language
**Last Updated:** 2026-02-05

Language-specific best practices and idioms for using FraiseQL with your preferred programming language. Select your language below.

---

## Overview

FraiseQL SDKs are available for **16 languages**. While the GraphQL API is universal, each language has idiomatic patterns and best practices. This guide covers the major languages; principles apply to others.

**Quick links:**
- [Python](#python-best-practices)
- [TypeScript/JavaScript](#typescriptjavascript-best-practices)
- [Go](#go-best-practices)
- [Java](#java-best-practices)
- [Other Languages](#other-languages)

---

## Python Best Practices

### 1. Type Annotations & Validation

**Best practice:** Use Python 3.10+ union syntax and Pydantic for validation

```python
# ✅ Good: Modern type hints
from typing import Optional
from pydantic import BaseModel, EmailStr

class User(BaseModel):
    id: str
    name: str
    email: EmailStr
    age: int | None = None  # Python 3.10+ union syntax

# ❌ Avoid: Old Optional syntax
from typing import Optional
age: Optional[int] = None  # Older style
```

### 2. Async/Await Patterns

**Best practice:** Use `asyncio` consistently throughout the application

```python
# ✅ Good: Async client with async context manager
from fraiseql import AsyncClient

async def main():
    async with AsyncClient(url="http://localhost:5000") as client:
        result = await client.query(
            """
            query GetUsers {
              users { id name email }
            }
            """
        )
        for user in result["users"]:
            print(user["name"])

asyncio.run(main())
```

**Avoid blocking calls in async code:**
```python
# ❌ Bad: Blocking I/O in async function
async def fetch_user(id: str):
    response = requests.get(...)  # BLOCKS!
    return response.json()

# ✅ Good: Use async HTTP library
async def fetch_user(id: str):
    async with aiohttp.ClientSession() as session:
        async with session.get(...) as response:
            return await response.json()
```

### 3. Error Handling

**Best practice:** Handle FraiseQL errors explicitly

```python
from fraiseql import AsyncClient, FraiseQLError

async def safe_query():
    async with AsyncClient(url="http://localhost:5000") as client:
        try:
            result = await client.query(query_string)
        except FraiseQLError as e:
            if e.code == "E_AUTH_UNAUTHORIZED":
                print(f"Unauthorized: {e.message}")
            elif e.code.startswith("E_VALIDATION"):
                print(f"Validation error: {e.message}")
            else:
                raise  # Re-raise unknown errors
```

### 4. Connection Pooling

**Best practice:** Reuse single client instance

```python
# ✅ Good: Singleton client
client = AsyncClient(url="http://localhost:5000")

async def main():
    result1 = await client.query(query1)
    result2 = await client.query(query2)
    # Same connection reused

# ❌ Bad: Creating new client for each query
async def query_users():
    async with AsyncClient(...) as client:  # New connection!
        return await client.query(...)

async def query_posts():
    async with AsyncClient(...) as client:  # New connection!
        return await client.query(...)
```

### 5. Testing

**Best practice:** Use pytest with fixtures

```python
# conftest.py
@pytest.fixture
async def client():
    async with AsyncClient(url="http://localhost:5000") as c:
        yield c

# test_users.py
@pytest.mark.asyncio
async def test_get_users(client):
    result = await client.query("{ users { id name } }")
    assert len(result["users"]) > 0
    assert "id" in result["users"][0]
```

---

## TypeScript/JavaScript Best Practices

### 1. Type Safety with Generated Types

**Best practice:** Generate types from schema

```bash
# Generate types from FraiseQL schema
fraiseql-codegen generate --schema schema.json --output types.ts
```

**Usage:**
```typescript
import { Query, User } from './generated/types';

const query: Query = {
  users: {
    __typename: true,
    id: true,
    name: true,
  }
};

const result: User[] = await client.query(query);
// result has full type safety!
```

### 2. React/Vue Integration

**Best practice:** Use hooks for queries

```typescript
// React hook (using @fraiseql/react package)
import { useQuery } from '@fraiseql/react';

export function UserList() {
  const { data: users, loading, error } = useQuery(`
    query {
      users {
        id
        name
        email
      }
    }
  `);

  if (loading) return <div>Loading...</div>;
  if (error) return <div>Error: {error.message}</div>;

  return (
    <ul>
      {users.map(user => (
        <li key={user.id}>{user.name}</li>
      ))}
    </ul>
  );
}
```

### 3. Error Handling

**Best practice:** Specific error handling per use case

```typescript
import { gql } from '@fraiseql/core';

async function loginUser(email: string, password: string) {
  try {
    const result = await client.query(gql`
      mutation {
        login(email: "${email}", password: "${password}") {
          token
          user { id name }
        }
      }
    `);
    return result.login;
  } catch (error) {
    if (error.code === 'E_AUTH_INVALID_CREDENTIALS') {
      throw new Error('Invalid email or password');
    }
    if (error.code === 'E_AUTH_TOO_MANY_ATTEMPTS') {
      throw new Error('Too many login attempts. Try again later.');
    }
    throw error;  // Unknown error
  }
}
```

### 4. Caching Strategies

**Best practice:** Use Apollo Client or similar

```typescript
import { ApolloClient, InMemoryCache } from '@apollo/client';

const client = new ApolloClient({
  uri: 'http://localhost:5000/graphql',
  cache: new InMemoryCache(),
  // Cache automatically handles query deduplication
});

// Query 1: Fetches from server
const result1 = await client.query(GET_USERS_QUERY);

// Query 2: Same query, returned from cache (no server call)
const result2 = await client.query(GET_USERS_QUERY);
```

### 5. Subscription Handling

**Best practice:** Cleanup subscriptions properly

```typescript
import { useEffect, useState } from 'react';

export function LiveOrderCount() {
  const [count, setCount] = useState(0);

  useEffect(() => {
    const unsubscribe = client.subscribe(
      gql`
        subscription {
          orderCreated {
            id
            totalPrice
          }
        }
      `,
      {
        next: (data) => {
          setCount(prev => prev + 1);
        },
        error: (err) => console.error('Subscription error:', err),
        complete: () => console.log('Subscription complete'),
      }
    );

    // Cleanup on unmount
    return () => unsubscribe();
  }, []);

  return <div>Orders created: {count}</div>;
}
```

---

## Go Best Practices

### 1. Struct Tags & Validation

**Best practice:** Use struct tags for GraphQL field mapping

```go
package main

import "github.com/fraiseql/fraiseql-go"

type User struct {
    ID    string `graphql:"id"`
    Name  string `graphql:"name"`
    Email string `graphql:"email"`
}

type QueryResult struct {
    Users []User `graphql:"users"`
}
```

### 2. Context Management

**Best practice:** Always use context for cancellation

```go
ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
defer cancel()

result, err := client.Query(ctx, query)
if err != nil {
    if errors.Is(err, context.DeadlineExceeded) {
        log.Fatal("Query timeout")
    }
}
```

### 3. Error Handling

**Best practice:** Handle GraphQL errors and HTTP errors separately

```go
import "github.com/fraiseql/fraiseql-go"

result, err := client.Query(ctx, query)

// HTTP/network errors
if err != nil {
    log.Fatal("Network error:", err)
}

// GraphQL errors in response
if len(result.Errors) > 0 {
    for _, e := range result.Errors {
        log.Printf("GraphQL error: %s (code: %s)", e.Message, e.Extensions.Code)
    }
}
```

### 4. Batching & Connection Pooling

**Best practice:** Reuse single client

```go
// Initialize once
client := fraiseql.NewClient("http://localhost:5000")

// Reuse for multiple queries
users, _ := client.Query(ctx, getUsersQuery)
posts, _ := client.Query(ctx, getPostsQuery)
```

---

## Java Best Practices

### 1. Annotations & Decorators

**Best practice:** Use custom annotations for cleaner code

```java
@FraiseQLType
public class User {
    @Key
    private String id;

    @Authorize(roles = {"ADMIN", "SELF"})
    private String email;

    @Cache(ttlSeconds = 300)
    private String name;
}
```

### 2. Optional & Nullability

**Best practice:** Use Optional for nullable fields

```java
import java.util.Optional;

public class User {
    private String id;
    private String name;
    private Optional<String> middleName;  // May be null
    private Optional<String> nickname;    // May be null

    public String getMiddleName() {
        return middleName.orElse(null);
    }
}
```

### 3. Async/Reactive Programming

**Best practice:** Use CompletableFuture or Project Reactor

```java
// CompletableFuture (standard Java)
CompletableFuture<User> user = client.queryAsync(queryString);
user.thenApply(u -> u.getName())
    .thenAccept(System.out::println)
    .exceptionally(e -> {
        log.error("Query failed", e);
        return null;
    });

// Project Reactor (if using Spring)
Mono<User> user = client.queryMono(queryString);
user.subscribe(u -> log.info("User: {}", u.getName()));
```

### 4. Testing with JUnit 5

**Best practice:** Use TestContainers for integration tests

```java
@Container
static FraiseQLContainer fraiseql = new FraiseQLContainer("fraiseql:v2.0.0");

@BeforeEach
void setUp() {
    client = new FraiseQLClient(fraiseql.getGraphQLEndpoint());
}

@Test
void testGetUsers() throws Exception {
    List<User> users = client.query(GET_USERS_QUERY, User.class);
    assertTrue(users.size() > 0);
}
```

---

## Other Languages

### Quick Reference for Additional Languages

**Kotlin** (JVM ecosystem)
- Use data classes for type safety
- Leverage coroutines for async operations
- Use sealed classes for error handling

**Rust** (Compiled language)
- Use `tokio` for async runtime
- Leverage type system for exhaustive error handling
- Use `serde` for JSON serialization

**C#** (.NET ecosystem)
- Use async/await consistently
- Leverage nullable reference types (C# 8+)
- Use LINQ for query manipulation
- Implement IDisposable for resource cleanup

**Go** (See detailed section above)

**PHP** (Dynamic language)
- Use type hints for function parameters
- Leverage async libraries: ReactPHP, Amp
- Handle JSON errors explicitly

**Ruby** (Dynamic language)
- Use Bundler for dependency management
- Leverage Ruby idioms: duck typing, blocks
- Use rspec for testing

**Swift** (Apple ecosystem)
- Use async/await (Swift 5.5+)
- Leverage Codable for JSON decoding
- Use Result type for error handling

**Dart** (Flutter ecosystem)
- Use async/await with Future
- Leverage strong type system
- Use build_runner for code generation

---

## Common Anti-Patterns Across Languages

### Anti-Pattern 1: Ignoring Errors

```python
# ❌ Bad
result = await client.query(query)
users = result["users"]  # What if query failed?

# ✅ Good
try:
    result = await client.query(query)
    users = result["users"]
except Exception as e:
    log.error(f"Query failed: {e}")
    users = []
```

### Anti-Pattern 2: Creating New Client Per Request

```python
# ❌ Bad: Creates new connection
async def endpoint(request):
    client = AsyncClient(url="...")  # New client!
    return await client.query(query)

# ✅ Good: Reuse singleton client
client = AsyncClient(url="...")

async def endpoint(request):
    return await client.query(query)
```

### Anti-Pattern 3: Blocking in Async Context

```python
# ❌ Bad: Blocking call in async function
async def get_data():
    data = requests.get("...")  # BLOCKS!
    return data

# ✅ Good: Async call
async def get_data():
    async with aiohttp.ClientSession() as session:
        async with session.get("...") as resp:
            return await resp.json()
```

---

## See Also

**Language-Specific Docs:**
- **[Python SDK Reference](../integrations/sdk/python-reference.md)**
- **[TypeScript SDK Reference](../integrations/sdk/typescript-reference.md)**
- **[Go SDK Reference](../integrations/sdk/go-reference.md)**
- **[Java SDK Reference](../integrations/sdk/java-reference.md)**

**Related Guides:**
- **[Common Gotchas](./common-gotchas.md)** — Language-agnostic pitfalls
- **[Testing Strategy](./testing-strategy.md)** — Testing across languages
- **[Production Deployment](./production-deployment.md)** — Deploying polyglot apps
- **[Error Code Reference](../reference/error-codes.md)** — All error codes

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
