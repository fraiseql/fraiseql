# FraiseQL API Reference

Complete reference for all FraiseQL decorators, classes, and functions.

## Table of Contents

1. [Decorators](#decorators)
2. [Repository](#repository)
3. [App Creation](#app-creation)
4. [Context](#context)
5. [Types](#types)
6. [Authentication](#authentication)
7. [Utilities](#utilities)

## Decorators

### @fraise_type

Defines a GraphQL type from a Python class.

```python
@fraise_type
class TypeName:
    field: FieldType
```

**Parameters**: None

**Usage**:
```python
from fraiseql import fraise_type
from uuid import UUID
from datetime import datetime

@fraise_type
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
    is_active: bool = True  # Default value
```

**Notes**:
- Uses Python type annotations
- Supports all standard Python types
- Supports Optional, List, and nested types
- Default values become GraphQL defaults

### @fraiseql.query

Registers a function as a GraphQL query.

```python
@fraiseql.query
async def query_name(info, arg1: Type1, arg2: Type2 = default) -> ReturnType:
    pass
```

**Parameters**:
- `info` (required, first): GraphQL resolve info containing context
- `*args`: Query arguments with type annotations
- `**kwargs`: Query arguments with defaults

**Returns**: Must have type annotation

**Usage**:
```python
@fraiseql.query
async def user(info, id: UUID) -> User | None:
    db = info.context["db"]
    return await db.find_one("user_view", id=id)

@fraiseql.query
async def users(
    info,
    limit: int = 20,
    offset: int = 0,
    role: str | None = None
) -> list[User]:
    db = info.context["db"]
    filters = {"role": role} if role else {}
    return await db.find("user_view", **filters, limit=limit, offset=offset)
```

**Important**:
- First parameter MUST be `info`
- Return type annotation is required
- Async is recommended but not required
- Automatically registered when module is imported

### @fraiseql.mutation

Registers a function as a GraphQL mutation.

```python
@fraiseql.mutation
async def mutation_name(info, input: InputType) -> ReturnType:
    pass
```

**Usage**:
```python
@fraise_input
class CreateUserInput:
    email: str
    name: str

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    db = info.context["db"]
    # Perform mutation
    return User(...)
```

### register_type_for_view()

Registers a type class for a database view (used in development mode).

```python
register_type_for_view(view_name: str, type_class: type) -> None
```

**Parameters**:
- `view_name`: The database view name
- `type_class`: The Python type class decorated with @fraise_type

**Usage**:
```python
from fraiseql.db import register_type_for_view

@fraise_type
class Product:
    id: UUID
    name: str
    price: Decimal

# Register for development mode instantiation
register_type_for_view("product_view", Product)
```

### @fraise_input

Defines a GraphQL input type.

```python
@fraise_input
class InputTypeName:
    field: FieldType
```

**Usage**:
```python
@fraise_input
class UpdateUserInput:
    id: UUID
    name: str | None = None
    email: str | None = None
```

**Important - Dict vs Object Handling**:

GraphQL may pass input types as either typed objects or dicts. Always handle both cases:

```python
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    # Handle both dict and object inputs
    if where:
        # Option 1: Helper function approach (recommended)
        def get_field(field_name: str):
            if isinstance(where, dict):
                return where.get(field_name)
            else:
                return getattr(where, field_name, None)

        # Option 2: Direct checking
        name = where.get('name') if isinstance(where, dict) else where.name
```

### @fraise_field

Adds metadata to type fields or defines resolver methods.

```python
# For field metadata
field_name: Type = fraise_field(description="...", default=value)

# For resolver methods
@fraise_field
def field_name(self, root, info) -> Type:
    pass
```

**Usage**:
```python
@fraise_type
class User:
    email: str = fraise_field(description="User's email address")
    name: str = fraise_field(description="Display name", default="Anonymous")

@fraise_type
class QueryRoot:
    @fraise_field
    async def current_user(self, root, info) -> User | None:
        user = info.context.get("user")
        if not user:
            return None
        return await fetch_user(user.id)
```

## Repository

### FraiseQLRepository

Main class for database operations.

```python
class FraiseQLRepository:
    def __init__(self, pool: AsyncConnectionPool, context: dict[str, Any] | None = None)
    async def find(self, view_name: str, **kwargs) -> list[Any]
    async def find_one(self, view_name: str, **kwargs) -> Any | None
    async def run(self, query: DatabaseQuery) -> list[dict[str, Any]]
```

#### Constructor

```python
repo = FraiseQLRepository(pool, context={"tenant_id": "123", "mode": "development"})
```

**Parameters**:
- `pool`: psycopg AsyncConnectionPool
- `context`: Optional context dictionary
  - `mode`: "development" or "production" (default: from env or "production")
  - `tenant_id`: For multi-tenant filtering
  - Any custom values

#### find()

Finds multiple records from a view.

```python
async def find(self, view_name: str, **kwargs) -> list[Any]
```

**Parameters**:
- `view_name`: Name of the database view
- `**kwargs`: Filter conditions
  - `where`: Where type instance for operator-based filtering (optional)
  - `limit`: Maximum records (optional)
  - `offset`: Skip records (optional)
  - `order_by`: Order by clause (optional)
  - Any column names for simple equality filtering

**Returns**:
- Development mode: List of typed objects
- Production mode: List of dicts

**Usage**:
```python
# Simple filtering
users = await db.find("user_view", status="active")

# With where type (operator-based filtering)
where = UserWhere()
where.age = {"gte": 18, "lt": 65}
where.status = {"eq": "active"}
users = await db.find("user_view", where=where)

# With pagination
users = await db.find("user_view",
    where=where,
    limit=10,
    offset=20,
    order_by="created_at DESC"
)

# Multi-tenant query with where type
users = await db.find("user_view",
    where=where,
    tenant_id=tenant_id  # Additional filter
)
```

**Common Issues**:
```python
# ❌ WRONG: Passing invalid kwargs
users = await db.find("user_view", invalid_column="value")

# ✅ CORRECT: Only use actual column names
users = await db.find("user_view", status="active")

# ❌ WRONG: Forgetting await
users = db.find("user_view")  # Returns coroutine!

# ✅ CORRECT: Always await
users = await db.find("user_view")

# ❌ WRONG: Mixing where type with conflicting kwargs
where = UserWhere()
where.status = {"eq": "active"}
users = await db.find("user_view", where=where, status="inactive")

# ✅ CORRECT: Use either where type OR kwargs
users = await db.find("user_view", where=where)
```

#### find_one()

Finds a single record from a view.

```python
async def find_one(self, view_name: str, **kwargs) -> Any | None
```

**Parameters**:
- Same as `find()`, but returns at most one record

**Usage**:
```python
# Find by ID
user = await db.find_one("user_view", id=user_id)

# With where type
where = UserWhere()
where.email = {"eq": "user@example.com"}
user = await db.find_one("user_view", where=where)
```

```python
async def find_one(self, view_name: str, **kwargs) -> Any | None
```

**Parameters**: Same as `find()`

**Returns**:
- Development mode: Typed object or None
- Production mode: Dict or None

**Usage**:
```python
# Find by ID
user = await db.find_one("user_view", id=user_id)

# Find by unique field
user = await db.find_one("user_view", email="user@example.com")

# Multi-tenant single record
user = await db.find_one("user_view",
    id=user_id,
    tenant_id=tenant_id
)
```

#### run()

Executes raw SQL queries (advanced usage).

```python
async def run(self, query: DatabaseQuery) -> list[dict[str, Any]]
```

**Usage**:
```python
from fraiseql.db import DatabaseQuery
from psycopg.sql import SQL

query = DatabaseQuery(
    statement=SQL("SELECT * FROM users WHERE created_at > %s"),
    params=(datetime.now() - timedelta(days=7),),
    fetch_result=True
)
results = await db.run(query)
```

## App Creation

### create_fraiseql_app()

Creates a FastAPI application with GraphQL endpoint.

```python
def create_fraiseql_app(
    *,
    database_url: str | None = None,
    types: Sequence[type] = (),
    queries: Sequence[type] = (),
    mutations: Sequence[Callable] = (),
    config: FraiseQLConfig | None = None,
    auth: AuthProvider | None = None,
    context_getter: Callable[[Request], Awaitable[dict]] | None = None,
    title: str = "FraiseQL API",
    version: str = "1.0.0",
    production: bool = False,
    app: FastAPI | None = None,
) -> FastAPI
```

**Parameters**:
- `database_url`: PostgreSQL connection string
- `types`: List of @fraise_type decorated classes
- `queries`: List of QueryRoot classes (if not using @fraiseql.query)
- `mutations`: List of mutation functions (if not using @fraiseql.mutation)
- `config`: Full configuration object (overrides other params)
- `auth`: Authentication provider
- `context_getter`: Custom context builder function
- `title`: API title for documentation
- `version`: API version
- `production`: Enable production optimizations
- `app`: Existing FastAPI app to extend

**Returns**: FastAPI application instance

**Usage**:
```python
# Simple usage
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post, Comment],
    production=False  # Enables GraphQL Playground
)

# With custom context
async def get_context(request: Request) -> dict[str, Any]:
    pool = request.app.state.db_pool
    tenant_id = request.headers.get("tenant-id")

    repo = FraiseQLRepository(pool, context={"tenant_id": tenant_id})

    return {
        "db": repo,
        "tenant_id": tenant_id,
        "custom": "value"
    }

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    context_getter=get_context
)

# With authentication
from fraiseql.auth import Auth0Config

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    auth=Auth0Config(
        domain="myapp.auth0.com",
        api_identifier="https://api.myapp.com"
    )
)
```

## Context

### Default Context Structure

```python
{
    "db": FraiseQLRepository,          # Database access
    "user": UserContext | None,        # Authenticated user
    "authenticated": bool,             # Is user authenticated
    "request": Request,                # FastAPI request
    "loader_registry": LoaderRegistry, # DataLoader registry
}
```

### Accessing Context

```python
@fraiseql.query
async def my_query(info) -> Any:
    # Get database
    db = info.context["db"]

    # Get user (may be None)
    user = info.context.get("user")

    # Check authentication
    if info.context["authenticated"]:
        # User is logged in
        pass

    # Get custom values
    tenant_id = info.context.get("tenant_id")
```

### Custom Context Builder

```python
async def get_context(request: Request) -> dict[str, Any]:
    """Build custom context for each request."""
    # Get default context first (recommended)
    from fraiseql.fastapi.dependencies import build_graphql_context
    context = await build_graphql_context(request)

    # Add custom values
    context.update({
        "tenant_id": request.headers.get("tenant-id"),
        "api_version": "2.0",
        "feature_flags": {
            "new_feature": True
        }
    })

    return context
```

## Types

### Supported Python Types

| Python Type | GraphQL Type | Notes |
|------------|--------------|-------|
| `str` | String | |
| `int` | Int | |
| `float` | Float | |
| `bool` | Boolean | |
| `UUID` | ID | From uuid module |
| `datetime` | DateTime | ISO format |
| `date` | Date | ISO format |
| `time` | Time | ISO format |
| `Decimal` | String | Preserved precision |
| `list[T]` | [T] | |
| `T \| None` | T | Nullable |
| `Optional[T]` | T | Nullable |
| `dict[str, T]` | JSON | Custom scalar |

### Custom Scalars

```python
from fraiseql.types.scalars import EmailAddress, JSON

@fraise_type
class User:
    email: EmailAddress  # Validates email format
    metadata: JSON      # Any JSON data
```

## Authentication

### @requires_auth

Requires user to be authenticated.

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def protected_query(info) -> Any:
    user = info.context["user"]  # Guaranteed to exist
    # ...
```

### @requires_role

Requires user to have specific role.

```python
from fraiseql.auth import requires_role

@fraiseql.query
@requires_role("admin")
async def admin_query(info) -> Any:
    # Only users with "admin" role can access
    pass
```

### @requires_permission

Requires user to have specific permission.

```python
from fraiseql.auth import requires_permission

@fraiseql.query
@requires_permission("users:read")
async def list_users(info) -> list[User]:
    # Only users with "users:read" permission
    pass
```

### UserContext

User information available in context.

```python
class UserContext:
    user_id: str           # Unique user identifier
    email: str | None      # User email
    roles: list[str]       # User roles
    permissions: list[str] # User permissions

    def has_role(self, role: str) -> bool
    def has_permission(self, permission: str) -> bool
```

## Utilities

### create_graphql_where_input()

Creates a GraphQL-compatible where input type with operator-based filtering.

```python
from fraiseql.sql import create_graphql_where_input

UserWhereInput = create_graphql_where_input(User)
```

**Parameters**:
- `cls`: A dataclass or type decorated with `@fraise_type`
- `name`: Optional custom name for the generated type (defaults to `{ClassName}WhereInput`)

**Returns**:
- A GraphQL input type decorated with `@fraise_input` that supports operator-based filtering

**Features**:
- Automatic conversion to SQL where types in repository methods
- Rich operator support for all field types
- Type-safe filtering with GraphQL schema integration
- No manual conversion boilerplate required

**Usage**:
```python
from fraiseql import fraise_type, fraiseql
from fraiseql.sql import create_graphql_where_input
from datetime import datetime
from uuid import UUID

@fraise_type
class User:
    id: UUID
    name: str
    email: str
    age: int
    is_active: bool
    created_at: datetime

# Generate GraphQL where input type
UserWhereInput = create_graphql_where_input(User)

# Use directly in resolver - no manual conversion!
@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]
    # FraiseQL automatically converts GraphQL input to SQL where
    return await db.find("user_view", where=where)
```

**GraphQL Schema Generated**:
```graphql
input UserWhereInput {
  id: UUIDFilter
  name: StringFilter
  email: StringFilter
  age: IntFilter
  isActive: BooleanFilter
  createdAt: DateTimeFilter
}

input StringFilter {
  eq: String
  neq: String
  contains: String
  startswith: String
  endswith: String
  in: [String!]
  nin: [String!]
  isnull: Boolean
}

input IntFilter {
  eq: Int
  neq: Int
  gt: Int
  gte: Int
  lt: Int
  lte: Int
  in: [Int!]
  nin: [Int!]
  isnull: Boolean
}
```

**Client Usage**:
```graphql
query GetUsers($where: UserWhereInput) {
  users(where: $where) {
    id
    name
    email
  }
}
```

With variables:
```json
{
  "where": {
    "age": {"gte": 18, "lt": 65},
    "isActive": {"eq": true},
    "email": {"contains": "@example.com"},
    "name": {"startswith": "John"}
  }
}
```

### safe_create_where_type()

Dynamically generates a SQL where type with operator-based filtering from a data class.

```python
from fraiseql.sql.where_generator import safe_create_where_type

WhereType = safe_create_where_type(DataClass)
```

**Parameters**:
- `cls`: A dataclass or type decorated with `@fraise_type`

**Returns**:
- A new dataclass type with operator fields for each original field

**Supported Operators**:
- **All types**: `eq` (equal), `neq` (not equal), `isnull` (null check)
- **Numeric types** (int, float, Decimal): `gt`, `gte`, `lt`, `lte`, `in`
- **String types**: `contains`, `startswith`, `in`
- **Date/datetime types**: `gt`, `gte`, `lt`, `lte`
- **Boolean types**: `eq`, `neq`

**Usage**:
```python
from fraiseql import fraise_type
from fraiseql.sql.where_generator import safe_create_where_type
from decimal import Decimal
from datetime import datetime

@fraise_type
class Product:
    id: UUID
    name: str
    price: Decimal
    stock: int
    created_at: datetime
    is_active: bool

# Generate where type
ProductWhere = safe_create_where_type(Product)

# Use in queries
@fraiseql.query
async def products(info, where: ProductWhere | None = None) -> list[Product]:
    db = info.context["db"]
    return await db.find("product_view", where=where)
```

**Example Filter Usage**:
```python
# Create filter instance
where = ProductWhere()

# Equality
where.name = {"eq": "Widget"}
where.is_active = {"eq": True}

# Comparison
where.price = {"gt": 10.0, "lte": 100.0}  # 10 < price <= 100
where.stock = {"gte": 1}  # In stock

# String operations
where.name = {"contains": "Widget"}
where.name = {"startswith": "W"}

# Date filtering
where.created_at = {"gte": datetime(2024, 1, 1)}

# Null checks
where.category = {"isnull": False}  # Non-null only

# List operations
where.status = {"in": ["active", "pending"]}
```

**SQL Generation**:
The where type generates parameterized SQL with automatic type casting:
- `(data->>'price')::numeric > 10.0`
- `(data->>'is_active')::boolean = true`
- `(data->>'created_at')::timestamp >= '2024-01-01'`

### Error Handling

```python
from graphql import GraphQLError

@fraiseql.query
async def my_query(info, id: UUID) -> User:
    db = info.context["db"]
    user = await db.find_one("user_view", id=id)

    if not user:
        raise GraphQLError(f"User {id} not found")

    return user
```

### Logging

```python
import logging

logger = logging.getLogger(__name__)

@fraiseql.query
async def my_query(info) -> Any:
    logger.info("Query started")
    try:
        result = await do_something()
        logger.info("Query completed successfully")
        return result
    except Exception as e:
        logger.error(f"Query failed: {e}")
        raise GraphQLError("Internal server error")
```

### Environment Variables

```python
import os

# Mode detection
FRAISEQL_ENV = os.getenv("FRAISEQL_ENV", "production")

# In repository
mode = context.get("mode") or os.getenv("FRAISEQL_ENV", "production")
```

## Filtering and Where Inputs

### Basic Where Input Pattern

```python
@fraise_input
class UserWhereInput:
    id: UUID | None = None
    email: str | None = None
    status: str | None = None
    is_active: bool | None = None

@fraiseql.query
async def users(info, where: UserWhereInput | None = None) -> list[User]:
    db = info.context["db"]

    # Build filters from where input
    filters = _build_user_filters(where)

    return await db.find("user_view", **filters)

def _build_user_filters(where: UserWhereInput | dict | None) -> dict[str, Any]:
    """Convert where input to database filters."""
    filters = {}

    if not where:
        return filters

    # Handle both dict and object input
    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    # Add filters for each field
    for field in ['id', 'email', 'status']:
        value = get_field(field)
        if value is not None:
            filters[field] = value

    # Boolean filters
    if get_field('is_active') is not None:
        filters['is_active'] = get_field('is_active')

    return filters
```

### Complex Filtering with Date Ranges

For filters that need SQL operators (>=, <=, LIKE), use custom SQL:

```python
@fraise_input
class OrderWhereInput:
    status: str | None = None
    created_after: datetime | None = None
    created_before: datetime | None = None
    total_min: Decimal | None = None
    total_max: Decimal | None = None

@fraiseql.query
async def orders(info, where: OrderWhereInput | None = None) -> list[Order]:
    db = info.context["db"]

    # Check if we need custom SQL
    if where and _needs_custom_sql(where):
        return await _filter_orders_custom(db, where)

    # Otherwise use standard filtering
    filters = _build_order_filters(where)
    return await db.find("order_view", **filters)

async def _filter_orders_custom(db: FraiseQLRepository, where: OrderWhereInput) -> list[Order]:
    from fraiseql.db import DatabaseQuery
    from psycopg.sql import SQL

    conditions = []
    params = {}

    get_field = (lambda f: where.get(f)) if isinstance(where, dict) else (lambda f: getattr(where, f, None))

    if get_field('status'):
        conditions.append("status = %(status)s")
        params['status'] = get_field('status')

    if get_field('created_after'):
        conditions.append("created_at >= %(created_after)s")
        params['created_after'] = get_field('created_after')

    if get_field('created_before'):
        conditions.append("created_at <= %(created_before)s")
        params['created_before'] = get_field('created_before')

    if get_field('total_min') is not None:
        conditions.append("total_amount >= %(total_min)s")
        params['total_min'] = get_field('total_min')

    if get_field('total_max') is not None:
        conditions.append("total_amount <= %(total_max)s")
        params['total_max'] = get_field('total_max')

    where_clause = " AND ".join(conditions) if conditions else "TRUE"

    query = DatabaseQuery(
        statement=SQL(f"SELECT * FROM order_view WHERE {where_clause}"),
        params=params,
        fetch_result=True
    )

    results = await db.run(query)

    # Handle mode-specific returns
    if db.mode == "development":
        return [Order(**row["data"]) for row in results]
    return results
```

### Boolean Filters with Pre-computed Views

For best performance, pre-compute boolean flags in your database views:

```sql
CREATE VIEW machine_view AS
SELECT
    id,
    tenant_id,
    status,
    -- Pre-computed boolean columns
    (removed_at IS NULL) as is_active,
    (stock_location_id IS NOT NULL) as is_stock,
    -- JSONB data
    jsonb_build_object(
        'id', id,
        'status', status,
        'is_active', (removed_at IS NULL),
        'is_stock', (stock_location_id IS NOT NULL)
    ) as data
FROM machines;
```

Then filter directly:
```python
filters = {}
if where.is_active is not None:
    filters['is_active'] = where.is_active
if where.is_stock is not None:
    filters['is_stock'] = where.is_stock
```

## Common Patterns

### Pagination

```python
@fraise_type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: str | None
    end_cursor: str | None

@fraise_type
class UserConnection:
    edges: list[UserEdge]
    page_info: PageInfo
    total_count: int

@fraiseql.query
async def users_paginated(
    info,
    first: int = 20,
    after: str | None = None
) -> UserConnection:
    # Implementation
    pass
```

### Filtering

```python
@fraise_input
class UserFilter:
    role: str | None = None
    status: str | None = None
    created_after: datetime | None = None

@fraiseql.query
async def users(info, filter: UserFilter | None = None) -> list[User]:
    db = info.context["db"]

    kwargs = {}
    if filter:
        if filter.role:
            kwargs["role"] = filter.role
        if filter.status:
            kwargs["status"] = filter.status

    return await db.find("user_view", **kwargs)
```

### Batch Operations

```python
@fraiseql.mutation
async def update_users(info, ids: list[UUID], input: UpdateUserInput) -> list[User]:
    db = info.context["db"]
    updated = []

    for user_id in ids:
        # Update each user
        user = await update_single_user(db, user_id, input)
        updated.append(user)

    return updated
```

## Repository Troubleshooting

### Common Repository Issues

#### Issue: `'FraiseQLRepository' object has no attribute 'find'`

**Cause**: Version mismatch or incorrect repository setup.

**Solutions**:
```python
# 1. Check FraiseQL version
# pip show fraiseql  # Should be v0.1.0a14+

# 2. Verify repository type in query
@fraiseql.query
async def debug_repo(info) -> str:
    db = info.context["db"]
    return f"Type: {type(db)}, Has find: {hasattr(db, 'find')}"

# 3. Check context getter
from fraiseql.db import FraiseQLRepository

async def get_context(request: Request) -> dict[str, Any]:
    pool = request.app.state.db_pool
    repo = FraiseQLRepository(pool, context={"mode": "development"})
    return {"db": repo, "request": request}
```

#### Issue: Repository returns `None` or wrong data

**Cause**: View doesn't exist or lacks required `data` column.

**Solutions**:
```sql
-- Verify view exists
SELECT * FROM information_schema.views WHERE table_name = 'your_view';

-- Verify view has data column
SELECT column_name FROM information_schema.columns
WHERE table_name = 'your_view' AND column_name = 'data';

-- Correct view structure
CREATE VIEW user_view AS
SELECT
    id, status, tenant_id,  -- Filtering columns
    jsonb_build_object(
        'id', id,
        'name', name,
        'email', email
    ) as data  -- REQUIRED!
FROM users;
```

#### Issue: Mode detection not working

**Cause**: Mode not set correctly in context or environment.

**Solutions**:
```python
# Option 1: Set in context
repo = FraiseQLRepository(pool, context={"mode": "development"})

# Option 2: Set environment variable
# FRAISEQL_ENV=development

# Option 3: Debug mode detection
@fraiseql.query
async def debug_mode(info) -> str:
    db = info.context["db"]
    return f"Mode: {db.mode}"
```

### Repository Testing

#### Test Repository Setup
```python
@fraiseql.query
async def test_repository_setup(info) -> dict[str, str]:
    """Test if repository is properly configured."""
    db = info.context["db"]

    return {
        "repository_type": type(db).__name__,
        "has_find": str(hasattr(db, 'find')),
        "has_find_one": str(hasattr(db, 'find_one')),
        "mode": getattr(db, 'mode', 'unknown'),
        "pool_available": str(hasattr(db, '_pool'))
    }
```

#### Test View Access
```python
@fraiseql.query
async def test_view_access(info, view_name: str) -> dict[str, Any]:
    """Test if view is accessible."""
    db = info.context["db"]

    try:
        # Try to fetch one record
        result = await db.find(view_name, limit=1)
        return {
            "success": True,
            "count": len(result),
            "sample": result[0] if result else None
        }
    except Exception as e:
        return {
            "success": False,
            "error": str(e)
        }
```

## Best Practices

1. **Always type annotate** - Required for schema generation
2. **Use async functions** - Better performance
3. **Handle errors gracefully** - User-friendly messages
4. **Log important operations** - For debugging
5. **Validate inputs** - Don't trust client data
6. **Use filtering columns** - Not JSONB queries
7. **Test both modes** - Development and production
8. **Debug repository setup** - Use test queries to verify setup
9. **Check view structure** - Ensure data column exists
10. **Version compatibility** - Keep FraiseQL updated
