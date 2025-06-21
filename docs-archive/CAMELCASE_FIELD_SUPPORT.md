# CamelCase Field Name Support in FraiseQL

## Overview

FraiseQL now supports automatic conversion of Python snake_case field names to GraphQL camelCase field names, following GraphQL best practices.

## Features

### 1. Automatic Conversion (Default)

By default, all snake_case field names are automatically converted to camelCase in the GraphQL schema:

```python
@fraiseql.type
class Repository:
    default_branch: str  # Exposed as 'defaultBranch' in GraphQL
    total_commits: int   # Exposed as 'totalCommits' in GraphQL
```

### 2. Configuration Option

You can disable automatic conversion if needed:

```python
schema = build_fraiseql_schema(
    query_types=[Repository],
    camel_case_fields=False  # Keeps snake_case in GraphQL
)
```

### 3. Explicit Field Names

For custom field names, use the `graphql_name` parameter:

```python
@fraiseql.type
class Product:
    internal_id: int = fraise_field(graphql_name="id")
    product_name: str = fraise_field(graphql_name="name")
```

### 4. Input Type Support

Input types also support camelCase conversion:

```python
@fraiseql.input
class CreateUserInput:
    user_name: str      # Accept as 'userName' in GraphQL
    email_address: str  # Accept as 'emailAddress' in GraphQL
```

### 5. Smart Conversion Rules

- Already camelCase fields are preserved: `httpTimeout` stays `httpTimeout`
- All uppercase acronyms are preserved: `URL` stays `URL`
- Enum values are NOT converted (following GraphQL conventions)

## Migration Guide

### For Existing FraiseQL Users

The change is backward compatible. To maintain snake_case behavior:

```python
schema = build_fraiseql_schema(
    query_types=[...],
    camel_case_fields=False
)
```

### For Users Coming from Strawberry

FraiseQL now matches Strawberry's default behavior of using camelCase fields, making migration easier.

## Example

```python
@fraiseql.type
class User:
    first_name: str
    last_login_time: float
    is_active: bool

@fraiseql.query
def get_current_user(info) -> User:
    return User(
        first_name="John",
        last_login_time=time.time(),
        is_active=True
    )

# GraphQL Query
query {
    getCurrentUser {
        firstName
        lastLoginTime
        isActive
    }
}
```

## Implementation Details

- Field names are converted at schema build time
- Original Python field names are preserved internally
- Resolvers continue to use Python field names
- Input coercion handles both snake_case and camelCase
