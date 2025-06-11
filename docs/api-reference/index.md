# API Reference

Complete reference for all FraiseQL APIs, decorators, and utilities.

## [Decorators](./decorators.md)
Core decorators for defining GraphQL types, inputs, enums, and mutations.

- `@fraiseql.type` - Define GraphQL object types
- `@fraiseql.input` - Define GraphQL input types
- `@fraiseql.enum` - Define GraphQL enum types
- `@fraiseql.interface` - Define GraphQL interface types
- `@fraiseql.result`, `@fraiseql.success`, `@fraiseql.failure` - Result unions

## [Field Types](./field-types.md)
Field definitions and metadata for GraphQL schema generation.

- `fraise_field()` - Field metadata and configuration
- Type mapping from Python to GraphQL
- Optional and nullable field handling

## [Scalars](./scalars.md)
Built-in and custom scalar types for domain-specific data.

- `UUID` - Universally unique identifiers
- `DateTime` - ISO 8601 date and time
- `Date` - Date only
- `JSON` - Arbitrary JSON data
- `EmailAddress` - Email validation
- Custom scalar creation

## [Application](./application.md)
FastAPI application creation and configuration.

- `create_fraiseql_app()` - Main application factory
- Configuration options
- Development vs production modes
- Middleware integration

## [Repository](./repository.md)
Database repository pattern for CQRS operations.

- `CQRSRepository` - Base repository class
- Query operations
- Command operations
- Transaction handling

## [Authentication](./authentication.md)
Authentication providers and decorators.

- `@requires_auth` - Require authentication
- `@requires_role` - Require specific roles
- `@requires_permission` - Require permissions
- Custom auth providers

## [TestFoundry](./testfoundry.md)
Automated test generation for database operations and GraphQL mutations.

- Automatic pgTAP test generation
- Constraint violation testing
- Authorization testing
- Custom scenario support

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
| `FRAISEQL_AUTO_CAMEL_CASE` | `true` | Auto convert snake_case to camelCase |
| `FRAISEQL_DEV_PASSWORD` | - | Development authentication password |
| `FRAISEQL_DEV_USERNAME` | `admin` | Development authentication username |
| `DATABASE_URL` | - | PostgreSQL connection string |
