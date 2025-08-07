---
← [Home](../index.md) | [Documentation](../index.md#quick-navigation) | [Next: Decorators](decorators.md) →
---

# API Reference

> **In this section:** Complete API documentation for FraiseQL
> **Prerequisites:** Basic FraiseQL knowledge from [quickstart](../getting-started/quickstart.md)
> **Reference:** Full API details for all features

Complete reference documentation for all FraiseQL APIs, decorators, types, and utilities.

## Quick Navigation

### Core APIs

| Category | Description | Most Common |
|----------|-------------|-------------|
| [**Decorators**](decorators.md) | Schema definition decorators | `@fraiseql.type`, `@fraiseql.query`, `@fraiseql.mutation` |
| [**Repository**](repository.md) | Database access methods | `find()`, `find_one()`, `call_function()` |
| [**Types**](types.md) | Built-in GraphQL types | `ID`, `EmailAddress`, `UUID`, `JSON` |
| [**Context**](context.md) | Request context and info | `info.context`, resolver info |
| [**Errors**](errors.md) | Error handling and codes | Exception types, error codes |
| [**Utilities**](utilities.md) | CLI and helpers | `fraiseql init`, `fraiseql dev` |

## Getting Started with the API

### Basic Pattern

All FraiseQL APIs follow a consistent pattern:

```python
import fraiseql
from fraiseql import ID, FraiseQL

# 1. Define types with decorators
@fraiseql.type  # or @fraiseql.fraise_type
class User:
    id: ID
    name: str
    email: str

# 2. Define queries/mutations
@fraiseql.query
async def users(info) -> list[User]:
    repo = info.context["repo"]
    return await repo.find("v_user")

# 3. Initialize app
app = FraiseQL(database_url="postgresql://...")
```

### Decorator Syntax

FraiseQL supports two decorator naming conventions:

```python
# Standard (recommended)
@fraiseql.type
@fraiseql.query
@fraiseql.mutation

# Alternative (fraise_ prefix)
@fraiseql.fraise_type
@fraiseql.fraise_query
@fraiseql.fraise_mutation
```

Both work identically - choose based on your preference.

## Common Tasks

### Define a GraphQL Type
```python
@fraiseql.type
class Product:
    id: ID
    name: str
    price: float
    in_stock: bool
```
[Learn more →](decorators.md#type)

### Query Database
```python
@fraiseql.query
async def products(info) -> list[Product]:
    repo = info.context["repo"]
    return await repo.find("v_product")
```
[Learn more →](repository.md#find)

### Create Mutations
```python
@fraiseql.mutation
async def create_product(info, name: str, price: float) -> Product:
    repo = info.context["repo"]
    product_id = await repo.call_function(
        "fn_create_product",
        p_name=name,
        p_price=price
    )
    return await repo.find_one("v_product", where={"id": product_id})
```
[Learn more →](decorators.md#mutation)

### Handle Errors
```python
from fraiseql import GraphQLError

@fraiseql.query
async def product(info, id: ID) -> Product:
    repo = info.context["repo"]
    result = await repo.find_one("v_product", where={"id": id})
    if not result:
        raise GraphQLError(f"Product {id} not found", code="NOT_FOUND")
    return Product(**result)
```
[Learn more →](errors.md)

## API Categories

### Schema Definition
- [`@fraiseql.type`](decorators.md#type) - Define GraphQL types
- [`@fraiseql.input`](decorators.md#input) - Define input types
- [`@fraiseql.query`](decorators.md#query) - Define queries
- [`@fraiseql.mutation`](decorators.md#mutation) - Define mutations
- [`@fraiseql.subscription`](decorators.md#subscription) - Define subscriptions
- [`@fraiseql.field`](decorators.md#field) - Computed fields
- [`@fraiseql.dataloader_field`](decorators.md#dataloader_field) - Batched fields

### Database Access
- [`find()`](repository.md#find) - Query multiple records
- [`find_one()`](repository.md#find_one) - Query single record
- [`call_function()`](repository.md#call_function) - Call PostgreSQL functions
- [`execute()`](repository.md#execute) - Execute raw SQL
- [`transaction()`](repository.md#transaction) - Transaction management

### Type System
- [`ID`](types.md#id) - GraphQL ID scalar
- [`EmailAddress`](types.md#emailaddress) - Email validation
- [`UUID`](types.md#uuid) - UUID type
- [`JSON`](types.md#json) - JSON/JSONB data
- [Network types](types.md#network-types) - IPv4, IPv6, CIDR, MAC
- [Custom scalars](types.md#custom-scalars) - Create your own

### Context & Info
- [`info.context`](context.md#context) - Request context
- [`info.field_name`](context.md#field_name) - Current field
- [`info.return_type`](context.md#return_type) - Return type info
- [Authentication](context.md#authentication) - User context

### Error Handling
- [`GraphQLError`](errors.md#graphqlerror) - Standard errors
- [`ValidationError`](errors.md#validationerror) - Input validation
- [Error codes](errors.md#error-codes) - Standard codes
- [Custom errors](errors.md#custom-errors) - Domain errors

## Requirements

- **Python**: 3.11 or higher
- **PostgreSQL**: 14+ (with JSONB support)
- **Dependencies**: See [pyproject.toml](https://github.com/fraiseql/fraiseql/blob/main/pyproject.toml) for full list

## Need Help?

- [Quickstart Guide](../getting-started/quickstart.md) - Get started in 5 minutes
- [Core Concepts](../core-concepts/index.md) - Understand the architecture
- [Tutorials](../tutorials/index.md) - Step-by-step guides
- [GitHub Issues](https://github.com/fraiseql/fraiseql/issues) - Report bugs or request features

## See Also

### Essential Reading
- [**Getting Started**](../getting-started/index.md) - Begin with FraiseQL
- [**Core Concepts**](../core-concepts/index.md) - Understand the philosophy
- [**Type System**](../core-concepts/type-system.md) - GraphQL type definitions

### Practical Examples
- [**Quickstart**](../getting-started/quickstart.md) - 5-minute tutorial
- [**Your First API**](../getting-started/first-api.md) - User management example
- [**Blog Tutorial**](../tutorials/blog-api.md) - Complete application

### Advanced Topics
- [**Performance**](../advanced/performance.md) - Optimization techniques
- [**Security**](../advanced/security.md) - Best practices
- [**Authentication**](../advanced/authentication.md) - User auth patterns
- [**Caching**](../advanced/lazy-caching.md) - Database-native caching

### Troubleshooting
- [**Error Types**](../errors/error-types.md) - Error reference
- [**Debugging**](../errors/debugging.md) - Debug strategies
- [**Common Issues**](../errors/troubleshooting.md) - FAQ
