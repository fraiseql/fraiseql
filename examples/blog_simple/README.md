# FraiseQL Blog Simple - Complete Example Application

A complete blog application demonstrating FraiseQL's fundamental patterns and best practices.

## 🌟 Overview

This is a **production-ready blog application** that showcases:
- **Database-first architecture** with PostgreSQL functions
- **Command/Query separation** with views and materialized tables
- **CRUD operations** with comprehensive error handling
- **Real-time database testing** patterns
- **Authentication and authorization** flows

**Perfect for**: New FraiseQL projects, learning core patterns, simple content management systems.

## 🎯 Key Features

This example demonstrates FraiseQL's opinionated approach:

- **GraphQL API** exposes only `id` (UUID) and `identifier` (slug) fields
- **Python code** uses UUIDs exclusively for relationships
- **Database layer** uses views with JOINs to expose UUID relationships
- **Mutations** delegate to PostgreSQL functions for business logic

## 🚀 Quick Start

### 1. Setup Database

```bash
# Start PostgreSQL (using Docker)
docker run -d --name blog_simple_db \
  -e POSTGRES_DB=fraiseql_blog_simple \
  -e POSTGRES_USER=fraiseql \
  -e POSTGRES_PASSWORD=fraiseql \
  -p 5432:5432 \
  postgres:16

# Setup database schema
psql postgresql://fraiseql:fraiseql@localhost:5432/fraiseql_blog_simple -f db/setup.sql
```

### 2. Install Dependencies

```bash
pip install fraiseql[fastapi] psycopg[binary]
```

### 3. Run Application

```bash
python app.py
```

Visit http://localhost:8000/graphql for GraphQL Playground.

## 🏗️ Architecture

```
blog_simple/
├── README.md                    # This documentation
├── app.py                       # FastAPI + FraiseQL application
├── models.py                    # Blog domain models with GraphQL types
├── db/
│   ├── setup.sql               # Complete database schema
│   └── seed_data.sql           # Sample blog data
├── tests/
│   ├── conftest.py             # Test fixtures
│   ├── test_queries.py         # Query testing
│   ├── test_mutations.py       # Mutation testing
│   └── test_integration.py     # End-to-end testing
└── docker-compose.yml          # Development environment
```

## 📊 Database Schema

### Core Tables

```sql
-- Users and authentication
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'user',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    profile_data JSONB DEFAULT '{}'::jsonb
);

-- Blog posts
CREATE TABLE posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    content TEXT NOT NULL,
    excerpt TEXT,
    author_id UUID NOT NULL REFERENCES users(id),
    status TEXT NOT NULL DEFAULT 'draft',
    published_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    metadata JSONB DEFAULT '{}'::jsonb
);

-- Comments system
CREATE TABLE comments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users(id),
    parent_id UUID REFERENCES comments(id),
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Tagging system
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    slug TEXT NOT NULL UNIQUE,
    color TEXT DEFAULT '#6366f1',
    description TEXT
);

-- Post-tag relationships
CREATE TABLE post_tags (
    post_id UUID NOT NULL REFERENCES posts(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (post_id, tag_id)
);
```

### Query Views

```sql
-- Optimized post view with author and tag information
CREATE VIEW v_posts AS
SELECT
    p.id,
    p.title,
    p.slug,
    p.content,
    p.excerpt,
    p.status,
    p.published_at,
    p.created_at,
    p.metadata,
    -- Author information
    jsonb_build_object(
        'id', u.id,
        'username', u.username,
        'profile', u.profile_data
    ) as author,
    -- Tag array
    COALESCE(
        array_agg(
            jsonb_build_object(
                'id', t.id,
                'name', t.name,
                'slug', t.slug,
                'color', t.color
            )
        ) FILTER (WHERE t.id IS NOT NULL),
        '{}'
    ) as tags,
    -- Comment count
    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id AND c.status = 'published') as comment_count
FROM posts p
JOIN users u ON p.author_id = u.id
LEFT JOIN post_tags pt ON p.id = pt.post_id
LEFT JOIN tags t ON pt.tag_id = t.id
GROUP BY p.id, u.id, u.username, u.profile_data;
```

## 📝 GraphQL Schema

### Types

```python
@fraiseql.type(sql_source="users")
class User:
    id: str
    username: str
    email: str
    role: str
    created_at: datetime
    profile_data: dict

    @fraiseql.field
    async def posts(self, info: GraphQLResolveInfo) -> list[Post]:
        db = info.context["db"]
        return await db.find("v_posts", author_id=self.id)

@fraiseql.type(sql_source="v_posts")
class Post:
    id: str
    title: str
    slug: str
    content: str
    excerpt: str
    status: str
    published_at: datetime | None
    created_at: datetime
    author: User
    tags: list[Tag]
    comment_count: int

@fraiseql.type(sql_source="comments")
class Comment:
    id: str
    content: str
    created_at: datetime
    author: User
    parent_id: str | None

@fraiseql.type(sql_source="tags")
class Tag:
    id: str
    name: str
    slug: str
    color: str
    description: str | None
```

### Queries

```python
@fraiseql.query
async def posts(
    info: GraphQLResolveInfo,
    where: PostWhereInput | None = None,
    order_by: list[PostOrderByInput] | None = None,
    limit: int = 20,
    offset: int = 0
) -> list[Post]:
    """Query posts with filtering and pagination."""
    db = info.context["db"]
    return await db.find("v_posts", where=where, order_by=order_by, limit=limit, offset=offset)

@fraiseql.query
async def post(info: GraphQLResolveInfo, id: str | None = None, slug: str | None = None) -> Post | None:
    """Get single post by ID or slug."""
    db = info.context["db"]
    if id:
        return await db.find_one("v_posts", id=id)
    elif slug:
        return await db.find_one("v_posts", slug=slug)
    return None
```

### Mutations

```python
@fraiseql.input
class CreatePostInput:
    title: str
    content: str
    excerpt: str | None = None
    tag_ids: list[str] | None = None

@fraiseql.success
class CreatePostSuccess:
    post: Post
    message: str = "Post created successfully"

@fraiseql.failure
class CreatePostError:
    message: str
    code: str

@fraiseql.mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    failure: CreatePostError

    async def resolve(self, info: GraphQLResolveInfo) -> Union[CreatePostSuccess, CreatePostError]:
        db = info.context["db"]
        user_id = info.context["user_id"]

        try:
            # Create post
            post_data = {
                "title": self.input.title,
                "content": self.input.content,
                "excerpt": self.input.excerpt,
                "author_id": user_id,
                "slug": self.input.title.lower().replace(" ", "-")
            }

            post_id = await db.insert("posts", post_data, returning="id")

            # Add tags if provided
            if self.input.tag_ids:
                for tag_id in self.input.tag_ids:
                    await db.insert("post_tags", {"post_id": post_id, "tag_id": tag_id})

            # Fetch created post
            post = await db.find_one("v_posts", id=post_id)
            return CreatePostSuccess(post=post)

        except Exception as e:
            return CreatePostError(message=str(e), code="CREATE_FAILED")
```

## 🧪 Testing

### Test Structure

```python
# tests/conftest.py
import pytest
import psycopg
from fraiseql.cqrs import CQRSRepository

@pytest.fixture
async def db():
    """Database connection for testing."""
    conn = await psycopg.AsyncConnection.connect("postgresql://fraiseql:fraiseql@localhost/fraiseql_blog_simple_test")
    yield CQRSRepository(conn)
    await conn.close()

@pytest.fixture
async def sample_user(db):
    """Create sample user for testing."""
    user_data = {
        "username": "testuser",
        "email": "test@example.com",
        "password_hash": "hash",
        "role": "user"
    }
    user_id = await db.insert("users", user_data, returning="id")
    return await db.find_one("users", id=user_id)

# tests/test_queries.py
async def test_query_posts(db, sample_user):
    """Test basic post querying."""
    # Create sample post
    post_data = {
        "title": "Test Post",
        "content": "Test content",
        "author_id": sample_user["id"]
    }
    await db.insert("posts", post_data)

    # Query posts
    posts = await db.find("v_posts", limit=10)
    assert len(posts) == 1
    assert posts[0]["title"] == "Test Post"

# tests/test_mutations.py
async def test_create_post_mutation(graphql_client, sample_user):
    """Test post creation via GraphQL."""
    mutation = """
        mutation CreatePost($input: CreatePostInput!) {
            createPost(input: $input) {
                __typename
                ... on CreatePostSuccess {
                    post {
                        id
                        title
                        slug
                    }
                }
                ... on CreatePostError {
                    message
                }
            }
        }
    """

    result = await graphql_client.execute(
        mutation,
        variables={"input": {"title": "New Post", "content": "Content here"}}
    )

    assert result["data"]["createPost"]["__typename"] == "CreatePostSuccess"
```

### Running Tests

```bash
# Setup test database
createdb fraiseql_blog_simple_test
psql fraiseql_blog_simple_test -f db/setup.sql

# Run all tests
pytest tests/ -v

# Run with coverage
pytest tests/ --cov=. --cov-report=html
```

## 🔧 Configuration

### Environment Variables

```bash
# Database
DB_NAME=fraiseql_blog_simple
DB_USER=fraiseql
DB_PASSWORD=fraiseql
DB_HOST=localhost
DB_PORT=5432

# Application
ENV=development
LOG_LEVEL=info
```

### Docker Development

```yaml
# docker-compose.yml
version: '3.8'
services:
  db:
    image: postgres:16
    environment:
      POSTGRES_DB: fraiseql_blog_simple
      POSTGRES_USER: fraiseql
      POSTGRES_PASSWORD: fraiseql
    ports:
      - "5432:5432"
    volumes:
      - ./db:/docker-entrypoint-initdb.d

  app:
    build: .
    ports:
      - "8000:8000"
    depends_on:
      - db
    environment:
      DB_HOST: db
      DB_NAME: fraiseql_blog_simple
      DB_USER: fraiseql
      DB_PASSWORD: fraiseql
```

## 📚 Key Learning Points

This example demonstrates:

1. **FraiseQL Fundamentals**
   - Type definitions with SQL sources
   - Query and mutation resolvers
   - Database integration patterns
   - Error handling strategies

2. **Database Design**
   - PostgreSQL schema with relationships
   - Query-optimized views
   - JSONB for flexible data
   - Proper indexing strategies

3. **Testing Patterns**
   - Fixture-based database testing
   - GraphQL integration tests
   - Isolated test environments
   - Mock data strategies

4. **Production Readiness**
   - Environment configuration
   - Error handling and logging
   - Security considerations
   - Performance optimization

## 🚀 Next Steps

After mastering this simple example:

1. Explore **blog_enterprise** for advanced patterns
2. Study **authentication** and **authorization**
3. Learn **performance optimization** techniques
4. Implement **real-time subscriptions**
5. Add **caching** and **monitoring**

---

**This simple blog demonstrates FraiseQL's power for building clean, testable GraphQL APIs with PostgreSQL.**
