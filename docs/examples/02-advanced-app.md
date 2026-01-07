# Example 2: Advanced Multi-Feature App

A production-like FraiseQL application with subscriptions, custom inputs, error handling, and organized structure.

**Features:**
- Complex types with relationships
- Subscriptions (real-time updates)
- Custom input types
- Error handling
- Organized module structure
- Testing patterns
- Production deployment

## Project Structure

```
advanced_api/
├── main.py                  # Entry point
├── config.py                # Configuration
├── types/
│   ├── __init__.py
│   ├── user.py
│   ├── post.py
│   ├── comment.py
│   └── common.py
├── queries/
│   ├── __init__.py
│   ├── user_queries.py
│   └── post_queries.py
├── mutations/
│   ├── __init__.py
│   ├── user_mutations.py
│   └── post_mutations.py
├── subscriptions/
│   ├── __init__.py
│   ├── user_subscriptions.py
│   └── post_subscriptions.py
├── inputs/
│   ├── __init__.py
│   └── dtos.py
├── tests/
│   ├── conftest.py
│   ├── test_users.py
│   ├── test_posts.py
│   └── test_mutations.py
└── requirements.txt
```

## Implementation

### config.py

```python
"""Application configuration."""
import os
from dataclasses import dataclass

@dataclass
class Config:
    """Application configuration."""
    database_url: str = os.getenv(
        "DATABASE_URL",
        "postgresql://user:password@localhost/db"
    )
    host: str = os.getenv("HOST", "0.0.0.0")
    port: int = int(os.getenv("PORT", "8000"))
    environment: str = os.getenv("ENVIRONMENT", "development")
    debug: bool = environment == "development"

config = Config()
```

### types/common.py

```python
"""Common types shared across the application."""
from fraiseql.fields import ID
from dataclasses import dataclass
from datetime import datetime

@dataclass
class Timestamp:
    """Mixin for timestamp fields."""
    created_at: datetime
    updated_at: datetime
```

### types/user.py

```python
"""User type definitions."""
from fraiseql import type as fraise_type
from fraiseql.fields import ID
from dataclasses import dataclass
from datetime import datetime

@fraise_type(sql_source="users")
@dataclass
class User:
    """A user in the system."""
    id: ID
    username: str
    email: str
    full_name: str | None = None
    is_active: bool = True
    created_at: datetime = None
    updated_at: datetime = None
```

### types/post.py

```python
"""Post type definitions."""
from fraiseql import type as fraise_type
from fraiseql.fields import ID
from dataclasses import dataclass
from datetime import datetime
from .user import User

@fraise_type(sql_source="posts")
@dataclass
class Post:
    """A blog post."""
    id: ID
    title: str
    content: str
    author_id: ID
    author: User | None = None
    published: bool = False
    view_count: int = 0
    created_at: datetime = None
    updated_at: datetime = None

@fraise_type(sql_source="comments")
@dataclass
class Comment:
    """A comment on a post."""
    id: ID
    content: str
    author_id: ID
    post_id: ID
    created_at: datetime = None
```

### types/__init__.py

```python
"""Export all types."""
from .user import User
from .post import Post, Comment

__all__ = ["User", "Post", "Comment"]
```

### inputs/dtos.py

```python
"""Data transfer objects for input types."""
from dataclasses import dataclass
from typing import Optional

@dataclass
class CreateUserInput:
    """Input for creating a user."""
    username: str
    email: str
    full_name: Optional[str] = None
    password: str = ""  # Would be hashed in real app

@dataclass
class UpdateUserInput:
    """Input for updating a user."""
    username: Optional[str] = None
    email: Optional[str] = None
    full_name: Optional[str] = None
    is_active: Optional[bool] = None

@dataclass
class CreatePostInput:
    """Input for creating a post."""
    title: str
    content: str
    published: bool = False

@dataclass
class UpdatePostInput:
    """Input for updating a post."""
    title: Optional[str] = None
    content: Optional[str] = None
    published: Optional[bool] = None

@dataclass
class CreateCommentInput:
    """Input for creating a comment."""
    content: str
    post_id: str
```

### queries/user_queries.py

```python
"""User-related queries."""
from fraiseql import query
from typing import list, Optional
from ..types import User
from ..inputs.dtos import UpdateUserInput

@query
async def get_users(limit: int = 100, offset: int = 0) -> list[User]:
    """Fetch multiple users with pagination."""
    # Implement database query
    pass

@query
async def get_user(id: str) -> Optional[User]:
    """Fetch a specific user by ID."""
    # Implement database query
    pass

@query
async def get_user_by_username(username: str) -> Optional[User]:
    """Fetch user by username."""
    # Implement database query
    pass

@query
async def search_users(query: str) -> list[User]:
    """Search users by username or email."""
    # Implement database search
    pass
```

### queries/post_queries.py

```python
"""Post-related queries."""
from fraiseql import query
from typing import list, Optional
from ..types import Post, Comment

@query
async def get_posts(limit: int = 50, offset: int = 0) -> list[Post]:
    """Fetch multiple posts with pagination."""
    # Implement database query
    pass

@query
async def get_post(id: str) -> Optional[Post]:
    """Fetch a specific post by ID."""
    # Implement database query
    pass

@query
async def get_user_posts(user_id: str) -> list[Post]:
    """Fetch all posts by a specific user."""
    # Implement database query
    pass

@query
async def get_post_comments(post_id: str) -> list[Comment]:
    """Fetch all comments on a post."""
    # Implement database query
    pass

@query
async def search_posts(query: str) -> list[Post]:
    """Search posts by title or content."""
    # Implement database search
    pass
```

### mutations/user_mutations.py

```python
"""User-related mutations."""
from fraiseql import mutation
from typing import Optional
from ..types import User
from ..inputs.dtos import CreateUserInput, UpdateUserInput

@mutation
async def create_user(input: CreateUserInput) -> User:
    """Create a new user."""
    # Validate input
    if not input.username:
        raise ValueError("Username required")
    if not input.email:
        raise ValueError("Email required")

    # Implement creation logic
    pass

@mutation
async def update_user(id: str, input: UpdateUserInput) -> User:
    """Update an existing user."""
    # Implement update logic
    pass

@mutation
async def delete_user(id: str) -> bool:
    """Delete a user."""
    # Implement deletion logic
    return True

@mutation
async def deactivate_user(id: str) -> User:
    """Deactivate a user account."""
    # Implement deactivation
    pass
```

### mutations/post_mutations.py

```python
"""Post-related mutations."""
from fraiseql import mutation
from ..types import Post, Comment
from ..inputs.dtos import CreatePostInput, UpdatePostInput, CreateCommentInput

@mutation
async def create_post(author_id: str, input: CreatePostInput) -> Post:
    """Create a new post."""
    # Validate
    if not input.title:
        raise ValueError("Title required")

    # Implement creation
    pass

@mutation
async def update_post(id: str, input: UpdatePostInput) -> Post:
    """Update an existing post."""
    # Implement update
    pass

@mutation
async def delete_post(id: str) -> bool:
    """Delete a post."""
    # Implement deletion
    return True

@mutation
async def publish_post(id: str) -> Post:
    """Publish a post."""
    # Implement publish logic
    pass

@mutation
async def create_comment(author_id: str, input: CreateCommentInput) -> Comment:
    """Add a comment to a post."""
    # Validate
    if not input.content:
        raise ValueError("Comment content required")

    # Implement creation
    pass

@mutation
async def delete_comment(id: str) -> bool:
    """Delete a comment."""
    # Implement deletion
    return True
```

### subscriptions/user_subscriptions.py

```python
"""User-related subscriptions (real-time updates)."""
from fraiseql import subscription
from ..types import User

@subscription
async def on_user_created() -> User:
    """Subscribe to new user creations."""
    # Implement WebSocket subscription
    pass

@subscription
async def on_user_updated(user_id: str) -> User:
    """Subscribe to updates on a specific user."""
    # Implement WebSocket subscription
    pass
```

### subscriptions/post_subscriptions.py

```python
"""Post-related subscriptions (real-time updates)."""
from fraiseql import subscription
from ..types import Post, Comment

@subscription
async def on_post_created() -> Post:
    """Subscribe to new post creations."""
    # Implement WebSocket subscription
    pass

@subscription
async def on_post_published(author_id: str) -> Post:
    """Subscribe to posts published by a user."""
    # Implement WebSocket subscription
    pass

@subscription
async def on_comment_created(post_id: str) -> Comment:
    """Subscribe to comments on a post."""
    # Implement WebSocket subscription
    pass
```

### main.py

```python
"""Application entry point."""
from fraiseql.axum import create_axum_fraiseql_app
from config import config

# Import all types, queries, mutations, subscriptions
from types import User, Post, Comment
from queries.user_queries import (
    get_users, get_user, get_user_by_username, search_users
)
from queries.post_queries import (
    get_posts, get_post, get_user_posts, get_post_comments, search_posts
)
from mutations.user_mutations import (
    create_user, update_user, delete_user, deactivate_user
)
from mutations.post_mutations import (
    create_post, update_post, delete_post, publish_post, create_comment, delete_comment
)
from subscriptions.user_subscriptions import (
    on_user_created, on_user_updated
)
from subscriptions.post_subscriptions import (
    on_post_created, on_post_published, on_comment_created
)

# Create the application
app = create_axum_fraiseql_app(
    database_url=config.database_url,

    # Register types
    types=[User, Post, Comment],

    # Register queries
    queries=[
        get_users, get_user, get_user_by_username, search_users,
        get_posts, get_post, get_user_posts, get_post_comments, search_posts,
    ],

    # Register mutations
    mutations=[
        create_user, update_user, delete_user, deactivate_user,
        create_post, update_post, delete_post, publish_post, create_comment, delete_comment,
    ],

    # Register subscriptions
    subscriptions=[
        on_user_created, on_user_updated,
        on_post_created, on_post_published, on_comment_created,
    ],
)

# Verify registration
def verify_schema():
    """Verify the schema is properly configured."""
    registry = app.get_registry()
    counts = registry.count_registered()

    print("=" * 50)
    print("FraiseQL Advanced API - Schema Summary")
    print("=" * 50)
    print(registry.summary())
    print("=" * 50)

    assert counts["types"] >= 3, "Missing type registrations"
    assert counts["queries"] >= 5, "Missing query registrations"
    assert counts["mutations"] >= 6, "Missing mutation registrations"
    assert counts["subscriptions"] >= 3, "Missing subscription registrations"
    print("✅ All schemas registered correctly")

if __name__ == "__main__":
    verify_schema()

    # Start the server
    print(f"\n🚀 Starting server on {config.host}:{config.port}")
    print(f"   GraphQL endpoint: http://{config.host}:{config.port}/graphql")
    print(f"   Metrics: http://{config.host}:{config.port}/metrics")
    print(f"   Environment: {config.environment}")

    app.start(host=config.host, port=config.port)
```

### tests/conftest.py

```python
"""Test configuration and fixtures."""
import pytest
from fraiseql.axum import AxumRegistry, create_axum_fraiseql_app
from types import User, Post, Comment
from queries.user_queries import get_users, get_user
from queries.post_queries import get_posts
from mutations.user_mutations import create_user
from inputs.dtos import CreateUserInput
from config import Config

@pytest.fixture
def test_config():
    """Test configuration."""
    return Config(database_url="postgresql://test:test@localhost/test_db")

@pytest.fixture
def test_registry():
    """Clean registry for each test."""
    registry = AxumRegistry()
    registry.clear()
    yield registry
    registry.clear()

@pytest.fixture
def test_app(test_config, test_registry):
    """Create app for testing with isolated registry."""
    app = create_axum_fraiseql_app(
        database_url=test_config.database_url,
        types=[User, Post, Comment],
        queries=[get_users, get_user, get_posts],
        mutations=[create_user],
        registry=test_registry,
    )
    return app
```

### tests/test_users.py

```python
"""Tests for user queries and mutations."""
import pytest

def test_get_users_query(test_app):
    """Test fetching users."""
    query = """
    query {
        users(limit: 10) {
            id
            username
            email
        }
    }
    """
    result = test_app.execute_query(query)
    assert "data" in result
    assert "users" in result["data"]

def test_get_user_query(test_app):
    """Test fetching a specific user."""
    query = """
    query {
        user(id: "550e8400-e29b-41d4-a716-446655440000") {
            id
            username
            email
        }
    }
    """
    result = test_app.execute_query(query)
    assert "data" in result

def test_create_user_mutation(test_app):
    """Test creating a user."""
    mutation = """
    mutation {
        createUser(input: {
            username: "newuser",
            email: "new@example.com",
            fullName: "New User"
        }) {
            id
            username
            email
        }
    }
    """
    result = test_app.execute_query(mutation)
    assert "data" in result
```

## Running the App

```bash
# Install dependencies
pip install -r requirements.txt

# Run with default config
python main.py

# Run with custom config
export DATABASE_URL=postgresql://user:pass@db/mydb
export HOST=0.0.0.0
export PORT=8000
export ENVIRONMENT=production
python main.py
```

## Testing

```bash
# Run all tests
pytest

# Run specific test file
pytest tests/test_users.py

# Run with verbose output
pytest -v

# Run with coverage
pytest --cov=. tests/
```

## Key Features Demonstrated

✅ **Organized structure** - Types, queries, mutations, subscriptions in separate modules
✅ **Complex types** - Post with relationship to User
✅ **Input types** - CreateUserInput, UpdatePostInput, etc.
✅ **Error handling** - Validation in mutations
✅ **Subscriptions** - Real-time updates via WebSocket
✅ **Testing** - Isolated registry per test
✅ **Configuration** - Environment-based config
✅ **Schema verification** - Verify registration on startup

## Next Steps

- Add authentication and authorization
- Implement actual database queries
- Add logging and monitoring
- Deploy to production
- Add more complex types and relationships
- Implement caching

---

**This example shows production-ready patterns for FraiseQL applications.**
