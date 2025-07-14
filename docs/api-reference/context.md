# GraphQL Context Reference

## Overview

The GraphQL context (`info.context`) is a dictionary available in all resolvers that provides access to request-scoped resources and data. It's the primary way to access the database, authentication information, and custom values in FraiseQL.

## Table of Contents

1. [Default Context Structure](#default-context-structure)
2. [Accessing Context](#accessing-context)
3. [Custom Context](#custom-context)
4. [Authentication Context](#authentication-context)
5. [DataLoader Integration](#dataloader-integration)
6. [Multi-Tenant Context](#multi-tenant-context)
7. [Testing with Context](#testing-with-context)
8. [Best Practices](#best-practices)

---

## Default Context Structure

When you create a FraiseQL app, it provides this default context:

```python
info.context = {
    "db": FraiseQLRepository,           # Database repository
    "user": UserContext | None,         # Authenticated user (if any)
    "authenticated": bool,              # Is user authenticated?
    "loader_registry": DataLoaderRegistry,  # DataLoader instances
    "request": Request,                 # FastAPI request object
}
```

### Context Fields Explained

| Field | Type | Description | Always Present |
|-------|------|-------------|----------------|
| `db` | `FraiseQLRepository` | Database operations interface | Yes |
| `user` | `UserContext \| None` | Authenticated user information | No (None if not authenticated) |
| `authenticated` | `bool` | Quick auth check | Yes |
| `loader_registry` | `DataLoaderRegistry` | DataLoader cache | Yes |
| `request` | `Request` | Original HTTP request | Yes |

---

## Accessing Context

### Basic Access Pattern

```python
@fraiseql.query
async def my_query(info) -> Result:
    # Get required values
    db = info.context["db"]

    # Get optional values safely
    user = info.context.get("user")
    tenant_id = info.context.get("tenant_id")

    # Check authentication
    if not info.context["authenticated"]:
        raise GraphQLError("Authentication required")

    return await db.find("some_view")
```

### Common Patterns

#### Database Access
```python
@fraiseql.query
async def users(info) -> list[User]:
    db = info.context["db"]
    return await db.find("user_view")
```

#### Authentication Check
```python
@fraiseql.query
async def me(info) -> User:
    if not info.context["authenticated"]:
        raise GraphQLError("Not authenticated")

    user = info.context["user"]
    db = info.context["db"]

    return await db.find_one("user_view", id=user.user_id)
```

#### Request Headers
```python
@fraiseql.query
async def debug_info(info) -> dict:
    request = info.context["request"]
    return {
        "user_agent": request.headers.get("user-agent"),
        "ip": request.client.host,
        "method": request.method
    }
```

---

## Custom Context

You can add custom values to the context using a `context_getter` function:

### Basic Custom Context

```python
async def get_context(request: Request) -> dict[str, Any]:
    # Start with default context
    context = await get_default_context(request)

    # Add custom values
    context["tenant_id"] = request.headers.get("x-tenant-id")
    context["api_version"] = request.headers.get("x-api-version", "v1")
    context["request_id"] = str(uuid4())

    return context

# Use in app creation
app = fraiseql.create_fraiseql_app(
    database_url=DATABASE_URL,
    types=[User, Post],
    context_getter=get_context
)
```

### Advanced Custom Context

```python
from typing import Any, Dict
import redis.asyncio as redis

async def get_context(request: Request) -> Dict[str, Any]:
    # Get database pool from app state
    db_pool = request.app.state.db_pool

    # Extract tenant from subdomain or header
    tenant_id = extract_tenant_id(request)

    # Create repository with tenant context
    repo = FraiseQLRepository(
        pool=db_pool,
        context={
            "tenant_id": tenant_id,
            "request_id": request.headers.get("x-request-id", str(uuid4()))
        }
    )

    # Get current user if authenticated
    user = None
    authenticated = False
    if "authorization" in request.headers:
        user = await get_current_user(request)
        authenticated = user is not None

    # Additional services
    cache = redis.from_url("redis://localhost")

    return {
        "db": repo,
        "user": user,
        "authenticated": authenticated,
        "tenant_id": tenant_id,
        "cache": cache,
        "feature_flags": await get_feature_flags(tenant_id),
        "request": request,
        "loader_registry": DataLoaderRegistry(),
    }

def extract_tenant_id(request: Request) -> str:
    # From header
    if "x-tenant-id" in request.headers:
        return request.headers["x-tenant-id"]

    # From subdomain
    host = request.headers.get("host", "")
    if "." in host:
        subdomain = host.split(".")[0]
        if subdomain != "www":
            return subdomain

    # Default
    return "default"
```

### Using Custom Context

```python
@fraiseql.query
async def tenant_stats(info) -> TenantStats:
    tenant_id = info.context["tenant_id"]
    cache = info.context["cache"]
    feature_flags = info.context["feature_flags"]

    # Check cache first
    cache_key = f"stats:{tenant_id}"
    cached = await cache.get(cache_key)
    if cached:
        return TenantStats(**json.loads(cached))

    # Query database
    db = info.context["db"]
    stats = await calculate_tenant_stats(db, tenant_id)

    # Cache if feature enabled
    if feature_flags.get("enable_stats_cache"):
        await cache.setex(cache_key, 300, json.dumps(stats))

    return stats
```

---

## Authentication Context

When authentication is enabled, the context includes user information:

### UserContext Structure

```python
@dataclass
class UserContext:
    user_id: str          # Unique user identifier
    email: str            # User's email
    roles: list[str]      # User's roles
    permissions: list[str] # User's permissions
    metadata: dict        # Additional claims from token
```

### Accessing User Information

```python
@fraiseql.query
@requires_auth
async def my_profile(info) -> UserProfile:
    # After @requires_auth, user is guaranteed to exist
    user = info.context["user"]
    db = info.context["db"]

    profile = await db.find_one("user_profile_view", id=user.user_id)
    if not profile:
        raise GraphQLError("Profile not found")

    # Add computed fields
    profile.email = user.email
    profile.roles = user.roles

    return profile
```

### Role-Based Access

```python
@fraiseql.query
@requires_auth
@requires_role("admin")
async def admin_dashboard(info) -> AdminStats:
    # User is guaranteed to be admin
    user = info.context["user"]
    db = info.context["db"]

    return await db.find_one("admin_stats_view")

@fraiseql.query
@requires_auth
async def my_resources(info) -> list[Resource]:
    user = info.context["user"]
    db = info.context["db"]

    # Filter based on user role
    if "admin" in user.roles:
        # Admins see all
        return await db.find("resource_view")
    else:
        # Others see only their own
        return await db.find("resource_view", owner_id=user.user_id)
```

### Custom Authentication

```python
async def get_context(request: Request) -> dict[str, Any]:
    # Custom auth logic
    auth_header = request.headers.get("authorization", "")

    user = None
    authenticated = False

    if auth_header.startswith("Bearer "):
        token = auth_header[7:]
        try:
            # Verify token and extract user
            payload = jwt.decode(token, SECRET_KEY, algorithms=["HS256"])
            user = UserContext(
                user_id=payload["sub"],
                email=payload["email"],
                roles=payload.get("roles", []),
                permissions=payload.get("permissions", []),
                metadata=payload
            )
            authenticated = True
        except jwt.InvalidTokenError:
            pass  # Invalid token, user remains None

    # Get default context and add auth
    context = await get_default_context(request)
    context.update({
        "user": user,
        "authenticated": authenticated
    })

    return context
```

---

## DataLoader Integration

The context includes a DataLoader registry for efficient batch loading:

### Using DataLoaders

```python
from fraiseql.optimization import get_loader

@fraiseql.type
class Post:
    id: UUID
    title: str
    author_id: UUID

    @fraiseql.field
    async def author(self, info) -> User:
        # Get loader from registry
        loader = get_loader(info.context, UserLoader)
        return await loader.load(self.author_id)

# Define the loader
class UserLoader(DataLoader[UUID, User]):
    def __init__(self, db: FraiseQLRepository):
        super().__init__()
        self.db = db

    async def batch_load(self, user_ids: list[UUID]) -> list[Optional[User]]:
        # Batch query
        users = await self.db.find("user_view", id=user_ids)

        # Return in same order as requested
        user_map = {u.id: u for u in users}
        return [user_map.get(uid) for uid in user_ids]
```

### Custom DataLoader Context

```python
async def get_context(request: Request) -> dict[str, Any]:
    context = await get_default_context(request)

    # Pre-create loaders
    db = context["db"]
    registry = context["loader_registry"]

    registry.register(UserLoader(db))
    registry.register(PostLoader(db))
    registry.register(CommentLoader(db))

    return context
```

---

## Multi-Tenant Context

Common pattern for SaaS applications:

### Setup Multi-Tenant Context

```python
async def get_context(request: Request) -> dict[str, Any]:
    # Extract tenant
    tenant_id = request.headers.get("x-tenant-id")
    if not tenant_id:
        raise HTTPException(400, "Missing tenant ID")

    # Validate tenant
    tenant = await validate_tenant(tenant_id)
    if not tenant:
        raise HTTPException(404, "Invalid tenant")

    # Create tenant-scoped repository
    db_pool = request.app.state.db_pool
    repo = FraiseQLRepository(
        pool=db_pool,
        context={
            "tenant_id": tenant_id,
            "tenant": tenant
        }
    )

    # Get user within tenant context
    user = None
    authenticated = False
    if auth_header := request.headers.get("authorization"):
        user = await get_tenant_user(tenant_id, auth_header)
        authenticated = user is not None

    return {
        "db": repo,
        "tenant_id": tenant_id,
        "tenant": tenant,
        "user": user,
        "authenticated": authenticated,
        "request": request,
        "loader_registry": DataLoaderRegistry(),
    }
```

### Using Tenant Context

```python
@fraiseql.query
async def tenant_users(info) -> list[User]:
    tenant_id = info.context["tenant_id"]
    db = info.context["db"]

    # Always filter by tenant
    return await db.find("user_view", tenant_id=tenant_id)

@fraiseql.query
async def tenant_settings(info) -> TenantSettings:
    tenant = info.context["tenant"]

    return TenantSettings(
        id=tenant.id,
        name=tenant.name,
        plan=tenant.plan,
        features=tenant.features
    )

@fraiseql.mutation
@requires_auth
@requires_role("tenant_admin")
async def update_tenant_settings(
    info,
    input: UpdateTenantInput
) -> TenantSettings:
    tenant_id = info.context["tenant_id"]
    user = info.context["user"]
    db = info.context["db"]

    # Verify user belongs to tenant
    if user.tenant_id != tenant_id:
        raise GraphQLError("Access denied")

    # Update tenant
    result = await db.call_function(
        "update_tenant",
        tenant_id=tenant_id,
        updates=input.__dict__,
        updated_by=user.user_id
    )

    return TenantSettings(**result)
```

---

## Testing with Context

### Unit Testing with Mock Context

```python
import pytest
from unittest.mock import Mock, AsyncMock

@pytest.fixture
def mock_context():
    """Create mock context for testing."""
    mock_db = AsyncMock(spec=FraiseQLRepository)

    return {
        "db": mock_db,
        "user": UserContext(
            user_id="test-user-123",
            email="test@example.com",
            roles=["user"],
            permissions=[],
            metadata={}
        ),
        "authenticated": True,
        "tenant_id": "test-tenant",
        "request": Mock(spec=Request),
        "loader_registry": DataLoaderRegistry(),
    }

@pytest.fixture
def mock_info(mock_context):
    """Create mock info object."""
    info = Mock()
    info.context = mock_context
    return info

async def test_user_query(mock_info):
    # Setup mock response
    mock_user = User(id="123", name="Test User", email="test@example.com")
    mock_info.context["db"].find_one.return_value = mock_user

    # Test query
    result = await user(mock_info, id="123")

    # Verify
    assert result == mock_user
    mock_info.context["db"].find_one.assert_called_once_with(
        "user_view",
        id="123"
    )
```

### Integration Testing

```python
@pytest.fixture
async def test_context(test_db_pool):
    """Create real context for integration tests."""
    repo = FraiseQLRepository(test_db_pool)

    return {
        "db": repo,
        "user": UserContext(
            user_id="test-user",
            email="test@example.com",
            roles=["admin"],
            permissions=["users:read", "users:write"],
            metadata={}
        ),
        "authenticated": True,
        "tenant_id": "test-tenant",
        "request": Mock(spec=Request),
        "loader_registry": DataLoaderRegistry(),
    }

async def test_create_user_integration(test_context):
    # Create info object
    info = Mock()
    info.context = test_context

    # Test mutation
    input_data = CreateUserInput(
        email="new@example.com",
        name="New User"
    )

    result = await create_user(info, input=input_data)

    # Verify in database
    db = test_context["db"]
    created = await db.find_one("user_view", email="new@example.com")
    assert created is not None
    assert created.name == "New User"
```

---

## Best Practices

### 1. Always Access Context Safely

```python
# ❌ Bad: Assumes key exists
tenant_id = info.context["tenant_id"]  # KeyError if missing

# ✅ Good: Safe access with default
tenant_id = info.context.get("tenant_id", "default")
```

### 2. Type Your Context

```python
from typing import TypedDict

class AppContext(TypedDict, total=False):
    db: FraiseQLRepository
    user: Optional[UserContext]
    authenticated: bool
    tenant_id: str
    cache: redis.Redis
    feature_flags: dict[str, bool]

@fraiseql.query
async def typed_query(info) -> Result:
    context: AppContext = info.context
    # Now you get type hints!
    db = context["db"]
```

### 3. Don't Mutate Context

```python
# ❌ Bad: Modifying context
info.context["temp_value"] = calculate_something()

# ✅ Good: Use local variables
temp_value = calculate_something()
```

### 4. Validate Context in Queries

```python
@fraiseql.query
async def tenant_specific_query(info) -> Result:
    # Validate required context
    if "tenant_id" not in info.context:
        raise GraphQLError("Tenant context required")

    tenant_id = info.context["tenant_id"]
    # Continue with query...
```

### 5. Document Custom Context

```python
async def get_context(request: Request) -> dict[str, Any]:
    """
    Create GraphQL context.

    Adds:
    - tenant_id: From X-Tenant-ID header
    - feature_flags: Tenant-specific features
    - cache: Redis client for caching
    """
    # Implementation...
```

## Common Issues

### Issue: Context is None

```python
# This happens when not using FraiseQL patterns
class Query:
    def resolve_users(self, info):
        # info might be None if not properly integrated
        pass
```

**Solution**: Always use `@fraiseql.query` decorator.

### Issue: Missing Database

```python
AttributeError: 'NoneType' object has no attribute 'find'
```

**Solution**: Ensure `database_url` is provided to `create_fraiseql_app()`.

### Issue: Custom Context Not Available

**Solution**: Pass `context_getter` to app creation:

```python
app = fraiseql.create_fraiseql_app(
    types=[...],
    context_getter=get_context  # Don't forget this!
)
```

## Next Steps

- Learn about [Query Patterns](../patterns/queries.md)
- Explore [Authentication](../advanced/authentication.md)
- Understand [Multi-Tenancy](../patterns/multi-tenant.md)
