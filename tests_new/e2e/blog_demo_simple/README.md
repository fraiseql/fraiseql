# FraiseQL Blog Demo - Complete E2E Testing & Documentation

## 🌟 Overview

This directory contains a **self-contained blog application** that serves as both:
- **Comprehensive E2E testing** for FraiseQL features
- **Living documentation** showcasing real-world patterns
- **Onboarding demo** for new developers

The blog demo is completely standalone - it requires **no changes** to the FraiseQL core codebase and can be run independently.

## 🏗️ Architecture

```
blog_demo/
├── README.md                    # This documentation
├── conftest.py                  # Demo-specific fixtures
├── docker-compose.yml           # Standalone demo environment
├── app.py                       # Complete FraiseQL blog application
├── models.py                    # Blog domain models
├── schema.sql                   # Database schema
├── seed_data.sql               # Sample blog data
├── test_blog_complete.py        # Full workflow E2E test
├── test_blog_queries.py         # Query pattern tests
├── test_blog_mutations.py       # Mutation pattern tests
├── test_blog_auth.py           # Authentication flow tests
└── test_blog_performance.py    # Performance benchmark tests
```

## 🚀 Quick Start

### Run the Complete Demo

```bash
# Start the blog demo environment
cd tests_new/e2e/blog_demo
docker-compose up -d

# Run all blog demo tests
pytest . -v

# Run specific test categories
pytest test_blog_complete.py -v    # Full workflow
pytest test_blog_queries.py -v     # Query patterns
pytest test_blog_mutations.py -v   # Mutation patterns
```

### Explore the API

```bash
# The blog API runs on http://localhost:8080
curl http://localhost:8080/graphql

# GraphQL Playground available at:
# http://localhost:8080/graphql
```

## 📋 Features Demonstrated

### Core FraiseQL Patterns

- ✅ **Type System**: Complex types with relationships
- ✅ **Query Resolvers**: Filtering, pagination, nested loading
- ✅ **Mutations**: CRUD operations with validation
- ✅ **Database Integration**: PostgreSQL with JSONB
- ✅ **Authentication**: JWT-based auth with roles
- ✅ **Authorization**: Field-level permissions
- ✅ **Error Handling**: Comprehensive error patterns
- ✅ **Performance**: N+1 prevention, caching

### Blog Domain Features

- 👤 **User Management**: Registration, profiles, roles
- 📝 **Post Management**: Create, edit, publish, draft
- 💬 **Comment System**: Nested comments with moderation
- 🏷️ **Tagging System**: Hierarchical categories and tags
- 🔍 **Search**: Full-text search across content
- 📊 **Analytics**: View counts, popular content
- 🔒 **Moderation**: Content approval workflows

### GraphQL Operations

```graphql
# Query Examples
query GetPosts($first: Int!, $after: String) {
  posts(first: $first, after: $after, where: {status: {equals: PUBLISHED}}) {
    edges {
      node {
        id
        title
        slug
        excerpt
        publishedAt
        author {
          id
          username
          profile {
            avatarUrl
          }
        }
        tags {
          id
          name
          color
        }
        _commentCount
      }
      cursor
    }
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
  }
}

# Mutation Examples
mutation CreatePost($input: CreatePostInput!) {
  createPost(input: $input) {
    __typename
    ... on CreatePostSuccess {
      post {
        id
        title
        slug
        status
      }
      message
    }
    ... on CreatePostError {
      message
      code
      validationErrors {
        field
        message
      }
    }
  }
}

# Subscription Examples
subscription PostUpdates($userId: ID) {
  postUpdates(userId: $userId) {
    mutation
    node {
      id
      title
      status
      author {
        id
        username
      }
    }
  }
}
```

## 🧪 Test Categories

### 1. Complete Workflow Tests (`test_blog_complete.py`)

Tests complete user journeys:
- User registration → email verification → profile setup
- Author creates post → adds tags → publishes → receives comments
- Admin moderates content → manages users
- Analytics tracking and reporting

### 2. Query Pattern Tests (`test_blog_queries.py`)

Tests GraphQL query patterns:
- Complex filtering and sorting
- Relay-style pagination
- Nested data loading
- Performance optimization
- Search functionality

### 3. Mutation Pattern Tests (`test_blog_mutations.py`)

Tests mutation patterns:
- CRUD operations
- Validation error handling
- Business logic enforcement
- Optimistic updates
- Batch operations

### 4. Authentication Tests (`test_blog_auth.py`)

Tests authentication flows:
- User registration and login
- JWT token handling
- Role-based access control
- Permission enforcement
- Session management

### 5. Performance Tests (`test_blog_performance.py`)

Tests performance characteristics:
- Query execution time benchmarks
- N+1 query prevention
- Memory usage validation
- Connection pooling efficiency
- Cache hit rates

## 📊 Database Schema

The blog uses a realistic PostgreSQL schema with:

```sql
-- Core entities
users (id, username, email, password_hash, role, profile_data, created_at)
posts (id, title, slug, content, author_id, status, published_at, metadata)
comments (id, post_id, author_id, parent_id, content, status, created_at)
tags (id, name, slug, color, description, parent_id)
post_tags (post_id, tag_id)

-- Supporting tables
user_sessions (id, user_id, token_hash, expires_at)
post_views (id, post_id, user_id, viewed_at, ip_address)
notifications (id, user_id, type, data, read_at, created_at)
```

## 🎯 Learning Objectives

This demo teaches:

1. **FraiseQL Best Practices**
   - Proper type definitions
   - Efficient query patterns
   - Error handling strategies
   - Performance optimization

2. **GraphQL Patterns**
   - Schema design principles
   - Resolver implementation
   - Input validation
   - Response formatting

3. **Testing Strategies**
   - E2E test organization
   - Database testing patterns
   - Performance benchmarking
   - Error scenario coverage

4. **Production Readiness**
   - Authentication implementation
   - Security considerations
   - Monitoring and logging
   - Deployment patterns

## 🔍 Code Examples

### Model Definition

```python
@fraiseql.type(sql_source="users")
class User:
    id: str
    username: str
    email: str
    role: str
    created_at: datetime
    profile: Optional[UserProfile]

    @fraiseql.field
    async def posts(self, info: GraphQLResolveInfo) -> List[Post]:
        # Implementation with proper N+1 prevention
        pass

@fraiseql.type(sql_source="posts")
class Post:
    id: str
    title: str
    slug: str
    content: str
    status: str
    published_at: Optional[datetime]
    author: User
    tags: List[Tag]
    comments: List[Comment]
```

### Query Resolver

```python
@fraiseql.query
async def posts(
    info: GraphQLResolveInfo,
    first: int = 20,
    after: Optional[str] = None,
    where: Optional[PostWhereInput] = None
) -> PostConnection:
    # Implement Relay-style pagination with filtering
    pass
```

### Mutation Resolver

```python
@fraiseql.mutation
class CreatePost:
    input: CreatePostInput
    success: CreatePostSuccess
    error: CreatePostError

    async def resolve(self, info: GraphQLResolveInfo) -> Union[CreatePostSuccess, CreatePostError]:
        # Implement with proper validation and error handling
        pass
```

## 📈 Performance Benchmarks

The demo includes performance benchmarks for:

- **Query Performance**: < 50ms for most queries
- **Mutation Performance**: < 100ms for CRUD operations
- **Memory Usage**: < 100MB for 10k posts
- **Connection Pool**: Efficient connection reuse
- **Cache Hit Rate**: > 80% for repeated queries

## 🔐 Security Features

- **Authentication**: JWT-based with refresh tokens
- **Authorization**: Field-level permission checking
- **Input Validation**: Comprehensive input sanitization
- **SQL Injection Prevention**: Parameterized queries
- **Rate Limiting**: Request throttling
- **CORS Configuration**: Proper cross-origin setup

## 🚀 Deployment Ready

The demo includes production-ready configurations:

- **Docker Compose**: Complete environment setup
- **Environment Variables**: Configuration management
- **Health Checks**: Application health monitoring
- **Logging**: Structured logging with correlation IDs
- **Metrics**: Performance metrics collection
- **Error Tracking**: Error reporting integration

## 🤝 Contributing

This demo serves as a reference implementation. When adding new FraiseQL features:

1. Add corresponding demo implementation
2. Create comprehensive tests
3. Update documentation
4. Ensure performance benchmarks pass
5. Validate security considerations

---

**This blog demo showcases FraiseQL at its best - providing a complete, realistic example of building production-ready GraphQL APIs with PostgreSQL.**
