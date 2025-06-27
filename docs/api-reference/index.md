# API Reference

Complete reference for all FraiseQL APIs, decorators, and utilities.

## Core References

### [Complete Decorator Reference](./decorators-complete.md) 🆕
Comprehensive guide to all FraiseQL decorators with examples and common mistakes.

- `@fraiseql.type` - Define GraphQL object types
- `@fraiseql.query` - Define GraphQL queries (NOT `resolve_` methods!)
- `@fraiseql.mutation` - Define GraphQL mutations
- `@fraiseql.input` - Define GraphQL input types
- `@fraiseql.enum` - Define GraphQL enum types
- `@fraiseql.field` - Custom field resolvers
- `@fraiseql.interface` - Define GraphQL interfaces
- `@fraiseql.subscription` - Real-time subscriptions
- Result pattern decorators (`@fraiseql.result`, `@fraiseql.success`, `@fraiseql.failure`)

### [Repository API](./repository.md) 🆕
Complete FraiseQLRepository reference for database operations.

- `find()` - Query multiple records from views
- `find_one()` - Query single record
- `count()` - Count records
- `call_function()` - Call PostgreSQL functions
- Transaction support
- Advanced query methods

### [Context Reference](./context.md) 🆕
Understanding and using GraphQL context in FraiseQL.

- Default context structure
- Accessing database, user, and request info
- Custom context with `context_getter`
- Authentication context
- Multi-tenant patterns
- Testing with context

## Quick Links

### [Legacy Decorators Guide](./decorators.md)
Original decorator reference (see Complete Decorator Reference above for latest).

## Field Definitions and Scalars

### fraise_field()
Field metadata and configuration for GraphQL schema generation:

- `description` - GraphQL field description
- `purpose` - Internal documentation
- `default` - Default field values
- `deprecation_reason` - Mark fields as deprecated

### Built-in Scalars
FraiseQL includes several custom scalar types:

- `UUID` - Universally unique identifiers
- `DateTime` - ISO 8601 date and time
- `Date` - Date only
- `JSON` - Arbitrary JSON data
- `EmailAddress` - Email validation

## [Application](./application.md)
FastAPI application creation and configuration.

- `create_fraiseql_app()` - Main application factory
- Configuration options
- Development vs production modes
- Middleware integration

## Database Repository
FraiseQL uses the CQRS (Command Query Responsibility Segregation) pattern:

- `CQRSRepository` - Base repository class for database operations
- Query operations with automatic SQL generation
- Command operations through PostgreSQL functions
- Built-in transaction handling

## Authentication
Flexible authentication system with multiple providers:

- Development authentication for local testing
- Auth0 integration for production use
- Custom authentication provider support
- Role-based access control

## TurboRouter
High-performance query execution engine:

- `TurboQuery` - Define pre-validated queries with SQL templates
- `TurboRegistry` - Manage registered queries with LRU cache
- `TurboRouter` - Execute registered queries with minimal overhead
- Automatic fallback to standard GraphQL execution

## Quick Reference

### Import Aliases

```python
import fraiseql

# Core decorators (recommended)
@fraiseql.type
@fraiseql.input
@fraiseql.enum

# Alternative imports
from fraiseql import fraise_type, fraise_input, fraise_enum
```

### Common Patterns

```python
# Basic type definition
@fraiseql.type
class User:
    id: UUID
    name: str = fraise_field(description="User's display name")
    email: str = fraise_field(description="Email address")

# Input type with validation
@fraiseql.input
class CreateUserInput:
    name: str
    email: str
    password: str

# Result union type
@fraiseql.result
class CreateUserResult:
    pass

@fraiseql.success
class CreateUserSuccess:
    user: User

@fraiseql.failure
class CreateUserError:
    message: str
    code: str
```

### Type Mapping Reference

| Python Type | GraphQL Type | Notes |
|-------------|--------------|-------|
| `str` | `String` | |
| `int` | `Int` | |
| `float` | `Float` | |
| `bool` | `Boolean` | |
| `UUID` | `UUID` | Custom scalar |
| `datetime` | `DateTime` | Custom scalar |
| `date` | `Date` | Custom scalar |
| `Optional[T]` | `T` (nullable) | |
| `list[T]` | `[T]` | |
| `dict`/`Any` | `JSON` | Custom scalar |

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FRAISEQL_PRODUCTION` | `false` | Enable production mode |
| `FRAISEQL_AUTO_CAMEL_CASE` | `false` | Auto convert snake_case to camelCase |
| `FRAISEQL_DEV_PASSWORD` | - | Development authentication password |
| `FRAISEQL_DEV_USERNAME` | `admin` | Development authentication username |
| `FRAISEQL_ENABLE_TURBO_ROUTER` | `true` | Enable TurboRouter for registered queries |
| `FRAISEQL_TURBO_ROUTER_CACHE_SIZE` | `1000` | Max number of queries in TurboRouter cache |
| `DATABASE_URL` | - | PostgreSQL connection string |
