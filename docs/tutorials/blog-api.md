# Building a Blog API

In this tutorial, we'll build a complete blog API using FraiseQL. You'll learn how to set up the database, define GraphQL types, and implement queries and mutations.

## What We'll Build

A blog API with:
- **Users** - Authors and readers with profiles
- **Posts** - Blog articles with tags and publishing
- **Comments** - Threaded comments on posts
- **Authentication** - JWT-based auth with roles

## Prerequisites

- Python 3.13+
- PostgreSQL 14+
- Basic GraphQL knowledge

## Project Setup

### 1. Create Project Structure

```bash
mkdir blog-api
cd blog-api

# Create directory structure
mkdir -p {db/{migrations,functions,views},src,tests}
touch {src/__init__.py,tests/__init__.py}
```

### 2. Install Dependencies

```bash
# Create virtual environment
python -m venv .venv
source .venv/bin/activate  # On Windows: .venv\Scripts\activate

# Install FraiseQL
pip install fraiseql[fastapi,auth0]

# Development dependencies
pip install pytest pytest-asyncio httpx
```

### 3. Database Setup

Create the database:
```bash
createdb blog_api
```

## Database Schema

### 1. Create Tables

Create `db/migrations/001_tables.sql`:

```sql
-- Users table
CREATE TABLE tb_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    bio TEXT,
    avatar_url TEXT,
    is_active BOOLEAN DEFAULT true,
    roles TEXT[] DEFAULT ARRAY['user'],
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Posts table
CREATE TABLE tb_posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id UUID NOT NULL REFERENCES tb_users(id),
    title VARCHAR(500) NOT NULL,
    slug VARCHAR(500) UNIQUE NOT NULL,
    content TEXT NOT NULL,
    excerpt TEXT,
    tags TEXT[] DEFAULT ARRAY[]::TEXT[],
    is_published BOOLEAN DEFAULT false,
    published_at TIMESTAMP,
    view_count INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Comments table
CREATE TABLE tb_comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id UUID NOT NULL REFERENCES tb_posts(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES tb_users(id),
    parent_id UUID REFERENCES tb_comments(id),
    content TEXT NOT NULL,
    is_edited BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance
CREATE INDEX idx_posts_author_id ON tb_posts(author_id);
CREATE INDEX idx_posts_slug ON tb_posts(slug);
CREATE INDEX idx_posts_published ON tb_posts(is_published, published_at);
CREATE INDEX idx_comments_post_id ON tb_comments(post_id);
CREATE INDEX idx_comments_parent_id ON tb_comments(parent_id);
```

### 2. Create Views

Create `db/migrations/002_views.sql`:

```sql
-- Users view with snake_case JSON
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'bio', bio,
        'avatar_url', avatar_url,
        'is_active', is_active,
        'roles', roles,
        'created_at', created_at,
        'updated_at', updated_at
    ) AS data
FROM tb_users;

-- Posts view with snake_case JSON
CREATE VIEW v_posts AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'author_id', p.author_id,
        'title', p.title,
        'slug', p.slug,
        'content', p.content,
        'excerpt', p.excerpt,
        'tags', p.tags,
        'is_published', p.is_published,
        'published_at', p.published_at,
        'view_count', p.view_count,
        'created_at', p.created_at,
        'updated_at', p.updated_at
    ) AS data
FROM tb_posts p;

-- Comments view with snake_case JSON
CREATE VIEW v_comments AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id,
        'post_id', c.post_id,
        'author_id', c.author_id,
        'parent_id', c.parent_id,
        'content', c.content,
        'is_edited', c.is_edited,
        'created_at', c.created_at,
        'updated_at', c.updated_at
    ) AS data
FROM tb_comments c;
```

### 3. Apply Migrations

```bash
psql blog_api < db/migrations/001_tables.sql
psql blog_api < db/migrations/002_views.sql
```

## GraphQL Schema

### 1. Define Types

Create `src/models.py`:

```python
"""Blog API GraphQL types."""

from datetime import datetime
from uuid import UUID
from typing import Optional

import fraiseql
from fraiseql import fraise_field


@fraiseql.type
class User:
    """A user in the blog system."""

    id: UUID
    email: str = fraise_field(description="User's email address")
    name: str = fraise_field(description="User's display name")
    bio: Optional[str] = fraise_field(description="User biography")
    avatar_url: Optional[str] = fraise_field(description="Profile picture URL")
    is_active: bool = fraise_field(description="Whether user account is active")
    roles: list[str] = fraise_field(description="User roles")
    created_at: datetime
    updated_at: datetime


@fraiseql.type
class Post:
    """A blog post."""

    id: UUID
    author_id: UUID
    title: str = fraise_field(description="Post title")
    slug: str = fraise_field(description="URL-friendly identifier")
    content: str = fraise_field(description="Post content in Markdown")
    excerpt: Optional[str] = fraise_field(description="Short description")
    tags: list[str] = fraise_field(description="Post tags")
    is_published: bool = fraise_field(description="Publication status")
    published_at: Optional[datetime] = fraise_field(description="Publication date")
    view_count: int = fraise_field(description="Number of views")
    created_at: datetime
    updated_at: datetime


@fraiseql.type
class Comment:
    """A comment on a blog post."""

    id: UUID
    post_id: UUID
    author_id: UUID
    parent_id: Optional[UUID] = fraise_field(description="Parent comment for threading")
    content: str = fraise_field(description="Comment text")
    is_edited: bool = fraise_field(description="Whether comment was edited")
    created_at: datetime
    updated_at: datetime


# Input types for mutations

@fraiseql.input
class CreateUserInput:
    """Input for creating a new user."""

    email: str
    name: str
    password: str
    bio: Optional[str] = None
    avatar_url: Optional[str] = None


@fraiseql.input
class CreatePostInput:
    """Input for creating a new post."""

    title: str
    content: str
    excerpt: Optional[str] = None
    tags: Optional[list[str]] = None
    is_published: bool = False


@fraiseql.input
class UpdatePostInput:
    """Input for updating a post."""

    title: Optional[str] = None
    content: Optional[str] = None
    excerpt: Optional[str] = None
    tags: Optional[list[str]] = None
    is_published: Optional[bool] = None


@fraiseql.input
class CreateCommentInput:
    """Input for creating a comment."""

    post_id: UUID
    content: str
    parent_id: Optional[UUID] = None


@fraiseql.input
class PostFilters:
    """Filters for querying posts."""

    author_id: Optional[UUID] = None
    is_published: Optional[bool] = None
    tags_contain: Optional[list[str]] = None
    created_after: Optional[datetime] = None
    created_before: Optional[datetime] = None
    search: Optional[str] = None
```

### 2. Define Enums

Add to `src/models.py`:

```python
@fraiseql.enum
class PostOrderBy:
    """Post ordering options."""

    CREATED_AT_ASC = "created_at_asc"
    CREATED_AT_DESC = "created_at_desc"
    PUBLISHED_AT_ASC = "published_at_asc"
    PUBLISHED_AT_DESC = "published_at_desc"
    TITLE_ASC = "title_asc"
    TITLE_DESC = "title_desc"
    VIEW_COUNT_ASC = "view_count_asc"
    VIEW_COUNT_DESC = "view_count_desc"
```

### 3. Result Types

Add to `src/models.py`:

```python
# Result types for mutations

@fraiseql.result
class CreateUserResult:
    """Result of user creation."""


@fraiseql.success
class CreateUserSuccess:
    """Successful user creation."""

    user: User
    message: str = "User created successfully"


@fraiseql.failure
class CreateUserError:
    """Failed user creation."""

    message: str
    code: str
    field_errors: Optional[dict[str, str]] = None


@fraiseql.result
class CreatePostResult:
    """Result of post creation."""


@fraiseql.success
class CreatePostSuccess:
    """Successful post creation."""

    post: Post
    message: str = "Post created successfully"


@fraiseql.failure
class CreatePostError:
    """Failed post creation."""

    message: str
    code: str
```

## Repository Layer

Create `src/repository.py`:

```python
"""Database repository for blog operations."""

import asyncio
from datetime import datetime
from typing import Optional
from uuid import UUID

import asyncpg
from fraiseql.cqrs import CQRSRepository


class BlogRepository(CQRSRepository):
    """Repository for blog-specific operations."""

    def __init__(self, connection: asyncpg.Connection):
        super().__init__(connection)

    async def get_user_by_email(self, email: str) -> Optional[dict]:
        """Get user by email address."""
        query = "SELECT data FROM v_users WHERE data->>'email' = $1"
        result = await self.connection.fetchval(query, email)
        return result

    async def create_user(self, user_data: dict) -> dict:
        """Create a new user."""
        query = """
        INSERT INTO tb_users (email, name, bio, avatar_url)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        """
        user_id = await self.connection.fetchval(
            query,
            user_data["email"],
            user_data["name"],
            user_data.get("bio"),
            user_data.get("avatar_url")
        )

        # Return the created user
        return await self.get_by_id_raw("v_users", user_id)

    async def create_post(self, post_data: dict) -> dict:
        """Create a new post."""
        # Generate slug from title
        slug = self._generate_slug(post_data["title"])

        query = """
        INSERT INTO tb_posts (author_id, title, slug, content, excerpt, tags, is_published, published_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id
        """

        published_at = datetime.utcnow() if post_data.get("is_published") else None

        post_id = await self.connection.fetchval(
            query,
            post_data["author_id"],
            post_data["title"],
            slug,
            post_data["content"],
            post_data.get("excerpt"),
            post_data.get("tags", []),
            post_data.get("is_published", False),
            published_at
        )

        return await self.get_by_id_raw("v_posts", post_id)

    async def increment_view_count(self, post_id: UUID) -> None:
        """Increment the view count for a post."""
        await self.connection.execute(
            "UPDATE tb_posts SET view_count = view_count + 1 WHERE id = $1",
            post_id
        )

    def _generate_slug(self, title: str) -> str:
        """Generate URL-friendly slug from title."""
        import re
        slug = re.sub(r'[^\w\s-]', '', title.lower())
        slug = re.sub(r'[-\s]+', '-', slug)
        return slug.strip('-')
```

## Queries

Create `src/queries.py`:

```python
"""GraphQL query resolvers."""

from typing import Optional
from uuid import UUID

import fraiseql
from fraiseql.cqrs import CQRSRepository

from models import User, Post, Comment, PostFilters, PostOrderBy
from repository import BlogRepository


@fraiseql.type
class Query:
    """Root query type."""

    @fraiseql.field
    async def user(self, id: UUID, info: fraiseql.Info) -> Optional[User]:
        """Get a user by ID."""
        repo = BlogRepository(info.context["db"])
        user_data = await repo.get_by_id_raw("v_users", id)
        return User.from_dict(user_data) if user_data else None

    @fraiseql.field
    async def users(self, info: fraiseql.Info, limit: int = 20) -> list[User]:
        """Get all users."""
        repo = BlogRepository(info.context["db"])
        users_data = await repo.query("v_users", limit=limit)
        return [User.from_dict(data) for data in users_data]

    @fraiseql.field
    async def post(self, id: UUID, info: fraiseql.Info) -> Optional[Post]:
        """Get a post by ID."""
        repo = BlogRepository(info.context["db"])

        # Increment view count
        await repo.increment_view_count(id)

        post_data = await repo.get_by_id_raw("v_posts", id)
        return Post.from_dict(post_data) if post_data else None

    @fraiseql.field
    async def posts(
        self,
        info: fraiseql.Info,
        filters: Optional[PostFilters] = None,
        order_by: PostOrderBy = PostOrderBy.CREATED_AT_DESC,
        limit: int = 20,
        offset: int = 0
    ) -> list[Post]:
        """Get posts with filtering and pagination."""
        repo = BlogRepository(info.context["db"])

        # Convert filters
        filter_dict = {}
        if filters:
            if filters.is_published is not None:
                filter_dict["is_published"] = filters.is_published
            if filters.author_id:
                filter_dict["author_id"] = str(filters.author_id)

        posts_data = await repo.query(
            "v_posts",
            filters=filter_dict,
            order_by=order_by.value,
            limit=limit,
            offset=offset
        )

        return [Post.from_dict(data) for data in posts_data]

    @fraiseql.field
    async def comments(
        self,
        post_id: UUID,
        info: fraiseql.Info,
        limit: int = 50
    ) -> list[Comment]:
        """Get comments for a post."""
        repo = BlogRepository(info.context["db"])
        comments_data = await repo.query(
            "v_comments",
            filters={"post_id": str(post_id)},
            limit=limit
        )
        return [Comment.from_dict(data) for data in comments_data]
```

## Mutations

Create `src/mutations.py`:

```python
"""GraphQL mutation resolvers."""

from uuid import UUID

import fraiseql
from models import (
    CreateUserInput, CreateUserResult, CreateUserSuccess, CreateUserError,
    CreatePostInput, CreatePostResult, CreatePostSuccess, CreatePostError,
    CreateCommentInput, Comment
)
from repository import BlogRepository


@fraiseql.type
class Mutation:
    """Root mutation type."""

    @fraiseql.field
    async def create_user(
        self,
        input: CreateUserInput,
        info: fraiseql.Info
    ) -> CreateUserResult:
        """Create a new user."""
        repo = BlogRepository(info.context["db"])

        try:
            # Check if email exists
            existing = await repo.get_user_by_email(input.email)
            if existing:
                return CreateUserError(
                    message="Email already exists",
                    code="EMAIL_EXISTS",
                    field_errors={"email": "This email is already registered"}
                )

            # Create user
            user_data = await repo.create_user({
                "email": input.email,
                "name": input.name,
                "bio": input.bio,
                "avatar_url": input.avatar_url
            })

            from models import User
            user = User.from_dict(user_data)
            return CreateUserSuccess(user=user)

        except Exception as e:
            return CreateUserError(
                message=str(e),
                code="CREATION_FAILED"
            )

    @fraiseql.field
    async def create_post(
        self,
        input: CreatePostInput,
        info: fraiseql.Info
    ) -> CreatePostResult:
        """Create a new post."""
        repo = BlogRepository(info.context["db"])

        try:
            # Get current user from context
            user_context = info.context.get("user")
            if not user_context:
                return CreatePostError(
                    message="Authentication required",
                    code="UNAUTHENTICATED"
                )

            post_data = await repo.create_post({
                "author_id": UUID(user_context.user_id),
                "title": input.title,
                "content": input.content,
                "excerpt": input.excerpt,
                "tags": input.tags or [],
                "is_published": input.is_published
            })

            from models import Post
            post = Post.from_dict(post_data)
            return CreatePostSuccess(post=post)

        except Exception as e:
            return CreatePostError(
                message=str(e),
                code="CREATION_FAILED"
            )

    @fraiseql.field
    async def create_comment(
        self,
        input: CreateCommentInput,
        info: fraiseql.Info
    ) -> Comment:
        """Create a new comment."""
        repo = BlogRepository(info.context["db"])

        # Get current user
        user_context = info.context.get("user")
        if not user_context:
            raise Exception("Authentication required")

        query = """
        INSERT INTO tb_comments (post_id, author_id, parent_id, content)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        """

        comment_id = await repo.connection.fetchval(
            query,
            input.post_id,
            UUID(user_context.user_id),
            input.parent_id,
            input.content
        )

        comment_data = await repo.get_by_id_raw("v_comments", comment_id)
        return Comment.from_dict(comment_data)
```

## Application

Create `src/app.py`:

```python
"""FastAPI application with FraiseQL."""

import asyncpg
from contextlib import asynccontextmanager

import fraiseql
from models import *
from queries import Query
from mutations import Mutation


@asynccontextmanager
async def lifespan(app):
    """Manage database connection pool."""
    # Create connection pool
    app.state.db_pool = await asyncpg.create_pool(
        "postgresql://localhost/blog_api",
        min_size=5,
        max_size=20
    )
    yield
    # Close pool
    await app.state.db_pool.close()


app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/blog_api",
    types=[Query, Mutation],
    auto_camel_case=True,  # Enable automatic snake_case to camelCase conversion
    lifespan=lifespan
)


@app.middleware("http")
async def add_db_connection(request, call_next):
    """Add database connection to request context."""
    async with request.app.state.db_pool.acquire() as connection:
        request.state.db = connection
        response = await call_next(request)
    return response


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Testing

Create `tests/test_api.py`:

```python
"""Test the blog API."""

import pytest
from httpx import AsyncClient


@pytest.mark.asyncio
async def test_create_user():
    """Test user creation."""
    async with AsyncClient(app=app, base_url="http://test") as client:
        mutation = """
        mutation CreateUser($input: CreateUserInput!) {
            createUser(input: $input) {
                ... on CreateUserSuccess {
                    user {
                        id
                        email
                        name
                    }
                }
                ... on CreateUserError {
                    message
                    code
                }
            }
        }
        """

        variables = {
            "input": {
                "email": "test@example.com",
                "name": "Test User",
                "password": "password123"
            }
        }

        response = await client.post(
            "/graphql",
            json={"query": mutation, "variables": variables}
        )

        assert response.status_code == 200
        data = response.json()
        assert "errors" not in data
        assert data["data"]["createUser"]["user"]["email"] == "test@example.com"


@pytest.mark.asyncio
async def test_query_posts():
    """Test querying posts."""
    async with AsyncClient(app=app, base_url="http://test") as client:
        query = """
        query {
            posts {
                id
                title
                isPublished
                createdAt
            }
        }
        """

        response = await client.post("/graphql", json={"query": query})

        assert response.status_code == 200
        data = response.json()
        assert "errors" not in data
        assert isinstance(data["data"]["posts"], list)
```

## Run the API

```bash
# Start the development server
python src/app.py

# Visit GraphQL Playground
open http://localhost:8000/playground

# Or access the GraphQL endpoint directly
open http://localhost:8000/graphql
```

## Try Some Queries

### Create a User

```graphql
mutation {
  createUser(input: {
    email: "alice@example.com"
    name: "Alice Smith"
    bio: "Software developer and blogger"
  }) {
    ... on CreateUserSuccess {
      user {
        id
        email
        name
        bio
        createdAt
      }
    }
    ... on CreateUserError {
      message
      code
    }
  }
}
```

### Query Posts

```graphql
query {
  posts(filters: { isPublished: true }) {
    id
    title
    excerpt
    tags
    createdAt
    author {
      name
      email
    }
  }
}
```

## Key Takeaways

1. **Snake Case Everywhere**: Use snake_case in Python and SQL, GraphQL gets camelCase automatically
2. **View-Based Architecture**: Each type has a corresponding database view
3. **Efficient Queries**: One view query per resolver, no N+1 problems
4. **Type Safety**: Full type hints from database to GraphQL client
5. **Production Ready**: Separate development and production query modes

## Next Steps

- Add authentication with JWT tokens
- Implement user roles and permissions
- Add file upload for avatars
- Create REST endpoints alongside GraphQL
- Deploy to production

Congratulations! You've built a complete blog API with FraiseQL.
