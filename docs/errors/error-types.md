---
← [TurboRouter](../advanced/turbo-router.md) | [Errors Index](./index.md) | [Troubleshooting →](./troubleshooting.md)
---

# Error Types

> **In this section:** Understand and handle different error categories in FraiseQL
> **Prerequisites:** Basic understanding of error handling and debugging
> **Time to complete:** 20 minutes

FraiseQL categorizes errors into distinct types to help you quickly identify and resolve issues.

## GraphQL Errors

Errors that occur during GraphQL query parsing, validation, or execution.

### Schema Errors
Occur when there are issues with the GraphQL schema definition.

```python
from fraiseql.core.exceptions import SchemaError

# Example: Missing required view
raise SchemaError(
    "View 'v_user' not found in database. "
    "Create it with: CREATE VIEW v_user AS SELECT * FROM users;"
)
```

**Common Causes:**
- Missing database views
- Type registration failures
- Circular dependencies
- Invalid field definitions

### Validation Errors
Input validation failures during query or mutation execution.

```python
from fraiseql.core.exceptions import ValidationError
from graphql import GraphQLError

# Field validation
if not email.endswith("@company.com"):
    raise GraphQLError(
        message="Email must be a company email",
        extensions={
            "code": "VALIDATION_ERROR",
            "field": "email",
            "constraint": "company_domain"
        }
    )
```

**Common Causes:**
- Invalid input format
- Missing required fields
- Type mismatches
- Business rule violations

### Resolver Errors
Errors during field resolution.

```python
from fraiseql.errors.exceptions import ResolverError

# Example: Failed to resolve field
raise ResolverError(
    message="Failed to resolve user.posts",
    field="posts",
    parent_type="User",
    hint="Check that v_user_posts view exists"
)
```

## Database Errors

Errors related to database operations.

### Connection Errors
Database connection failures.

```python
from fraiseql.errors.exceptions import DatabaseQueryError
import psycopg

try:
    await repo.connect()
except psycopg.OperationalError as e:
    raise DatabaseQueryError(
        message="Failed to connect to database",
        query_context={"host": "localhost", "port": 5432},
        hint="Check that PostgreSQL is running"
    ) from e
```

**Common Causes:**
- PostgreSQL not running
- Invalid connection string
- Network issues
- Authentication failures

### Query Errors
Errors during SQL query execution.

```python
from fraiseql.errors.exceptions import DatabaseQueryError

try:
    result = await repo.execute(query)
except Exception as e:
    raise DatabaseQueryError(
        message="Query execution failed",
        query_context={"query": query, "params": params},
        hint="Check query syntax and table/view existence"
    ) from e
```

**Common Causes:**
- Syntax errors
- Missing tables/views
- Permission issues
- Constraint violations

### Where Clause Errors
Invalid WHERE clause construction.

```python
from fraiseql.errors.exceptions import WhereClauseError

# Example: Invalid operator
raise WhereClauseError(
    message="Invalid operator 'nearby'",
    field="location",
    operator="nearby",
    hint="Use supported operators: eq, ne, in, like, gt, lt, gte, lte"
)
```

## Authentication & Authorization Errors

Security-related errors.

### Authentication Errors
User not authenticated.

```python
from fraiseql.core.exceptions import AuthenticationError
from graphql import GraphQLError

if not user_token:
    raise GraphQLError(
        message="Authentication required",
        extensions={"code": "UNAUTHENTICATED"}
    )
```

**Common Causes:**
- Missing auth token
- Expired token
- Invalid credentials
- Session timeout

### Authorization Errors
User lacks required permissions.

```python
from fraiseql.core.exceptions import AuthorizationError
from graphql import GraphQLError

if not user.has_permission("posts.delete"):
    raise GraphQLError(
        message="Permission denied",
        extensions={
            "code": "FORBIDDEN",
            "required_permission": "posts.delete",
            "user_permissions": user.permissions
        }
    )
```

**Common Causes:**
- Insufficient permissions
- Resource ownership
- Role restrictions
- Field-level authorization

### Field Authorization Errors
Field-level access control failures.

```python
from graphql import GraphQLError

# Field-level authorization
if not can_access_field(user, "User.salary"):
    raise GraphQLError(
        message="Access denied to field 'salary'",
        extensions={
            "code": "FIELD_AUTHORIZATION_ERROR",
            "field": "salary",
            "type": "User"
        }
    )
```

## Business Logic Errors

Application-specific errors.

### Constraint Violations
Business rule violations.

```python
from graphql import GraphQLError

# Example: Business rule violation
if account.balance < withdrawal_amount:
    raise GraphQLError(
        message="Insufficient funds",
        extensions={
            "code": "INSUFFICIENT_FUNDS",
            "available": account.balance,
            "requested": withdrawal_amount
        }
    )
```

### State Errors
Invalid state transitions.

```python
# Example: Invalid state transition
if order.status == "completed":
    raise GraphQLError(
        message="Cannot modify completed order",
        extensions={
            "code": "INVALID_STATE",
            "current_state": "completed",
            "attempted_action": "update"
        }
    )
```

## System Errors

Infrastructure and system-level errors.

### Timeout Errors
Operation timeouts.

```python
import asyncio
from graphql import GraphQLError

try:
    result = await asyncio.wait_for(operation(), timeout=30)
except asyncio.TimeoutError:
    raise GraphQLError(
        message="Operation timed out",
        extensions={
            "code": "TIMEOUT",
            "timeout_seconds": 30
        }
    )
```

### Resource Errors
Resource limit violations.

```python
# Example: Rate limiting
if request_count > rate_limit:
    raise GraphQLError(
        message="Rate limit exceeded",
        extensions={
            "code": "RATE_LIMITED",
            "limit": rate_limit,
            "reset_time": reset_timestamp
        }
    )
```

### Configuration Errors
Invalid configuration.

```python
from fraiseql.core.exceptions import FraiseQLError

# Example: Missing configuration
if not config.database_url:
    raise FraiseQLError(
        "DATABASE_URL environment variable not set. "
        "Set it to your PostgreSQL connection string."
    )
```

## Custom Scalar Errors

Validation errors for custom scalar types.

### Email Validation
```python
from graphql import GraphQLError

if not re.match(r'^[\w\.-]+@[\w\.-]+\.\w+$', value):
    raise GraphQLError(f"Invalid email format: {value}")
```

### URL Validation
```python
from urllib.parse import urlparse

try:
    result = urlparse(value)
    if not all([result.scheme, result.netloc]):
        raise GraphQLError(f"Invalid URL: {value}")
except Exception:
    raise GraphQLError(f"Invalid URL format: {value}")
```

### IP Address Validation
```python
import ipaddress

try:
    ipaddress.ip_address(value)
except ValueError:
    raise GraphQLError(f"Invalid IP address: {value}")
```

## Partial Instantiation Errors

Object creation failures with partial data.

```python
from fraiseql.errors.exceptions import PartialInstantiationError

# Example: Missing required fields
raise PartialInstantiationError(
    type_name="User",
    missing_fields=["email", "name"],
    provided_fields=["id"],
    hint="Ensure view returns all required fields"
)
```

**Common Causes:**
- Incomplete view definitions
- Missing JOIN conditions
- NULL values in required fields
- Type mismatch in fields

## Error Hierarchies

FraiseQL errors follow this hierarchy:

```
Exception
├── FraiseQLError (base for all FraiseQL errors)
│   ├── SchemaError
│   ├── ValidationError
│   ├── AuthenticationError
│   └── AuthorizationError
└── FraiseQLException (enhanced errors)
    ├── DatabaseQueryError
    ├── WhereClauseError
    ├── ResolverError
    ├── TypeRegistrationError
    └── PartialInstantiationError
```

## Next Steps

- Review [error codes reference](./error-codes.md)
- Learn [error handling patterns](./handling-patterns.md)
- See [troubleshooting guide](./troubleshooting.md)
