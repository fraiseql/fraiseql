# FraiseQL Documentation

Welcome to the FraiseQL documentation! FraiseQL is a complete GraphQL API framework that provides a strongly-typed GraphQL-to-PostgreSQL translator with built-in FastAPI integration, authentication, and production optimizations.

## What is FraiseQL?

FraiseQL revolutionizes GraphQL API development by leveraging PostgreSQL's JSONB capabilities and database views. Each entity in your system has a corresponding view that provides data in JSON format, allowing FraiseQL's query builder to efficiently select only the fields requested by the GraphQL client. This approach eliminates N+1 queries and provides optimal performance out of the box.

### Key Benefits

- **Zero N+1 Queries**: Smart SQL generation with automatic joins and subqueries
- **Type Safety**: Full Python type hints with runtime validation
- **SQL Injection Protection**: All queries use parameterized SQL for complete security
- **Production Ready**: Separate development and production routers for optimal performance
- **Database First**: Leverages PostgreSQL's power with JSONB and views
- **Drop-in Replacement**: Compatible with Strawberry GraphQL APIs

## Documentation Overview

### 🚀 [Getting Started](./getting-started/index.md)
New to FraiseQL? Start here to learn the basics and get your first API up and running.

- [Installation](./getting-started/installation.md)
- [Quick Start](./getting-started/quickstart.md)
- [First API](./getting-started/first-api.md)

### 📚 [Core Concepts](./core-concepts/index.md)
Understand the fundamental concepts and architecture of FraiseQL.

- [Architecture Overview](./core-concepts/architecture.md)
- [Type System](./core-concepts/type-system.md)
- [Database Views](./core-concepts/database-views.md)
- [Query Translation](./core-concepts/query-translation.md)

### 📖 [API Reference](./api-reference/index.md)
Detailed reference for all FraiseQL APIs and decorators.

- [Decorators](./api-reference/decorators.md)
- [Field Types](./api-reference/field-types.md)
- [Scalars](./api-reference/scalars.md)
- [Mutations](./api-reference/mutations.md)
- [TestFoundry](./api-reference/testfoundry.md) - Automated test generation

### 🎓 [Tutorials](./tutorials/index.md)
Learn by building real-world applications with FraiseQL.

- [Building a Blog API](./tutorials/blog-api.md)
- [E-commerce Backend](./tutorials/ecommerce.md)
- [Real-time Chat](./tutorials/chat-app.md)

### 🔧 [Advanced Topics](./advanced/index.md)
Deep dive into advanced features and optimization techniques.

- [Authentication](./advanced/authentication.md)
- [Performance Optimization](./advanced/performance.md)
- [Custom Scalars](./advanced/custom-scalars.md)
- [Testing Strategies](./advanced/testing.md)

### 🔄 [Migration Guide](./migration/index.md)
Migrating from Strawberry or other GraphQL frameworks? We've got you covered.

- [From Strawberry](./migration/from-strawberry.md)
- [From GraphQL](./migration/from-graphql.md)

### 📊 [Comparisons](./comparisons/index.md)
See how FraiseQL compares to other GraphQL and API solutions.

- [FraiseQL vs Alternatives](./comparisons/alternatives.md) - Compare with Hasura, PostGraphile, Strawberry, and more

## Quick Example

```python
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    id: int
    name: str = fraise_field(description="User's full name")
    email: str = fraise_field(description="User's email address")
    posts: list["Post"] = fraise_field(description="User's blog posts")

@fraiseql.type
class Post:
    id: int
    title: str
    content: str
    author: User = fraise_field(description="Post author")

# Create FastAPI app with GraphQL endpoint
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
)
```

## Community

- [GitHub](https://github.com/fraiseql/fraiseql)
- [Discord](https://discord.gg/fraiseql)
- [Twitter](https://twitter.com/fraiseql)

## License

FraiseQL is MIT licensed. See [LICENSE](https://github.com/fraiseql/fraiseql/blob/main/LICENSE) for details.
