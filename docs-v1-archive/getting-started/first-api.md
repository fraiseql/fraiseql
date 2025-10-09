---
← [Quickstart](quickstart.md) | [Getting Started](index.md) | [Next: Core Concepts](../core-concepts/index.md) →
---

# Your First API

> **In this section:** Build a complete user management API with authentication
> **Prerequisites:** Completed [quickstart](quickstart.md), basic SQL knowledge
> **Time to complete:** 15 minutes

Build a complete user management API with FraiseQL in 15 minutes. This guide demonstrates FraiseQL's database-first approach, CQRS pattern, and type safety.

## Prerequisites

- PostgreSQL 12+ installed and running
- Python 3.10+ with FraiseQL installed
- Basic SQL knowledge

## Database Design

Create a database with proper separation between storage (tables) and API (views):

```sql
-- Create database
CREATE DATABASE user_management;

-- Users table (storage layer)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    role VARCHAR(50) DEFAULT 'user',
    password_hash VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Sessions table for authentication
CREATE TABLE sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    token VARCHAR(255) UNIQUE NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Roles table for permissions
CREATE TABLE roles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(50) UNIQUE NOT NULL,
    permissions JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- User roles junction table
CREATE TABLE user_roles (
    user_id UUID REFERENCES users(id) ON DELETE CASCADE,
    role_id UUID REFERENCES roles(id) ON DELETE CASCADE,
    assigned_at TIMESTAMPTZ DEFAULT NOW(),
    PRIMARY KEY (user_id, role_id)
);
```

## Creating Views (CQRS Read Model)

Design views that shape your API responses:

```sql
-- User view with proper structure
CREATE VIEW v_user AS
SELECT
    id,  -- Filter column for WHERE clauses
    email,  -- Another filter column
    role,  -- Filter column
    created_at,  -- Filter column for date ranges
    jsonb_build_object(
        '__typename', 'User',  -- Optional but recommended for GraphQL
        'id', id,
        'email', email,
        'name', name,
        'role', role,
        'createdAt', created_at,
        'updatedAt', updated_at
    ) AS data
FROM users;

-- User with roles view (demonstrates joins)
CREATE VIEW v_user_detail AS
SELECT
    u.id,
    u.email,
    jsonb_build_object(
        '__typename', 'UserDetail',
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'role', u.role,
        'roles', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', r.id,
                    'name', r.name,
                    'permissions', r.permissions
                ) ORDER BY r.name
            ) FILTER (WHERE r.id IS NOT NULL),
            '[]'::jsonb
        ),
        'createdAt', u.created_at
    ) AS data
FROM users u
LEFT JOIN user_roles ur ON u.id = ur.user_id
LEFT JOIN roles r ON ur.role_id = r.id
GROUP BY u.id, u.email, u.name, u.role, u.created_at;

-- Active sessions view
CREATE VIEW v_active_session AS
SELECT
    s.id,
    s.user_id,
    s.expires_at,
    jsonb_build_object(
        '__typename', 'Session',
        'id', s.id,
        'userId', s.user_id,
        'expiresAt', s.expires_at,
        'user', jsonb_build_object(
            'id', u.id,
            'name', u.name,
            'email', u.email
        )
    ) AS data
FROM sessions s
JOIN users u ON s.user_id = u.id
WHERE s.expires_at > NOW();
```

## Type Definitions

Define your GraphQL types using modern Python typing:

```python
from dataclasses import dataclass
from datetime import datetime
from uuid import UUID
from fraiseql import ID

@dataclass
class User:
    """User type for GraphQL schema"""
    id: ID  # UUID mapped to GraphQL ID
    email: str
    name: str
    role: str
    created_at: datetime
    updated_at: datetime

@dataclass
class UserDetail:
    """Detailed user with roles"""
    id: ID
    email: str
    name: str
    role: str
    roles: list[Role]  # Modern list syntax
    created_at: datetime

@dataclass
class Role:
    """Role with permissions"""
    id: ID
    name: str
    permissions: list[str]

@dataclass
class Session:
    """Active user session"""
    id: ID
    user_id: ID
    expires_at: datetime
    user: User | None  # Modern union syntax

@dataclass
class CreateUserInput:
    """Input for creating a user"""
    email: str
    name: str
    password: str
    role: str = "user"

@dataclass
class LoginInput:
    """Input for user login"""
    email: str
    password: str

@dataclass
class UserResponse:
    """Response for user operations"""
    success: bool
    user: User | None = None
    error: str | None = None
```

## Implementing Queries

Create queries that leverage your views:

```python
from fraiseql import fraiseql
from fraiseql.repository import Repository

@fraiseql.query
async def users(
    info,
    role: str | None = None,
    limit: int = 50,
    offset: int = 0
) -> list[User]:
    """Get users with optional filtering"""
    repo: Repository = info.context["repo"]

    where = {}
    if role:
        where["role"] = role

    results = await repo.find(
        "v_users",
        where=where,
        limit=limit,
        offset=offset,
        order_by=[("created_at", "DESC")]
    )

    return [User(**result["data"]) for result in results]

@fraiseql.query
async def user(info, id: ID) -> User | None:
    """Get a single user by ID"""
    repo: Repository = info.context["repo"]

    result = await repo.find_one("v_users", where={"id": id})
    return User(**result["data"]) if result else None

@fraiseql.query
async def user_detail(info, id: ID) -> UserDetail | None:
    """Get detailed user information with roles"""
    repo: Repository = info.context["repo"]

    result = await repo.find_one("v_user_details", where={"id": id})
    return UserDetail(**result["data"]) if result else None

@fraiseql.query
async def me(info) -> User | None:
    """Get current authenticated user"""
    user_id = info.context.get("user_id")
    if not user_id:
        return None

    repo: Repository = info.context["repo"]
    result = await repo.find_one("v_users", where={"id": user_id})
    return User(**result["data"]) if result else None
```

## Implementing Mutations

Use PostgreSQL functions for mutations (CQRS Write Model):

```sql
-- Create user function
CREATE OR REPLACE FUNCTION fn_create_user(
    p_email VARCHAR,
    p_name VARCHAR,
    p_password_hash VARCHAR,
    p_role VARCHAR DEFAULT 'user'
) RETURNS jsonb AS $$
DECLARE
    v_user_id UUID;
    v_result jsonb;
BEGIN
    -- Check if email exists
    IF EXISTS (SELECT 1 FROM users WHERE email = p_email) THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Email already exists'
        );
    END IF;

    -- Insert user
    INSERT INTO users (email, name, password_hash, role)
    VALUES (p_email, p_name, p_password_hash, p_role)
    RETURNING id INTO v_user_id;

    -- Return success with user data
    SELECT jsonb_build_object(
        'success', true,
        'user', jsonb_build_object(
            'id', id,
            'email', email,
            'name', name,
            'role', role,
            'createdAt', created_at,
            'updatedAt', updated_at
        )
    ) INTO v_result
    FROM users WHERE id = v_user_id;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Login function
CREATE OR REPLACE FUNCTION fn_login(
    p_email VARCHAR,
    p_password_hash VARCHAR
) RETURNS jsonb AS $$
DECLARE
    v_user_id UUID;
    v_token VARCHAR;
    v_result jsonb;
BEGIN
    -- Verify credentials
    SELECT id INTO v_user_id
    FROM users
    WHERE email = p_email AND password_hash = p_password_hash;

    IF v_user_id IS NULL THEN
        RETURN jsonb_build_object(
            'success', false,
            'error', 'Invalid credentials'
        );
    END IF;

    -- Create session token
    v_token := encode(gen_random_bytes(32), 'hex');

    INSERT INTO sessions (user_id, token, expires_at)
    VALUES (v_user_id, v_token, NOW() + INTERVAL '7 days');

    -- Return success with user and token
    SELECT jsonb_build_object(
        'success', true,
        'token', v_token,
        'user', jsonb_build_object(
            'id', id,
            'email', email,
            'name', name,
            'role', role,
            'createdAt', created_at,
            'updatedAt', updated_at
        )
    ) INTO v_result
    FROM users WHERE id = v_user_id;

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;
```

Python mutation handlers:

```python
import hashlib
from fraiseql import fraiseql

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> UserResponse:
    """Create a new user"""
    repo: Repository = info.context["repo"]

    # Hash password (use bcrypt in production)
    password_hash = hashlib.sha256(input.password.encode()).hexdigest()

    result = await repo.call_function(
        "fn_create_user",
        p_email=input.email,
        p_name=input.name,
        p_password_hash=password_hash,
        p_role=input.role
    )

    if result["success"]:
        return UserResponse(
            success=True,
            user=User(**result["user"])
        )
    else:
        return UserResponse(
            success=False,
            error=result["error"]
        )

@fraiseql.mutation
async def login(info, input: LoginInput) -> dict:
    """Authenticate user and create session"""
    repo: Repository = info.context["repo"]

    # Hash password for comparison
    password_hash = hashlib.sha256(input.password.encode()).hexdigest()

    result = await repo.call_function(
        "fn_login",
        p_email=input.email,
        p_password_hash=password_hash
    )

    return result  # Contains success, token, user, or error
```

## Adding Authentication

Implement context-based authentication:

```python
from fraiseql.fastapi import create_fraiseql_app
from fastapi import Request, HTTPException

async def get_context(request: Request) -> dict:
    """Extract user context from request"""
    context = {"request": request}

    # Get token from Authorization header
    auth_header = request.headers.get("Authorization")
    if auth_header and auth_header.startswith("Bearer "):
        token = auth_header[7:]

        # Get repository from app state
        repo = request.app.state.repo

        # Verify session
        session = await repo.find_one(
            "v_active_sessions",
            where={"token": token}
        )

        if session:
            context["user_id"] = session["user_id"]
            context["authenticated"] = True

    return context

# Create app with authentication
app = create_fraiseql_app(
    database_url="postgresql://localhost/user_management",
    types=[User, UserDetail, Role, Session],
    mutations=[create_user, login],
    context_getter=get_context
)
```

## Error Handling

Implement proper error handling patterns:

```python
from fraiseql import FraiseQLError

class AuthenticationError(FraiseQLError):
    """Raised when authentication fails"""
    pass

class ValidationError(FraiseQLError):
    """Raised when input validation fails"""
    pass

@fraiseql.mutation
async def update_user(
    info,
    id: ID,
    name: str | None = None,
    email: str | None = None
) -> UserResponse:
    """Update user with authentication check"""

    # Check authentication
    if not info.context.get("authenticated"):
        raise AuthenticationError("Authentication required")

    # Check authorization
    current_user_id = info.context.get("user_id")
    if str(current_user_id) != str(id):
        raise AuthenticationError("Can only update your own profile")

    # Validate input
    if email and "@" not in email:
        raise ValidationError("Invalid email format")

    repo: Repository = info.context["repo"]

    # Build update query
    updates = {}
    if name:
        updates["name"] = name
    if email:
        updates["email"] = email

    if not updates:
        return UserResponse(success=False, error="No updates provided")

    # Update user
    await repo.update("users", where={"id": id}, data=updates)

    # Fetch updated user
    result = await repo.find_one("v_users", where={"id": id})

    return UserResponse(
        success=True,
        user=User(**result["data"]) if result else None
    )
```

## Running Your API

```python
# main.py
import os
from fraiseql.fastapi import create_fraiseql_app
from fraiseql import FraiseQLConfig

# Import your types and resolvers
from .types import *
from .queries import *
from .mutations import *

# Configure FraiseQL
config = FraiseQLConfig(
    database_url=os.getenv("DATABASE_URL", "postgresql://localhost/user_management"),
    environment=os.getenv("ENV", "development"),
    enable_playground=True,
    enable_introspection=True
)

# Create application
app = create_fraiseql_app(
    config=config,
    types=[User, UserDetail, Role, Session, CreateUserInput, LoginInput, UserResponse],
    queries=[users, user, user_detail, me],
    mutations=[create_user, login, update_user],
    context_getter=get_context
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Testing Your API

Run the server and test with GraphQL Playground:

```bash
python main.py
# Visit http://localhost:8000/graphql
```

Example queries:

```graphql
# Create a user
mutation CreateUser {
  createUser(input: {
    email: "john@example.com"
    name: "John Doe"
    password: "secure_password"
  }) {
    success
    user {
      id
      email
      name
      role
    }
    error
  }
}

# Login
mutation Login {
  login(input: {
    email: "john@example.com"
    password: "secure_password"
  }) {
    success
    token
    user {
      id
      name
    }
    error
  }
}

# Get users (with authentication header)
query GetUsers {
  users(role: "user", limit: 10) {
    id
    name
    email
    createdAt
  }
}

# Get current user
query Me {
  me {
    id
    name
    email
    role
  }
}
```

## Performance Optimization

### Index Creation

```sql
-- Add indexes for common queries
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_role ON users(role);
CREATE INDEX idx_users_created_at ON users(created_at DESC);
CREATE INDEX idx_sessions_token ON sessions(token);
CREATE INDEX idx_sessions_expires_at ON sessions(expires_at);
```

### Materialized Views

For expensive queries, use materialized views:

```sql
-- Materialized view for user statistics
CREATE MATERIALIZED VIEW tv_user_stats AS
SELECT
    role,
    COUNT(*) as user_count,
    MAX(created_at) as last_signup
FROM users
GROUP BY role;

-- Refresh periodically
CREATE OR REPLACE FUNCTION refresh_user_stats() RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY tv_user_stats;
END;
$$ LANGUAGE plpgsql;
```

### Query Analysis

Enable query analysis in development:

```python
config = FraiseQLConfig(
    enable_query_analysis=True,
    log_slow_queries=True,
    slow_query_threshold_ms=100
)
```

## Next Steps

You've built a complete user management API with:

- ✅ Database-first design with views
- ✅ Type-safe GraphQL schema
- ✅ Authentication and authorization
- ✅ Error handling
- ✅ Performance optimization

## See Also

### Related Concepts

- [**Authentication Guide**](../advanced/authentication.md) - Complete auth implementation
- [**Database Views**](../core-concepts/database-views.md) - View design patterns
- [**Type System**](../core-concepts/type-system.md) - Advanced type features
- [**CQRS Pattern**](../core-concepts/architecture.md#cqrs) - Command Query Responsibility Segregation

### Next Steps

- [**Core Concepts**](../core-concepts/index.md) - Understand FraiseQL philosophy
- [**Blog Tutorial**](../tutorials/blog-api.md) - Complete production example
- [**API Reference**](../api-reference/index.md) - Complete API documentation

### Advanced Topics

- [**Security Best Practices**](../advanced/security.md) - Production security
- [**Performance Optimization**](../advanced/performance.md) - Query optimization
- [**Multi-tenancy**](../advanced/multi-tenancy.md) - Isolate tenant data
- [**Lazy Caching**](../advanced/lazy-caching.md) - Database-native caching

### Troubleshooting

- [**Error Types**](../errors/error-types.md) - Common error reference
- [**Debugging Guide**](../errors/debugging.md) - Debug strategies
- [**FAQ**](../errors/troubleshooting.md) - Common issues
