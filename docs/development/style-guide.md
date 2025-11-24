# FraiseQL Documentation Style Guide

**Purpose**: Ensure consistent, clear, and maintainable code examples across all FraiseQL documentation.

## Import Pattern (STANDARD)

```python
import fraiseql
```

**Why this pattern:**
- Concise and readable
- Imports only what you need
- Consistent across all examples

**NOT these patterns:**
```python
# ❌ Too verbose
@fraiseql.type
class User:
    pass

# ❌ Too specific imports
import fraiseql
from fraiseql.resolvers import query, mutation

# ❌ Import everything
from fraiseql import *
```

## Type Definition (STANDARD)

```python
import fraiseql
from uuid import UUID

@fraiseql.type(sql_source="v_user")  # Always specify source for queryable types
class User:
    id: UUID  # Always use UUID not str for IDs
    name: str
    email: str
    created_at: str  # ISO format datetime strings
```

**Rules:**
- Always use `@type(sql_source="v_*")` for database-backed types
- Use `UUID` type for ID fields, not `str`
- Use descriptive field names (snake_case)
- Include type hints for all fields
- Use `str` for datetime fields (ISO format from database)

## Query Pattern (STANDARD)

```python
import fraiseql

@fraiseql.query
def get_users() -> list[User]:
    """Get all users."""
    pass  # Implementation handled by framework

@fraiseql.query
def get_user_by_id(id: UUID) -> User:
    """Get a single user by ID."""
    pass  # Implementation handled by framework
```

**Rules:**
- Use `@query` decorator
- Return type hints must match GraphQL schema
- Use `list[Type]` for collections
- Include docstrings explaining the query
- Parameter names should match GraphQL field names

## Mutation Pattern (STANDARD)

```python
from fraiseql import mutation, input
from uuid import UUID

@fraiseql.input
class CreateUserInput:
    name: str
    email: str

@fraiseql.mutation
def create_user(input: CreateUserInput) -> User:
    """Create a new user."""
    pass  # Implementation handled by framework
```

**Rules:**
- Use `@input` for mutation input types
- Use `@mutation` decorator
- Input types should be separate from domain types
- Include docstrings explaining the mutation
- Return the created/updated resource

## Naming Conventions

### Database Objects
```sql
-- Tables: tb_ prefix
CREATE TABLE tb_user (...);

-- Views: v_ prefix
CREATE VIEW v_user AS (...);

-- Table views: tv_ prefix
CREATE TABLE tv_user_with_stats (...);

-- Functions: fn_ prefix
CREATE FUNCTION fn_create_user(...) RETURNS UUID AS $$
```

### Python Types
```python
# Domain types: PascalCase
class User:
    pass

# Input types: PascalCase + Input suffix
class CreateUserInput:
    pass

# Enums: PascalCase
class UserRole:
    pass
```

### GraphQL Fields
```python
import fraiseql

# Queries: camelCase
@fraiseql.query
def getUserById(id: UUID) -> User:
    pass

# Mutations: camelCase
@fraiseql.mutation
def createUser(input: CreateUserInput) -> User:
    pass

# Fields: camelCase
class User:
    firstName: str  # not first_name
    lastName: str   # not last_name
```

## File Structure (STANDARD)

```
my-fraiseql-api/
├── app.py              # Main FastAPI application
├── types.py            # All GraphQL type definitions
├── resolvers.py        # All queries and mutations
├── db/
│   ├── schema.sql      # Database schema (tables, views, functions)
│   └── migrations/     # Schema migration scripts
└── config.py           # Database connection and app config
```

**Rules:**
- Keep types separate from resolvers
- Database schema in dedicated directory
- Clear separation of concerns
- Consistent naming across projects

## Error Handling (STANDARD)

```python
import fraiseql

@fraiseql.mutation
def create_user(input: CreateUserInput) -> User | None:
    """Create a new user. Returns None if email already exists."""
    pass  # Framework handles database errors
```

**Rules:**
- Use `Type | None` for mutations that might fail
- Document failure conditions in docstrings
- Let framework handle database constraint violations
- Use descriptive error messages in GraphQL responses

## Code Comments (STANDARD)

```python
import fraiseql

@type(sql_source="v_user")
class User:
    id: UUID  # Primary key, auto-generated
    name: str  # User's full name, required
    email: str  # Unique email address, validated
    created_at: str  # ISO 8601 timestamp, auto-set
```

**Rules:**
- Comment non-obvious fields
- Explain business logic constraints
- Reference database constraints
- Keep comments concise but informative

## Testing Examples (STANDARD)

```python
import fraiseql

# In documentation examples, show both the code and expected GraphQL usage
@fraiseql.query
def get_user(id: UUID) -> User:
    """Get user by ID."""
    pass

# GraphQL usage:
# query {
#   getUser(id: "123e4567-e89b-12d3-a456-426614174000") {
#     id
#     name
#     email
#   }
# }
```

**Rules:**
- Show GraphQL query examples alongside Python code
- Use realistic UUIDs in examples
- Include both success and error cases
- Test examples manually before publishing

## Migration from Old Patterns

### Old Pattern → New Pattern
```python
# Old ❌
import fraiseql as gql_type

@gql_type(sql_source="v_user")
class User:
    id: UUID  # Wrong type
    name: str

# New ✅
import fraiseql

@type(sql_source="v_user")
class User:
    id: UUID  # Correct type
    name: str
```

**Migration checklist:**
- [ ] Replace `from fraiseql.decorators import` with `from fraiseql import`
- [ ] Change `str` IDs to `UUID` type
- [ ] Add missing type hints
- [ ] Update decorator names (`@gql_type` → `@type`)
- [ ] Add docstrings to queries/mutations
- [ ] Update naming conventions (snake_case → camelCase for GraphQL)

## Validation Checklist

Before publishing documentation:
- [ ] All imports use standard pattern
- [ ] All types have proper type hints
- [ ] All IDs use `UUID` not `str`
- [ ] All decorators use standard names
- [ ] All examples include GraphQL usage
- [ ] All code blocks are tested manually
- [ ] All naming follows conventions
- [ ] All docstrings are present and helpful
