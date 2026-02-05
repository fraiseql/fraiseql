# FraiseQL v2 - Java Schema Authoring

> Compiled GraphQL execution engine - Schema authoring in Java

FraiseQL v2 is a high-performance GraphQL engine that compiles schemas at build-time for zero-cost query execution. This package provides **schema authoring in Java** that generates JSON schemas consumed by the Rust compiler.

**Key Principle**: Java is for **authoring only** - no runtime FFI, no language bindings. Just pure JSON generation.

## Architecture

```
Java Code (annotations + builders)
         ↓
    schema.json
         ↓
 fraiseql-cli compile
         ↓
 schema.compiled.json
         ↓
 Rust Runtime (fraiseql-server)
```

## Installation

### Requirements

- Java 17 or later
- Maven 3.8.0 or later

### Add Dependency

```xml
<dependency>
    <groupId>com.fraiseql</groupId>
    <artifactId>fraiseql-java</artifactId>
    <version>1.0.0</version>
</dependency>
```

Or with Gradle:

```gradle
implementation 'com.fraiseql:fraiseql-java:1.0.0'
```

## Quick Start

### 1. Define Types

```java
package com.example;

import com.fraiseql.core.*;

@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;

    @GraphQLField
    public String email;

    @GraphQLField(name = "created_at")
    public String createdAt;
}
```

### 2. Define Queries

```java
QueryBuilder usersQuery = FraiseQL.query("users")
    .returnType(User.class)
    .returnsArray(true)
    .arg("limit", "Int", 10)
    .arg("offset", "Int", 0)
    .description("Get all users with pagination");
```

### 3. Define Mutations

```java
MutationBuilder createUser = FraiseQL.mutation("createUser")
    .returnType(User.class)
    .arg("name", "String", null)
    .arg("email", "String", null)
    .description("Create a new user");
```

### 4. Export Schema

```java
// Register types
FraiseQL.registerTypes(User.class, Post.class);

// Export schema to JSON
FraiseQL.exportSchema("schema.json");
```

## Type System

Java types map to GraphQL types:

| Java Type | GraphQL Type | Nullable |
|-----------|-------------|----------|
| `int`, `Integer` | `Int` | No |
| `long`, `Long` | `Int` | No |
| `float`, `Float` | `Float` | No |
| `double`, `Double` | `Float` | No |
| `String` | `String` | No |
| `boolean`, `Boolean` | `Boolean` | No |
| `LocalDate`, `LocalDateTime` | `String` | No |
| `List<T>`, `Set<T>` | `[T]` | No |
| `Optional<T>` | `T` | Yes |

### Annotations

**@GraphQLType**: Marks a class as a GraphQL type

```java
@GraphQLType(description = "A user account")
public class User { ... }
```

**@GraphQLField**: Marks a field as a GraphQL field

```java
@GraphQLField(name = "user_id", nullable = true)
public int id;
```

## Features

- **Type-Safe**: Java types map directly to GraphQL types
- **Annotation-Based**: Use annotations for zero-boilerplate schema definition
- **Compile-Time**: All validation happens at build time
- **No FFI**: Pure JSON output, no Java-Rust bindings needed
- **Builder Pattern**: Fluent API for defining queries and mutations
- **Reflection-Based**: Automatic field extraction from annotated classes
- **Analytics Ready**: Support for fact tables and OLAP workloads

## Examples

### Basic Schema

See `examples/BasicSchema.java` for a complete example with Users and Posts.

Run:

```bash
mvn exec:java -Dexec.mainClass="com.fraiseql.examples.BasicSchema"
```

## Project Structure

```
fraiseql-java/
├── src/
│   ├── main/java/com/fraiseql/
│   │   ├── core/                # Core type conversion and annotations
│   │   ├── registry/            # Schema registry
│   │   ├── builders/            # Query/Mutation builders
│   │   └── analytics/           # Fact tables
│   └── test/java/com/fraiseql/  # Unit tests
├── pom.xml                       # Maven configuration
└── README.md                     # This file
```

## Test Coverage

FraiseQL Java includes comprehensive test suites covering all major features:

### Test Suites

| Suite | Tests | Coverage |
|-------|-------|----------|
| **TypeSystemTest** | 18 | Type registration, field extraction, type conversion |
| **OperationsTest** | 13 | Query, mutation, subscription builders |
| **FieldMetadataTest** | 15 | Field descriptions, nullability, custom names |
| **ObserverTest** | 13 | Event observers, webhooks, Slack, email, retry config |
| **SubscriptionTest** | 10 | Subscription filtering, topics, operations |
| **ParityTest** | 12 | Java ↔ TypeScript/Python feature equivalence |
| **AnalyticsTest** | 10 | Analytics patterns, aggregations, fact tables |
| **Phase2Test** | 19 | Type info, schema registry, field extraction |
| **Phase3Test** | 17 | Schema formatting, JSON export |
| **Phase4–6Tests** | ~30 | Integration, advanced, optimization features |

**Total: 137+ tests** with 100% pass rate

### Run Tests

```bash
# Run all tests
mvn test

# Run specific test class
mvn test -Dtest=TypeSystemTest

# Run with verbose output
mvn test -X
```

## Development

### Build

```bash
mvn clean compile
```

### Package

```bash
mvn package
```

## Next Steps

After generating `schema.json`:

1. **Compile schema:**

   ```bash
   fraiseql-cli compile schema.json -o schema.compiled.json
   ```

2. **Start server:**

   ```bash
   fraiseql-server --schema schema.compiled.json --port 8000
   ```

3. **Test GraphQL:**

   ```bash
   curl -X POST http://localhost:8000/graphql \
     -H "Content-Type: application/json" \
     -d '{"query":"{ users(limit:10) { id name email } }"}'
   ```

## Implementation Phases

- ✅ Phase 1: Project setup and infrastructure
- ⏳ Phase 2: Type system and conversion
- ⏳ Phase 3: Registry and builders
- ⏳ Phase 4: Type registration and export
- ⏳ Phase 5: Analytics support
- ⏳ Phase 6: Examples and documentation
- ⏳ Phase 7: Testing and packaging

## License

Apache License 2.0 - See LICENSE file for details

## See Also

- [FraiseQL v2 (Rust core)](https://github.com/fraiseql/fraiseql)
- [Go Authoring Layer](https://github.com/fraiseql/fraiseql-go)
