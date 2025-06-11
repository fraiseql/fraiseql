# Security

FraiseQL takes security seriously and implements multiple layers of protection to ensure your GraphQL APIs are safe from common vulnerabilities.

## SQL Injection Prevention

FraiseQL uses **parameterized queries** throughout its SQL generation layer, providing complete protection against SQL injection attacks. This is achieved through:

### Parameterized WHERE Clauses

All WHERE clause generation uses psycopg's `Composed` and `Literal` classes, which ensure proper parameterization:

```python
from fraiseql import type, fraise_field
from fraiseql.sql.where_generator import safe_create_where_type

@type
class User:
    id: int
    name: str
    email: str
    is_admin: bool

# Generate type-safe WHERE filter
UserWhere = safe_create_where_type(User)

# Even with malicious input, the query is safe
where = UserWhere(
    name={"eq": "'; DROP TABLE users; --"},  # SQL injection attempt
    email={"in": ["admin@example.com", "' OR '1'='1"]}
)

# The generated SQL uses proper parameterization
# All values are safely escaped by psycopg
sql = where.to_sql()
```

### How It Works

1. **No String Concatenation**: FraiseQL never concatenates user input directly into SQL strings
2. **Type-Safe Operators**: All query operators (`eq`, `gt`, `in`, etc.) use parameterized SQL
3. **Automatic Escaping**: psycopg handles all value escaping and type casting
4. **JSONB Safety**: Special handling for PostgreSQL's JSONB operators ensures type safety

### Supported Secure Operators

All operators are implemented with parameterization:

- **Comparison**: `eq`, `neq`, `gt`, `gte`, `lt`, `lte`
- **String**: `contains`, `startswith`, `matches`
- **Array**: `in`, `notin`
- **JSON**: `overlaps`, `strictly_contains`
- **Null**: `isnull`
- **Ltree**: `depth_eq`, `depth_gt`, `depth_lt`, `isdescendant`

## Input Validation

FraiseQL provides multiple layers of input validation:

### Type Validation

Python's type system validates all inputs at the GraphQL layer:

```python
@input
class CreateUserInput:
    email: EmailAddress  # Validates email format
    age: int  # Ensures integer type
    ip_address: IPAddress  # Validates IP format
```

### Custom Validators

Add custom validation logic to any field:

```python
from fraiseql import input, fraise_field

@input
class UpdateProfileInput:
    bio: str = fraise_field(
        description="User biography",
        validator=lambda v: len(v) <= 500 or "Bio must be 500 characters or less"
    )
```

## Authentication & Authorization

FraiseQL provides flexible authentication with built-in security features:

### Secure by Default

```python
from fraiseql import create_fraiseql_app
from fraiseql.auth import Auth0Config

app = create_fraiseql_app(
    database_url="postgresql://...",
    auth=Auth0Config(
        domain="your-domain.auth0.com",
        api_identifier="https://api.example.com"
    ),
    # Security settings
    enable_introspection=False,  # Disabled in production
    enable_playground=False,     # Disabled in production
)
```

### Role-Based Access Control

```python
from fraiseql import requires_auth, requires_role

@requires_auth
@requires_role("admin")
async def delete_user(id: int, info) -> DeleteUserResult:
    # Only admins can delete users
    ...
```

## Production Security

### Environment-Based Configuration

```python
import os
from fraiseql import create_fraiseql_app

is_production = os.getenv("ENVIRONMENT") == "production"

app = create_fraiseql_app(
    # ... other config ...
    production=is_production,
    enable_introspection=not is_production,
    enable_playground=not is_production,
)
```

### Query Depth Limiting

Prevent denial-of-service attacks from deeply nested queries:

```python
app = create_fraiseql_app(
    # ... other config ...
    max_query_depth=10,  # Limit query nesting
    query_cost_analysis=True,  # Enable query complexity analysis
)
```

## Best Practices

### 1. Use Environment Variables

Never hardcode sensitive information:

```python
import os

DATABASE_URL = os.environ["DATABASE_URL"]
AUTH0_DOMAIN = os.environ["AUTH0_DOMAIN"]
AUTH0_CLIENT_SECRET = os.environ["AUTH0_CLIENT_SECRET"]
```

### 2. Validate All Inputs

Always validate user inputs, even with type safety:

```python
@mutation
async def update_user(input: UpdateUserInput, info) -> UpdateUserResult:
    # Additional validation beyond type checking
    if input.age and (input.age < 0 or input.age > 150):
        return UpdateUserError(message="Invalid age")

    # Proceed with update
    ...
```

### 3. Use Database Constraints

Leverage PostgreSQL's constraint system as an additional security layer:

```sql
-- Ensure data integrity at the database level
ALTER TABLE users
ADD CONSTRAINT email_format
CHECK (data->>'email' ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}$');

ALTER TABLE users
ADD CONSTRAINT age_range
CHECK ((data->>'age')::int BETWEEN 0 AND 150);
```

### 4. Audit Sensitive Operations

Log all sensitive operations for security auditing:

```python
import logging

logger = logging.getLogger(__name__)

@mutation
@requires_role("admin")
async def delete_user(id: int, info) -> DeleteUserResult:
    user = info.context["user"]
    logger.info(
        "User deletion attempted",
        extra={
            "admin_id": user.id,
            "target_user_id": id,
            "timestamp": datetime.utcnow()
        }
    )
    # Proceed with deletion
    ...
```

### 5. Regular Security Updates

Keep FraiseQL and its dependencies up to date:

```bash
# Check for outdated packages
pip list --outdated

# Update FraiseQL
pip install --upgrade fraiseql

# Update all dependencies
pip install --upgrade -r requirements.txt
```

## Security Reporting

If you discover a security vulnerability in FraiseQL, please report it to security@fraiseql.com. We take all security reports seriously and will respond promptly.

## Summary

FraiseQL provides robust security features out of the box:

- ✅ **SQL Injection Prevention**: All queries use parameterization
- ✅ **Type-Safe Inputs**: Python type system validates all data
- ✅ **Flexible Authentication**: Pluggable auth with Auth0 support
- ✅ **Production Hardening**: Secure defaults for production
- ✅ **Best Practices**: Clear guidelines for secure development

By following these security practices and leveraging FraiseQL's built-in protections, you can build GraphQL APIs that are both powerful and secure.
