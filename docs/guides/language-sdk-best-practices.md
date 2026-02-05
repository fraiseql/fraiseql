<!-- Skip to main content -->
---
title: Language SDK Best Practices
description: Language-specific best practices and idioms for using FraiseQL with your preferred programming language. Select your language below.
keywords: ["debugging", "implementation", "best-practices", "deployment", "tutorial"]
tags: ["documentation", "reference"]
---

# Language SDK Best Practices

**Status:** ✅ Production Ready
**Audience:** Developers (all languages)
**Reading Time:** 20-25 minutes per language
**Last Updated:** 2026-02-05

Language-specific best practices and idioms for using FraiseQL with your preferred programming language. Select your language below.

---

## Overview

FraiseQL SDKs are available for **16 languages**. While the GraphQL API is universal, each language has idiomatic patterns and best practices. This guide covers the major languages; principles apply to others.

**Quick links to best practices:**

- [Python](#python-best-practices)
- [TypeScript/JavaScript](#typescriptjavascript-best-practices)
- [Go](#go-best-practices)
- [Java](#java-best-practices)
- [Other Languages](#other-languages)

**Comprehensive API References (All 16 Languages):**
👉 **[SDK Reference Documentation](../integrations/sdk/)** — Complete API reference for all 16 FraiseQL SDKs with installation, type systems, examples, and language-specific patterns for:

- **Primary**: Python, TypeScript, Go, Java
- **JVM**: Kotlin, Scala, Clojure, Groovy
- **Native**: Rust, C#, Swift
- **Dynamic**: PHP, Ruby, Dart, Elixir

---

## Python Best Practices

### 1. Type Annotations & Validation

**Best practice:** Use Python 3.10+ union syntax and Pydantic for validation

```python
<!-- Code example in Python -->
# ✅ Good: Modern type hints
from typing import Optional
from pydantic import BaseModel, EmailStr

class User(BaseModel):
    id: UUID  # UUID v4 for GraphQL ID
    name: str
    email: EmailStr
    age: int | None = None  # Python 3.10+ union syntax

# ❌ Avoid: Old Optional syntax
from typing import Optional
age: Optional[int] = None  # Older style
```text
<!-- Code example in TEXT -->

### 2. Async/Await Patterns

**Best practice:** Use `asyncio` consistently throughout the application

```python
<!-- Code example in Python -->
# ✅ Good: Async client with async context manager
from FraiseQL import AsyncClient

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
```text
<!-- Code example in TEXT -->

**Avoid blocking calls in async code:**

```python
<!-- Code example in Python -->
# ❌ Bad: Blocking I/O in async function
async def fetch_user(id: str):
    response = requests.get(...)  # BLOCKS!
    return response.json()

# ✅ Good: Use async HTTP library
async def fetch_user(id: str):
    async with aiohttp.ClientSession() as session:
        async with session.get(...) as response:
            return await response.json()
```text
<!-- Code example in TEXT -->

### 3. Error Handling

**Best practice:** Handle FraiseQL errors explicitly

```python
<!-- Code example in Python -->
from FraiseQL import AsyncClient, FraiseQLError

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
```text
<!-- Code example in TEXT -->

### 4. Connection Pooling

**Best practice:** Reuse single client instance

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

### 5. Testing

**Best practice:** Use pytest with fixtures

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

---

## TypeScript/JavaScript Best Practices

### 1. Type Safety with Generated Types

**Best practice:** Generate types from schema

```bash
<!-- Code example in BASH -->
# Generate types from FraiseQL schema
FraiseQL-codegen generate --schema schema.json --output types.ts
```text
<!-- Code example in TEXT -->

**Usage:**

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

### 2. React/Vue Integration

**Best practice:** Use hooks for queries

```typescript
<!-- Code example in TypeScript -->
// React hook (using @FraiseQL/react package)
import { useQuery } from '@FraiseQL/react';

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
```text
<!-- Code example in TEXT -->

### 3. Error Handling

**Best practice:** Specific error handling per use case

```typescript
<!-- Code example in TypeScript -->
import { gql } from '@FraiseQL/core';

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
```text
<!-- Code example in TEXT -->

### 4. Caching Strategies

**Best practice:** Use Apollo Client or similar

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

### 5. Subscription Handling

**Best practice:** Cleanup subscriptions properly

```typescript
<!-- Code example in TypeScript -->
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
```text
<!-- Code example in TEXT -->

---

## Go Best Practices

### 1. Struct Tags & Validation

**Best practice:** Use struct tags for GraphQL field mapping

```go
<!-- Code example in Go -->
package main

import "github.com/FraiseQL/FraiseQL-go"

type User struct {
    ID    string `graphql:"id"`
    Name  string `graphql:"name"`
    Email string `graphql:"email"`
}

type QueryResult struct {
    Users []User `graphql:"users"`
}
```text
<!-- Code example in TEXT -->

### 2. Context Management

**Best practice:** Always use context for cancellation

```go
<!-- Code example in Go -->
ctx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
defer cancel()

result, err := client.Query(ctx, query)
if err != nil {
    if errors.Is(err, context.DeadlineExceeded) {
        log.Fatal("Query timeout")
    }
}
```text
<!-- Code example in TEXT -->

### 3. Error Handling

**Best practice:** Handle GraphQL errors and HTTP errors separately

```go
<!-- Code example in Go -->
import "github.com/FraiseQL/FraiseQL-go"

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
```text
<!-- Code example in TEXT -->

### 4. Batching & Connection Pooling

**Best practice:** Reuse single client

```go
<!-- Code example in Go -->
// Initialize once
client := FraiseQL.NewClient("http://localhost:5000")

// Reuse for multiple queries
users, _ := client.Query(ctx, getUsersQuery)
posts, _ := client.Query(ctx, getPostsQuery)
```text
<!-- Code example in TEXT -->

---

## Java Best Practices

### 1. Annotations & Decorators

**Best practice:** Use custom annotations for cleaner code

```java
<!-- Code example in Java -->
@FraiseQLType
public class User {
    @Key
    private String id;

    @Authorize(roles = {"ADMIN", "SELF"})
    private String email;

    @Cache(ttlSeconds = 300)
    private String name;
}
```text
<!-- Code example in TEXT -->

### 2. Optional & Nullability

**Best practice:** Use Optional for nullable fields

```java
<!-- Code example in Java -->
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
```text
<!-- Code example in TEXT -->

### 3. Async/Reactive Programming

**Best practice:** Use CompletableFuture or Project Reactor

```java
<!-- Code example in Java -->
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
```text
<!-- Code example in TEXT -->

### 4. Testing with JUnit 5

**Best practice:** Use TestContainers for integration tests

```java
<!-- Code example in Java -->
@Container
static FraiseQLContainer FraiseQL = new FraiseQLContainer("FraiseQL:v2.0.0");

@BeforeEach
void setUp() {
    client = new FraiseQLClient(FraiseQL.getGraphQLEndpoint());
}

@Test
void testGetUsers() throws Exception {
    List<User> users = client.query(GET_USERS_QUERY, User.class);
    assertTrue(users.size() > 0);
}
```text
<!-- Code example in TEXT -->

---

## Other Languages

The following languages have complete SDK reference documentation available with the same level of detail as the major languages above. Click each language for comprehensive API documentation, examples, and best practices:

### JVM Ecosystem Languages

**[Kotlin Reference](../integrations/sdk/kotlin-reference.md)** — Modern JVM with data classes, coroutines, and null safety

- Use data classes for type safety
- Leverage coroutines for async operations
- Use sealed classes for error handling

**[Scala Reference](../integrations/sdk/scala-reference.md)** — Functional programming with case classes and type system

- Functional composition patterns
- Pattern matching and sealed traits
- Advanced type inference

**[Clojure Reference](../integrations/sdk/clojure-reference.md)** — Functional Lisp dialect with persistent data structures

- Immutable data structures
- REPL-driven development
- Spec validation

**[Groovy Reference](../integrations/sdk/groovy-reference.md)** — Dynamic JVM language with DSL capabilities

- Closures for clean DSLs
- Metaprogramming patterns
- 100% Java interoperability

### Compiled Native Languages

**[Rust Reference](../integrations/sdk/rust-reference.md)** — Memory-safe systems programming

- Zero-cost abstractions
- Type system for exhaustive error handling
- Async/await with tokio

**[C# Reference](../integrations/sdk/csharp-reference.md)** — .NET ecosystem with modern language features

- Nullable reference types (C# 11+)
- Records and pattern matching
- LINQ for data transformation
- Dependency injection patterns

**[Swift Reference](../integrations/sdk/swift-reference.md)** — Apple ecosystem with async/await

- Async/await (Swift 5.9+)
- Codable protocol for JSON handling
- SwiftUI integration
- Actors for thread safety

### Dynamic/Interpreted Languages

**[PHP Reference](../integrations/sdk/php-reference.md)** — Web-first language with PHP 8 attributes

- PHP 8.2+ attributes (#[Type], #[Field])
- Readonly classes and properties
- Laravel/Symfony integration

**[Ruby Reference](../integrations/sdk/ruby-reference.md)** — Expressive language with Rails integration

- Bundler for dependency management
- Ruby idioms: duck typing, blocks, mixins
- Active Record pattern integration
- RSpec for testing

**[Dart Reference](../integrations/sdk/dart-reference.md)** — Flutter ecosystem with null safety

- Null safety (required, ?, late)
- Flutter widget integration
- State management with Riverpod
- JSON serialization with json_serializable

**[Elixir Reference](../integrations/sdk/elixir-reference.md)** — Functional language for distributed systems

- OTP patterns and supervisors
- Pipe operator for composition
- Phoenix web framework integration
- ExUnit testing framework

---

## Common Anti-Patterns Across Languages

### Anti-Pattern 1: Ignoring Errors

```python
<!-- Code example in Python -->
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
```text
<!-- Code example in TEXT -->

### Anti-Pattern 2: Creating New Client Per Request

```python
<!-- Code example in Python -->
# ❌ Bad: Creates new connection
async def endpoint(request):
    client = AsyncClient(url="...")  # New client!
    return await client.query(query)

# ✅ Good: Reuse singleton client
client = AsyncClient(url="...")

async def endpoint(request):
    return await client.query(query)
```text
<!-- Code example in TEXT -->

### Anti-Pattern 3: Blocking in Async Context

```python
<!-- Code example in Python -->
# ❌ Bad: Blocking call in async function
async def get_data():
    data = requests.get("...")  # BLOCKS!
    return data

# ✅ Good: Async call
async def get_data():
    async with aiohttp.ClientSession() as session:
        async with session.get("...") as resp:
            return await resp.json()
```text
<!-- Code example in TEXT -->

---

## See Also

**SDK References:**

- **[SDK Reference Documentation](../integrations/sdk/)** — Comprehensive API reference for all 16 languages
  - [Python](../integrations/sdk/python-reference.md)
  - [TypeScript](../integrations/sdk/typescript-reference.md)
  - [Go](../integrations/sdk/go-reference.md)
  - [Java](../integrations/sdk/java-reference.md)
  - [Kotlin](../integrations/sdk/kotlin-reference.md)
  - [Scala](../integrations/sdk/scala-reference.md)
  - [Clojure](../integrations/sdk/clojure-reference.md)
  - [Groovy](../integrations/sdk/groovy-reference.md)
  - [Rust](../integrations/sdk/rust-reference.md)
  - [C#](../integrations/sdk/csharp-reference.md)
  - [Swift](../integrations/sdk/swift-reference.md)
  - [PHP](../integrations/sdk/php-reference.md)
  - [Ruby](../integrations/sdk/ruby-reference.md)
  - [Dart](../integrations/sdk/dart-reference.md)
  - [Elixir](../integrations/sdk/elixir-reference.md)

**Related Guides:**

- **[SDK Reference Documentation](../integrations/sdk/)** — Complete API reference for all 17 SDKs
- **[Getting Started Guide](../getting-started.md)** — Quick start guide for FraiseQL

---

**Last Updated:** 2026-02-05
**Version:** v2.0.0-alpha.1
