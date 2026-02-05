# FraiseQL SDK Reference Documentation

**Status**: Production-Ready | **Coverage**: 16 Languages | **Feature Parity**: 100%

Complete API reference documentation for all 16 FraiseQL language SDKs. Each SDK provides identical GraphQL authoring capabilities across different language paradigms.

## SDK Categories

FraiseQL provides **17 SDKs** across two categories:

### Runtime Clients (Execute Pre-Compiled Queries)

Use these to execute queries against a FraiseQL server at runtime:

| Runtime | Reference | Best For | Module System |
|---------|-----------|----------|---|
| **Node.js** | [nodejs-reference.md](./nodejs-reference.md) | REST/GraphQL APIs, Express apps | CommonJS + ESM |

### Authoring Languages (Define Schemas)

Use these to authoring GraphQL schemas that compile to optimized SQL:

### 🟢 Primary Languages (Recommended)

These are the most mature SDKs with comprehensive documentation and examples:

| Language | Reference | Best For | Paradigm |
|----------|-----------|----------|----------|
| **Python** | [python-reference.md](./python-reference.md) | Data pipelines, rapid development | Dynamic + Functional |
| **TypeScript** | [typescript-reference.md](./typescript-reference.md) | Web apps, Node.js services | Static typing + Functional |
| **Go** | [go-reference.md](./go-reference.md) | Cloud infrastructure, microservices | Systems programming |
| **Java** | [java-reference.md](./java-reference.md) | Enterprise applications, JVM ecosystem | Static + Object-Oriented |

### 🟡 JVM Ecosystem Languages

Mature SDKs for JVM platform, compatible with Java tooling:

| Language | Reference | Best For | Paradigm |
|----------|-----------|----------|----------|
| **Kotlin** | [kotlin-reference.md](./kotlin-reference.md) | Modern Android, JVM services | Modern + Interoperable |
| **Scala** | [scala-reference.md](./scala-reference.md) | Functional data pipelines, type safety | Functional + Static |
| **Clojure** | [clojure-reference.md](./clojure-reference.md) | Data processing, dynamic typing | Functional + Dynamic |
| **Groovy** | [groovy-reference.md](./groovy-reference.md) | Scripting, DSL creation, testing | Dynamic + Scripting |

### 🔵 Native/Systems Languages

High-performance SDKs for compiled languages and systems programming:

| Language | Reference | Best For | Paradigm |
|----------|-----------|----------|----------|
| **Rust** | [rust-reference.md](./rust-reference.md) | Systems, embedded, zero-copy | Memory-safe systems |
| **C#** | [csharp-reference.md](./csharp-reference.md) | .NET ecosystem, Azure services | Static + Object-Oriented |
| **Swift** | [swift-reference.md](./swift-reference.md) | iOS/macOS apps, Apple ecosystem | Static + Functional |

### 🟣 Dynamic/Interpreted Languages

Production-ready SDKs for dynamic languages and scripting:

| Language | Reference | Best For | Paradigm |
|----------|-----------|----------|----------|
| **PHP** | [php-reference.md](./php-reference.md) | Web services, Laravel/Symfony | Dynamic + Web-first |
| **Ruby** | [ruby-reference.md](./ruby-reference.md) | Web frameworks, Rails integration | Dynamic + Expressive |
| **Dart** | [dart-reference.md](./dart-reference.md) | Flutter mobile apps, web (WASM) | Static + Multi-platform |
| **Elixir** | [elixir-reference.md](./elixir-reference.md) | Distributed systems, real-time apps | Functional + Distributed |

## Feature Parity

**Authoring Languages** (16 SDKs): All implement **100% feature parity** (480/480 features):

| Feature Category | Count | Status |
|------------------|-------|--------|
| Type Decorators | 30 | ✅ All 16 Authoring SDKs |
| Query Operations | 30 | ✅ All 16 Authoring SDKs |
| Mutation Operations | 30 | ✅ All 16 Authoring SDKs |
| Analytics/OLAP | 30 | ✅ All 16 Authoring SDKs |
| Observers/Webhooks | 30 | ✅ All 16 Authoring SDKs |
| Security/RBAC | 30 | ✅ All 16 Authoring SDKs |
| Configuration | 30 | ✅ All 16 Authoring SDKs |
| Error Handling | 30 | ✅ All 16 Authoring SDKs |
| Type System | 30 | ✅ All 16 Authoring SDKs |
| Field Metadata | 30 | ✅ All 16 Authoring SDKs |
| Subscriptions | 30 | ✅ All 16 Authoring SDKs |
| Validators | 30 | ✅ All 16 Authoring SDKs |
| Middleware | 30 | ✅ All 16 Authoring SDKs |
| Lifecycle Hooks | 30 | ✅ All 16 Authoring SDKs |
| Performance | 30 | ✅ All 16 Authoring SDKs |
| Testing | 30 | ✅ All 16 Authoring SDKs |
| **TOTAL** | **480** | ✅ All 16 Complete |

**Runtime Clients** (Node.js): Support all execution capabilities (queries, mutations, subscriptions, batch operations, type validation)

## Reference Document Structure

### Authoring Language References (Python, TypeScript, Go, Java, etc.)

Each document covers schema definition for compilation:

1. **Header** - Language info, installation, requirements
2. **Quick Reference Table** - Decorator/annotation API summary
3. **Type System** - Decorators/Annotations, nullability, generics
4. **Operations** - Queries, mutations, subscriptions definitions
5. **Advanced Features** - Analytics, observers, security, field metadata
6. **Scalar Types** - Reference to 60+ scalar types with examples
7. **Schema Export** - Compilation workflow and TOML configuration
8. **Type Mapping** - Language types ↔ GraphQL types
9. **Common Patterns** - CRUD, pagination, filtering, multi-tenancy
10. **Error Handling** - Exception/error types and handling
11. **Testing** - Unit test patterns and schema validation
12. **See Also** - Links to related documentation

### Runtime Client References (Node.js)

Each document covers query execution against deployed servers:

1. **Header** - Runtime info, installation, requirements
2. **Quick Reference Table** - Method/function API summary
3. **Client Initialization** - Connection setup and configuration
4. **Type System & Validation** - Runtime type checking and JSDoc
5. **Operations** - Query, mutation, subscription execution
6. **Advanced Features** - Analytics queries, field metadata, RBAC
7. **Scalar Types** - Node.js type mappings with examples
8. **Framework Integration** - Express.js, REST/GraphQL endpoints
9. **Error Handling** - Error types and handling patterns
10. **Testing** - Jest/Mocha integration test patterns
11. **Common Patterns** - CRUD, pagination, multi-tenancy
12. **See Also** - Links to related documentation

## Quick Start by Language

### Python

```python
import FraiseQL

@FraiseQL.type
class User:
    id: int
    name: str

@FraiseQL.query(sql_source="v_users")
def users(limit: int = 10) -> list[User]:
    pass

FraiseQL.export_schema("schema.json")
```text

→ [Full Python Reference](./python-reference.md)

### TypeScript

```typescript
import * as FraiseQL from 'FraiseQL';

@FraiseQL.Type
class User {
  id: number;
  name: string;
}

@FraiseQL.Query({ sqlSource: 'v_users' })
function users(limit: number = 10): User[] {
  // Auto-generated
}

FraiseQL.exportSchema('schema.json');
```text

→ [Full TypeScript Reference](./typescript-reference.md)

### Go

```go
package main

import "FraiseQL"

type User struct {
    ID   int    `FraiseQL:"id"`
    Name string `FraiseQL:"name"`
}

func Users(limit int) ([]User, error) {
    return FraiseQL.Query(...)
}

FraiseQL.ExportSchema("schema.json")
```text

→ [Full Go Reference](./go-reference.md)

### Java

```java
import com.FraiseQL.*;

@GraphQLType
public class User {
    @GraphQLField
    public int id;

    @GraphQLField
    public String name;
}

FraiseQL.query("users")
    .returnType(User.class)
    .arg("limit", "Int")
    .register();

FraiseQL.exportSchema("schema.json");
```text

→ [Full Java Reference](./java-reference.md)

### Node.js (Runtime Client)

```javascript
const { FraiseQLClient } = require('FraiseQL-nodejs');

const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: {
    type: 'postgres',
    host: 'localhost',
    port: 5432,
    database: 'fraiseql_db',
    user: 'postgres',
    password: process.env.DB_PASSWORD,
  },
});

await client.connect();
const result = await client.query('users', { limit: 10 });
console.log(result.data);
await client.disconnect();
```text

→ [Full Node.js Reference](./nodejs-reference.md)

## Common Workflows

### Schema Authoring Workflow (16 Authoring Languages)

**Step 1: Define Types**

Each authoring SDK provides a way to define GraphQL types using native language syntax:

- **Python/TypeScript/Go/Rust**: Decorators or attributes
- **Java/Kotlin**: Annotations
- **Scala/Clojure**: Macros or functions
- **Ruby/PHP/Elixir**: Modules or classes

See your language reference for specific syntax.

**Step 2: Define Queries & Mutations**

Queries are read operations, mutations are write operations. Both are type-safe and map to SQL views/functions:

- **Queries**: `@FraiseQL.query` → SELECT from view
- **Mutations**: `@FraiseQL.mutation` → CALL function
- **Subscriptions**: `@FraiseQL.subscription` → Topic-based events

**Step 3: Export Schema**

Each authoring SDK exports schema to JSON format:

```bash
# Python
python -m FraiseQL export schema.json

# TypeScript
npx FraiseQL export schema.json

# Go
go run cmd/export/main.go schema.json

# Java
java -cp FraiseQL.jar com.FraiseQL.CLI export schema.json
```text

**Step 4: Compile & Deploy**

Compile the schema once:

```bash
FraiseQL-cli compile schema.json FraiseQL.toml
# Outputs: schema.compiled.json
```text

Deploy `schema.compiled.json` with your FraiseQL server.

### Runtime Execution Workflow (Node.js)

**Step 1: Load Compiled Schema**

Initialize client with pre-compiled schema:

```javascript
const client = new FraiseQLClient({
  schemaPath: './schema.compiled.json',
  database: { /* ... */ },
});
```text

**Step 2: Execute Queries**

Run queries, mutations, and subscriptions at runtime:

```javascript
const result = await client.query('users', { limit: 10 });
```text

See [Node.js SDK Reference](./nodejs-reference.md) for complete runtime API.

## Schema Compilation Workflow

```text
┌─────────────────────┐
│ Language SDK        │
│ (Python/TS/Go/...)  │
├─────────────────────┤
│ @types              │
│ @queries            │
│ @mutations          │
│ @fact_tables        │
│ @observers          │
└──────────┬──────────┘
           │ export
           ↓
┌─────────────────────┐
│ schema.json         │
│ (GraphQL types)     │
└──────────┬──────────┘
           │ compile
           ↓
┌─────────────────────────────────┐
│ schema.compiled.json            │
│ (types + queries + SQL + config)│
└──────────┬──────────────────────┘
           │ deploy
           ↓
┌────────────────────────────────┐
│ FraiseQL-server                │
│ (REST/GraphQL API)             │
└────────────────────────────────┘
```text

## Language-Specific Paradigms

### Python 🐍

**Paradigm**: Dynamic + Functional
**Key Syntax**: Decorators (`@FraiseQL.type`, `@FraiseQL.query`)
**Type Hints**: Optional but recommended (PEP 484)
**Package**: `FraiseQL` via PyPI

Best for: Data pipelines, rapid prototyping, Django/FastAPI projects

### TypeScript ✨

**Paradigm**: Static + Functional
**Key Syntax**: Decorators (`@Type`, `@Query`)
**Type Safety**: Full TypeScript type checking
**Package**: `@FraiseQL/SDK` via npm

Best for: React/Vue web apps, NestJS backends, type safety

### Go 🦫

**Paradigm**: Systems Programming
**Key Syntax**: Struct tags + builder pattern
**Concurrency**: First-class goroutines
**Package**: `github.com/FraiseQL/SDK-go`

Best for: Cloud infrastructure, microservices, CLI tools

### Java ☕

**Paradigm**: Static + Object-Oriented
**Key Syntax**: Annotations (`@GraphQLType`)
**Ecosystem**: Maven/Gradle integration
**Package**: `com.FraiseQL:FraiseQL-SDK`

Best for: Enterprise apps, Spring Boot, Kafka integrations

### Kotlin 🔥

**Paradigm**: Modern + Interoperable
**Key Syntax**: Annotations + sealed classes
**Null Safety**: Nullable types built-in
**Package**: `com.FraiseQL:FraiseQL-kotlin-SDK`

Best for: Android apps, modern JVM services, coroutines

### Scala 🎯

**Paradigm**: Functional + Static
**Key Syntax**: Case classes + macros
**Type System**: Advanced type inference
**Package**: `com.FraiseQL %% FraiseQL-scala-SDK`

Best for: Data processing pipelines, Spark jobs, type-safe FP

### Clojure 🤡

**Paradigm**: Functional + Dynamic
**Key Syntax**: Keywords + maps
**REPL-Driven**: Interactive development
**Package**: `FraiseQL/SDK-clojure`

Best for: Data transformation, domain modeling, REPL workflows

### Groovy 🌳

**Paradigm**: Dynamic + Scripting
**Key Syntax**: Closures + builders
**Compatibility**: 100% Java compatible
**Package**: `com.FraiseQL:FraiseQL-groovy-SDK`

Best for: Gradle plugins, test frameworks, DSL creation

### Rust 🦀

**Paradigm**: Memory-Safe Systems
**Key Syntax**: Macros + traits
**Performance**: Zero-cost abstractions
**Package**: `FraiseQL` on crates.io

Best for: Systems programming, WebAssembly, embedded systems

### C# #️⃣

**Paradigm**: Static + Object-Oriented
**Key Syntax**: Records + attributes
**Async**: First-class async/await
**Package**: `FraiseQL` via NuGet

Best for: .NET applications, Azure services, Windows integration

### Swift 🍎

**Paradigm**: Static + Functional
**Key Syntax**: Structs + protocols
**Apple Ecosystem**: iOS/macOS/watchOS
**Package**: `FraiseQL` via SPM

Best for: iOS/macOS apps, Apple platform integration

### PHP 🐘

**Paradigm**: Dynamic + Web-first
**Key Syntax**: PHP 8 attributes
**Frameworks**: Laravel, Symfony compatible
**Package**: `FraiseQL/SDK` via Composer

Best for: Web services, Laravel/Symfony projects, PHP ecosystem

### Ruby 💎

**Paradigm**: Dynamic + Expressive
**Key Syntax**: Modules + constants
**Metaprogramming**: Powerful DSL creation
**Package**: `FraiseQL` via RubyGems

Best for: Rails projects, web frameworks, rapid development

### Dart 🎯

**Paradigm**: Static + Multi-platform
**Key Syntax**: Annotations + builders
**Flutter**: First-class support
**Package**: `FraiseQL` via pub.dev

Best for: Flutter mobile apps, web (WASM), Dart ecosystem

### Elixir 💧

**Paradigm**: Functional + Distributed
**Key Syntax**: Atoms + maps
**OTP**: Built-in distributed computing
**Package**: `:FraiseQL` via Hex

Best for: Real-time apps, distributed systems, Phoenix web apps

---

### Node.js 🟢 (Runtime Client)

**Type**: Runtime execution client (not authoring)
**Paradigm**: Async Promise-based
**Module Systems**: CommonJS + ESM (dual package)
**Type Safety**: Optional TypeScript support
**Package**: `FraiseQL-nodejs` via npm

Best for: REST/GraphQL APIs, Express servers, microservices, real-time apps

Key capabilities:

- Promise-based async query execution
- Dynamic runtime type validation
- Support for both CommonJS and ESM
- Express.js integration ready
- WebSocket subscriptions
- Batch operations and transactions

## Cross-Language Comparison

### Type Definition Syntax

| Language | Syntax | Example |
|----------|--------|---------|
| **Python** | `@FraiseQL.type class` | `@FraiseQL.type class User: id: int` |
| **TypeScript** | `@Type class` | `@Type class User { id: number; }` |
| **Go** | Struct tags | `type User struct { ID int \`FraiseQL:"id"\` }` |
| **Java** | `@GraphQLType` | `@GraphQLType public class User { ... }` |
| **Kotlin** | Data class + `@Type` | `@Type data class User(val id: Int)` |
| **Scala** | Case class + `@Type` | `@Type case class User(id: Int)` |
| **Clojure** | `defschema` macro | `(defschema User {:id Int})` |
| **Groovy** | `@GraphQLType` class | `@GraphQLType class User { int id }` |
| **Rust** | `#[type]` macro struct | `#[type] struct User { id: i32 }` |
| **C#** | `[GraphQLType]` record | `[GraphQLType] record User(int Id);` |
| **Swift** | `@Type` struct | `@Type struct User { let id: Int }` |
| **PHP** | `#[Type]` class | `#[Type] class User { public int $id; }` |
| **Ruby** | `fraiseql_type` block | `fraiseql_type :User do { ... }` |
| **Dart** | `@GraphQLType` class | `@GraphQLType() class User { final int id; }` |
| **Elixir** | `defschema` | `defschema User, id: :integer` |

### Query Definition Syntax

| Language | Syntax | Example |
|----------|--------|---------|
| **Python** | `@FraiseQL.query()` | `@FraiseQL.query(sql_source="v_users")` |
| **TypeScript** | `@Query()` | `@Query({ sqlSource: 'v_users' })` |
| **Go** | Builder pattern | `FraiseQL.Query("users").From("v_users")` |
| **Java** | Builder chain | `FraiseQL.query("users").returnType(User.class)` |
| **Kotlin** | Extension function | `query("users").returns(User::class)` |
| **Scala** | Object method | `Query("users").returns[User]` |
| **Clojure** | `defquery` macro | `(defquery users (schema User))` |
| **Groovy** | `@Query` method | `@Query(sqlSource="v_users")` |
| **Rust** | `#[query]` macro | `#[query(sql_source = "v_users")]` |
| **C#** | `[Query]` method | `[Query("sql_source='v_users'")]` |
| **Swift** | `@Query` property | `@Query(sqlSource: "v_users")` |
| **PHP** | `#[Query]` attribute | `#[Query(sqlSource: 'v_users')]` |
| **Ruby** | `fraiseql_query` block | `fraiseql_query :users do { ... }` |
| **Dart** | `@Query()` method | `@Query(sqlSource: 'v_users')` |
| **Elixir** | `defquery` macro | `defquery :users, :schema => User` |

## Resources

### Documentation

- [Language SDK Best Practices](../../guides/language-sdk-best-practices.md) - Cross-language patterns and idioms
- [FraiseQL Getting Started Guide](../../GETTING_STARTED.md) - Quick start for all languages
- **GitHub Repository**: [FraiseQL/FraiseQL](https://github.com/FraiseQL/FraiseQL) - Source code and examples

### Getting Help

- **Issues**: [GitHub Issues](https://github.com/FraiseQL/FraiseQL/issues) - Report bugs or request features
- **Discussions**: [GitHub Discussions](https://github.com/FraiseQL/FraiseQL/discussions) - Ask questions and share ideas
- **Stack Overflow**: Tag questions with `FraiseQL`
- **Community**: [Discord Server](https://discord.gg/FraiseQL) - Join community discussions

## Contributing

To contribute improvements to SDK documentation:

1. Review the SDK reference document structure above
2. Test all code examples with current SDK versions
3. Ensure consistency with the other 16 language SDK references
4. Maintain 100% feature parity across all languages
5. Submit a pull request with clear description of changes

## License

All SDK reference documentation is licensed under the MIT License. See [LICENSE](../../../LICENSE) for details.

---

**Last Updated**: 2026-02-05
**Maintained By**: FraiseQL Community
**Status**: Production Ready ✅
