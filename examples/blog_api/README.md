# Blog API Example - Enterprise Patterns Showcase

🟢 BEGINNER | ⏱️ 15 min | 🎯 Content Management | 🏷️ Enterprise Patterns

This example demonstrates how to build a complete GraphQL API using FraiseQL with enterprise-grade patterns for production systems.

**What you'll learn:**
- CQRS architecture with PostgreSQL functions and views
- Direct database queries for relationship resolution (no DataLoader needed)
- Enterprise mutation patterns (success/error/noop)
- Audit trails and change tracking
- Authentication and role-based access
- Production-ready error handling

**Prerequisites:** None (great starting point!)

**Next steps:**
- `../ecommerce/` - Add complex business logic
- `../enterprise_patterns/` - Master all enterprise patterns
- `../apq_multi_tenant/` - Add performance optimization

## Patterns Demonstrated

### ✅ Mutation Result Pattern
- Standardized success/error responses with metadata
- Field-level change tracking
- Comprehensive audit information
- See: `mutations.py` enterprise classes and `test_mutation_results.py`

### ✅ NOOP Handling Pattern
- Idempotent operations with graceful edge case handling
- Multiple NOOP scenarios (duplicate slugs, no changes detected)
- See: Enterprise mutation classes with `noop` result types

### ✅ App/Core Function Split
- Clean separation of input handling and business logic
- Type-safe core functions with JSONB app wrappers
- See: `db/functions/app_functions.sql` and `core_functions.sql`

### ✅ Unified Audit Logging
- **Single `audit_events` table** with CDC + cryptographic chain
- PostgreSQL-native crypto (no Python overhead)
- Tamper-proof audit trails with SHA-256 hashing
- See: `core.log_and_return_mutation()` function and unified audit table

### ❌ Advanced Features
For complete enterprise patterns (identifier management, multi-layer validation), see `../enterprise_patterns/`

## Features

- User management with comprehensive audit trails
- Blog posts with enterprise-grade change tracking
- Comments system with nested replies
- Role-based permissions (user, admin)
- Production-ready with optimized queries
- CQRS architecture with enterprise patterns
- App/core function split for clean architecture

## Setup

### 1. Database Setup

The blog API uses a CQRS architecture with:
- Write-side tables prefixed with `tb_`
- SQL functions for all mutations
- Read-side views prefixed with `v_` containing JSONB data

#### Option A: Automated Setup

```bash
cd db/
./setup.sh
```

#### Option B: Manual Setup

```bash
# Create database
createdb blog_db

# Run migrations in order
psql -d blog_db -f db/migrations/001_initial_schema.sql
psql -d blog_db -f db/migrations/002_functions.sql
psql -d blog_db -f db/migrations/003_views.sql
```

The migrations will create:
- Write-side tables: `tb_users`, `tb_posts`, `tb_comments`
- SQL functions: `fn_create_user`, `fn_create_post`, etc.
- Read-side views: `v_users`, `v_posts`, `v_comments` with JSONB data

### 2. Environment Variables

Create a `.env` file:

```bash
# Database
DATABASE_URL=postgresql://user:password@localhost/blog_db

# Auth0 (optional)
AUTH0_DOMAIN=your-domain.auth0.com
AUTH0_API_IDENTIFIER=https://api.yourdomain.com

# Environment
ENV=development  # or production
```

### 3. Install Dependencies

```bash
pip install fraiseql uvicorn python-dotenv
```

### 4. Run the Application

```bash
python app.py
```

The API will be available at:
- GraphQL endpoint: `http://localhost:8000/graphql`
- GraphQL Playground: `http://localhost:8000/playground` (development only)

## Example Queries

### Create a User

```graphql
mutation CreateUser {
  createUser(input: {
    email: "john@example.com"
    name: "John Doe"
    password: "secure123"
    bio: "Software developer"
  }) {
    ... on CreateUserSuccess {
      user {
        id
        email
        name
        createdAt
      }
      message
    }
    ... on CreateUserError {
      message
      code
      fieldErrors
    }
  }
}
```

### Create a Post (requires authentication)

```graphql
mutation CreatePost {
  createPost(input: {
    title: "Getting Started with FraiseQL"
    content: "FraiseQL makes it easy to build GraphQL APIs..."
    excerpt: "Learn how to use FraiseQL"
    tags: ["tutorial", "graphql", "fraiseql"]
    isPublished: true
  }) {
    ... on CreatePostSuccess {
      post {
        id
        title
        slug
        publishedAt
        tags
      }
    }
    ... on CreatePostError {
      message
      code
    }
  }
}
```

### Query Posts

```graphql
query GetPosts {
  posts(
    filters: {
      isPublished: true
      tagsContain: ["tutorial"]
    }
    orderBy: CREATED_AT_DESC
    limit: 10
  ) {
    id
    title
    excerpt
    author {
      name
      avatarUrl
    }
    tags
    viewCount
    createdAt
  }
}
```

### Add a Comment

```graphql
mutation AddComment {
  createComment(input: {
    postId: "post-uuid-here"
    content: "Great article!"
  }) {
    id
    content
    author {
      name
    }
    createdAt
  }
}
```

## Authentication

### Using Auth0

1. Set up an Auth0 application
2. Configure the environment variables
3. Include the JWT token in requests:

```bash
curl -X POST http://localhost:8000/graphql \
  -H "Authorization: Bearer YOUR_JWT_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "{ me { id email name } }"}'
```

### Using Custom Authentication

You can implement a custom auth provider:

```python
from fraiseql.auth import AuthProvider, UserContext

class CustomAuthProvider(AuthProvider):
    async def validate_token(self, token: str) -> dict:
        # Your token validation logic
        pass

    async def get_user_from_token(self, token: str) -> UserContext:
        # Your user lookup logic
        pass

# Use it in the app
app = create_fraiseql_app(
    auth=CustomAuthProvider(),
    # ... other config
)
```

## Production Deployment

### Optimizations

1. **Enable query compilation** for frequently used queries:

```python
app = create_fraiseql_app(
    production=True,
    compiled_queries_path="./compiled_queries.json"
)
```

2. **Use environment-specific settings**:

```python
# Production disables introspection and playground automatically
production=os.getenv("ENV") == "production"
```

3. **Add monitoring and caching**:

```python
from fraiseql.fastapi import FraiseQLConfig

config = FraiseQLConfig(
    enable_query_caching=True,
    cache_ttl=300,  # 5 minutes
    max_query_depth=10,  # Prevent deep queries
)

app = create_fraiseql_app(config=config, ...)
```

### Docker Deployment

```dockerfile
FROM python:3.13-slim

WORKDIR /app

COPY requirements.txt .
RUN pip install -r requirements.txt

COPY . .

ENV ENV=production

CMD ["uvicorn", "app:app", "--host", "0.0.0.0", "--port", "8000"]
```

## Advanced Features

### Custom Scalar Types

```python
from fraiseql.types.scalars import create_scalar_type

# Create a custom scalar for URLs
URLScalar = create_scalar_type(
    "URL",
    serialize=str,
    parse_value=lambda v: validate_url(v),
    parse_literal=lambda ast: validate_url(ast.value)
)
```

### Subscriptions (Coming Soon)

```python
@fraiseql.subscription
async def comment_added(info, post_id: UUID):
    # Real-time comment updates
    pass
```

### Federation Support (Coming Soon)

```python
@fraiseql.type(key="id")
class User:
    id: UUID
    # ... fields
```
