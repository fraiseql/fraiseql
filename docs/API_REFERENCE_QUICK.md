# FraiseQL API Quick Reference

## Core Imports

```python
import fraiseql
from fraiseql import fraise_field, create_fraiseql_app
```

## Decorators

### @fraiseql.type
Define a GraphQL output type:
```python
@fraiseql.type
class User:
    id: int
    name: str
    email: str
```

### @fraiseql.input
Define a GraphQL input type:
```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: str
```

### @fraiseql.query
Define a GraphQL query:
```python
@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:
    # info.context contains: db, user, authenticated, loader_registry
    return User(...)
```

### @fraiseql.mutation
Define a GraphQL mutation:
```python
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    # Mutation logic here
    return User(...)
```

### @fraiseql.subscription
Define a GraphQL subscription:
```python
@fraiseql.subscription
async def user_created(info) -> AsyncIterator[User]:
    # Subscription logic here
    yield User(...)
```

### @fraiseql.enum
Define a GraphQL enum:
```python
@fraiseql.enum
class UserRole(Enum):
    ADMIN = "admin"
    USER = "user"
    GUEST = "guest"
```

### @fraiseql.interface
Define a GraphQL interface:
```python
@fraiseql.interface
class Node:
    id: int
```

### @fraiseql.field
Add custom field resolvers to types:
```python
@fraiseql.type
class User:
    id: int

    @fraiseql.field
    async def posts(self, info) -> List[Post]:
        # Custom field resolver
        return []
```

### @fraiseql.dataloader_field
Automatic DataLoader integration (v0.1.0a4+):
```python
@fraiseql.type
class Post:
    author_id: int

    @fraiseql.dataloader_field(UserDataLoader, key_field="author_id")
    async def author(self, info) -> Optional[User]:
        # Auto-implemented with DataLoader
        pass
```

## Field Configuration

### fraise_field()
Configure field metadata:
```python
from fraiseql import fraise_field

@fraiseql.type
class User:
    name: str = fraise_field(
        description="User's full name",
        deprecation_reason="Use firstName and lastName instead"
    )
    email: str = fraise_field(description="Primary email")
    is_active: bool = fraise_field(default=True)
```

## App Creation

### create_fraiseql_app()
Create a FraiseQL FastAPI application:

```python
app = fraiseql.create_fraiseql_app(
    # Database (optional)
    database_url="postgresql://user:pass@localhost/dbname",

    # Type registration
    types=[User, Post, Comment],  # All @fraiseql.type classes

    # Configuration
    title="My GraphQL API",
    version="1.0.0",
    description="API description",

    # Development settings
    production=False,  # Enables GraphQL Playground

    # Authentication (optional)
    auth=Auth0Config(...),  # Or custom AuthProvider

    # Advanced options
    context_getter=custom_context_function,  # Custom context
    app=existing_fastapi_app,  # Use existing FastAPI app
)
```

## Authentication

### requires_auth
Require authentication for queries/mutations:
```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_profile(info) -> User:
    user_context = info.context["user"]
    # user_context has: user_id, email, roles
```

### requires_role
Require specific role:
```python
from fraiseql.auth import requires_role

@fraiseql.mutation
@requires_auth
@requires_role("admin")
async def delete_user(info, id: int) -> bool:
    # Only admins can delete users
    return True
```

## Database Integration

### Using FraiseQLRepository
```python
from fraiseql.repository import FraiseQLRepository

@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:
    db: FraiseQLRepository = info.context["db"]

    # Execute raw SQL
    result = await db.fetch_one(
        "SELECT * FROM users WHERE id = %s",
        (id,)
    )

    # Use JSON views
    result = await db.select_from_json_view(
        "v_users",
        where={"id": id}
    )

    return User(**result) if result else None
```

## DataLoader Integration

### Create a DataLoader
```python
from fraiseql.optimization import DataLoader

class UserDataLoader(DataLoader[int, dict]):
    def __init__(self, db):
        super().__init__()
        self.db = db

    async def batch_load(self, user_ids: List[int]) -> List[Optional[dict]]:
        # Batch load users
        users = await self.db.fetch_many(
            "SELECT * FROM users WHERE id = ANY(%s)",
            (user_ids,)
        )
        # Return in same order as requested
        user_map = {u["id"]: u for u in users}
        return [user_map.get(uid) for uid in user_ids]
```

### Use DataLoader in resolvers
```python
from fraiseql.optimization import get_loader

@fraiseql.type
class Post:
    author_id: int

    @fraiseql.field
    async def author(self, info) -> Optional[User]:
        loader = get_loader(UserDataLoader)
        user_data = await loader.load(self.author_id)
        return User(**user_data) if user_data else None
```

## Error Handling

### Success/Failure pattern
```python
@fraiseql.success
class CreateUserSuccess:
    user: User
    message: str = "User created successfully"

@fraiseql.failure
class CreateUserError:
    message: str
    code: str

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> Union[CreateUserSuccess, CreateUserError]:
    try:
        # Create user
        return CreateUserSuccess(user=new_user)
    except Exception as e:
        return CreateUserError(message=str(e), code="CREATE_FAILED")
```

## Common Patterns

### Pagination
```python
from fraiseql import PaginatedResponse, create_connection

@fraiseql.query
async def users(info, first: int = 10, after: str = None) -> PaginatedResponse[User]:
    # Implement pagination logic
    users = [...]  # Your data
    total_count = 100

    return create_connection(
        nodes=users,
        total_count=total_count,
        has_next_page=True,
        has_previous_page=False
    )
```

### Custom context
```python
async def get_context(request):
    return {
        "db": db_pool,
        "user": current_user,
        "request": request,
        # Add custom context
    }

app = fraiseql.create_fraiseql_app(
    types=[...],
    context_getter=get_context
)
```

## Environment Variables

```bash
# Database
DATABASE_URL=postgresql://user:pass@localhost/dbname

# Auth0 (if using)
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_API_IDENTIFIER=https://your-api
AUTH0_ALGORITHMS=RS256

# Development
FRAISEQL_PRODUCTION=false
FRAISEQL_DEBUG=true
```
