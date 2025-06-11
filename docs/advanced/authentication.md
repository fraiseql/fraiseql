# Authentication

FraiseQL provides a flexible authentication system with built-in support for development password protection, Auth0, and custom authentication providers.

## Development Authentication

For quick development protection, FraiseQL includes built-in basic authentication:

```bash
# Enable development password protection
export FRAISEQL_DEV_PASSWORD=mysecretpassword

# Optional: Custom username (default: "admin")
export FRAISEQL_DEV_USERNAME=developer
```

When enabled, GraphQL Playground will prompt for credentials. This is automatically disabled in production mode for security.

```python
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=False,  # Dev mode with auth protection
    # dev_auth_password="override",  # Optional programmatic override
)
```

## Production Authentication with Auth0

For production applications, use Auth0:

```python
import fraiseql
from fraiseql.auth import Auth0Provider

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=True,
    auth_provider=Auth0Provider(
        domain="your-domain.auth0.com",
        api_identifier="your-api-identifier"
    )
)
```

## Authentication Decorators

FraiseQL provides decorators to protect your resolvers:

### @requires_auth

Requires a valid authentication token:

```python
from fraiseql.auth import requires_auth

@fraiseql.type
class Query:
    @fraiseql.field
    @requires_auth
    async def me(self, info: fraiseql.Info) -> User:
        """Get the current authenticated user."""
        user_context = info.context["user"]
        user_id = UUID(user_context.user_id)

        repo = CQRSRepository(info.context["db"])
        user_data = await repo.get_by_id("v_users", user_id)
        return User.from_dict(user_data)
```

### @requires_role

Requires specific user roles:

```python
from fraiseql.auth import requires_role

@fraiseql.type
class Mutation:
    @fraiseql.field
    @requires_role("admin")
    async def delete_user(self, user_id: UUID, info: fraiseql.Info) -> bool:
        """Delete a user (admin only)."""
        repo = CQRSRepository(info.context["db"])
        await repo.delete("tb_users", user_id)
        return True
```

### @requires_permission

Requires specific permissions:

```python
from fraiseql.auth import requires_permission

@fraiseql.type
class Mutation:
    @fraiseql.field
    @requires_permission("posts:write")
    async def create_post(self, input: CreatePostInput, info: fraiseql.Info) -> Post:
        """Create a new post."""
        # Implementation here
        pass
```

## User Context

When a user is authenticated, FraiseQL adds user information to the GraphQL context:

```python
@fraiseql.field
@requires_auth
async def my_posts(self, info: fraiseql.Info) -> list[Post]:
    """Get posts for the current user."""
    user_context = info.context["user"]

    # Available fields:
    user_id = user_context.user_id      # str: User's unique ID
    email = user_context.email          # str: User's email
    roles = user_context.roles          # list[str]: User roles
    permissions = user_context.permissions  # list[str]: User permissions

    # Query user's posts
    repo = CQRSRepository(info.context["db"])
    posts_data = await repo.query(
        "v_posts",
        filters={"author_id": user_id}
    )
    return [Post.from_dict(data) for data in posts_data]
```

## Custom Authentication Provider

Create your own authentication provider by implementing the `AuthProvider` protocol:

```python
from fraiseql.auth.base import AuthProvider, UserContext
from typing import Optional

class JWTAuthProvider(AuthProvider):
    """Custom JWT authentication provider."""

    def __init__(self, secret_key: str, algorithm: str = "HS256"):
        self.secret_key = secret_key
        self.algorithm = algorithm

    async def authenticate(self, request) -> Optional[UserContext]:
        """Extract and validate JWT token from request."""
        auth_header = request.headers.get("Authorization")
        if not auth_header or not auth_header.startswith("Bearer "):
            return None

        token = auth_header[7:]  # Remove "Bearer " prefix

        try:
            payload = jwt.decode(token, self.secret_key, algorithms=[self.algorithm])

            return UserContext(
                user_id=payload["sub"],
                email=payload.get("email"),
                roles=payload.get("roles", []),
                permissions=payload.get("permissions", [])
            )
        except jwt.InvalidTokenError:
            return None

# Use your custom provider
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    auth_provider=JWTAuthProvider(secret_key="your-secret-key")
)
```

## Security Best Practices

1. **Use development auth** during development
2. **Always use HTTPS** in production
3. **Validate JWT signatures** properly
4. **Use strong secret keys** (256+ bits)
5. **Implement proper CORS** policies
6. **Log authentication events** for monitoring

## Environment Configuration

```bash
# Development Authentication
FRAISEQL_DEV_PASSWORD=mysecretpassword
FRAISEQL_DEV_USERNAME=admin

# Auth0 Configuration
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_API_IDENTIFIER=your-api-identifier

# JWT Configuration
JWT_SECRET_KEY=your-very-secure-secret-key
JWT_ALGORITHM=HS256
```

This authentication system ensures your FraiseQL API is secure in both development and production environments.
