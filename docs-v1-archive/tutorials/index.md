# FraiseQL Tutorials

Learn FraiseQL through practical, hands-on tutorials that demonstrate real-world usage patterns and best practices.

## Available Tutorials

### [Building a Blog API](./blog-api.md)
**Level:** Intermediate | **Time:** 45 minutes

Build a complete blog API with posts, comments, and user management. Learn:

- CQRS architecture with PostgreSQL
- Composed views to eliminate N+1 queries
- Type-safe GraphQL with modern Python
- Authentication and authorization
- Performance optimization techniques

**Prerequisites:**

- Basic PostgreSQL knowledge
- Python 3.13+ experience
- Understanding of GraphQL concepts

---

## Learning Paths

### Path 1: Quick Start (Beginner)
**Goal:** Get a working API in 15 minutes

1. [GraphQL Playground](../getting-started/graphql-playground.md) - Explore the interactive environment
2. [First API](../getting-started/first-api.md) - Build your first FraiseQL API
3. [Core Concepts](../core-concepts/index.md) - Understand the philosophy

### Path 2: Full Application (Intermediate)
**Goal:** Build a production-ready application

1. [Architecture](../core-concepts/architecture.md) - Understand CQRS and system design
2. [Database Views](../core-concepts/database-views.md) - Master the view layer
3. [Blog API Tutorial](./blog-api.md) - Complete application example
4. [Mutations](../mutations/index.md) - Handle write operations

### Path 3: Migration (Advanced)
**Goal:** Migrate existing GraphQL APIs to FraiseQL

1. [Type System](../core-concepts/type-system.md) - Map existing types
2. [Query Translation](../core-concepts/query-translation.md) - Understand query optimization
3. [Migration Guide](../migration/index.md) - Step-by-step migration from existing GraphQL frameworks
4. [Performance Tuning](./blog-api.md#performance-optimization) - Optimize for production

## Common Queries

### Basic Query Examples

Here are common GraphQL query patterns used throughout the tutorials:

```graphql
# Simple query with fields
query GetUser {
  user(id: "123") {
    id
    name
    email
  }
}

# Query with pagination
query GetPosts {
  posts(limit: 10, offset: 0) {
    id
    title
    author {
      name
    }
  }
}

# Query with filtering
query SearchPosts {
  posts(where: { status: { eq: "published" } }) {
    id
    title
    publishedAt
  }
}

# Query with nested relationships
query GetPostWithComments {
  post(id: "456") {
    id
    title
    content
    author {
      id
      name
    }
    comments {
      id
      content
      createdAt
      author {
        name
      }
    }
  }
}
```

## Tutorial Prerequisites

### System Requirements

- PostgreSQL 14 or higher
- Python 3.13 or higher
- Basic terminal/command line knowledge

### Recommended Knowledge

- **SQL Basics**: SELECT, INSERT, UPDATE, DELETE
- **Python**: Classes, async/await, type hints
- **GraphQL**: Queries, mutations, schema basics
- **Web APIs**: HTTP, JSON, REST concepts

### Development Environment Setup

```bash
# Install PostgreSQL (macOS)
brew install postgresql@14
brew services start postgresql@14

# Install PostgreSQL (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install postgresql-14

# Create a database for tutorials
createdb fraiseql_tutorial

# Install FraiseQL
pip install fraiseql

# Verify installation
python -c "import fraiseql; print(fraiseql.__version__)"
```

## Example Code Repository

All tutorial code is available in the FraiseQL repository:

```bash
# Clone the repository
git clone https://github.com/fraiseql/fraiseql.git
cd fraiseql/examples

# Tutorial examples
examples/
├── blog_api/          # Complete blog API
├── simple_api/        # Minimal example
├── auth_example/      # Authentication patterns
└── advanced/          # Advanced patterns
```

## Common Patterns

### Pattern 1: Basic CRUD Operations

Every FraiseQL application follows this structure:

```python
# 1. Define your types
@fraiseql.type
class Item:
    id: UUID
    name: str
    created_at: datetime

# 2. Create database views
CREATE VIEW v_items AS
SELECT id, jsonb_build_object(
    '__typename', 'Item',
    'id', id,
    'name', name,
    'createdAt', created_at
) AS data FROM tb_items;

# 3. Implement queries
@fraiseql.query
async def get_items(info) -> list[Item]:
    db = info.context["db"]
    return await db.query("SELECT * FROM v_items")

# 4. Handle mutations via functions
CREATE FUNCTION fn_create_item(input JSON)
RETURNS JSON AS $$ ... $$;
```

### Pattern 2: Eliminating N+1 Queries

Use composed views to fetch related data in one query:

```sql
-- Instead of separate queries for posts and authors
CREATE VIEW v_post_with_author AS
SELECT
    p.id,
    jsonb_build_object(
        '__typename', 'Post',
        'id', p.id,
        'title', p.title,
        'author', (
            SELECT jsonb_build_object(
                '__typename', 'User',
                'id', u.id,
                'name', u.name
            )
            FROM tb_users u
            WHERE u.id = p.author_id
        )
    ) AS data
FROM tb_posts p;
```

### Pattern 3: Authentication Integration

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_profile(info) -> User:
    user_id = info.context["user"].id
    db = info.context["db"]
    return await db.get_user_by_id(user_id)
```

## Testing Your APIs

### Unit Testing

```python
import pytest
from your_app import app

@pytest.mark.asyncio
async def test_query():
    query = """
        query {
            getItems {
                id
                name
            }
        }
    """
    response = await app.execute_query(query)
    assert response["data"]["getItems"] is not None
```

### Integration Testing

```python
from testcontainers.postgres import PostgresContainer

@pytest.fixture
async def database():
    with PostgresContainer("postgres:14") as postgres:
        # Run migrations
        await run_migrations(postgres.get_connection_url())
        yield postgres.get_connection_url()
```

## Performance Considerations

### View Optimization

- Create indexes on filter columns
- Use materialized views for expensive aggregations
- Compose views to reduce query count

### Query Analysis
Enable query analysis in development:

```python
app = create_fraiseql_app(
    analyze_queries=True,
    slow_query_threshold=100,  # milliseconds
)
```

### Monitoring
Track performance metrics:

```python
# Prometheus metrics
from prometheus_client import Histogram

query_duration = Histogram(
    'graphql_query_duration_seconds',
    'GraphQL query duration'
)

@app.middleware("http")
async def track_performance(request, call_next):
    with query_duration.time():
        return await call_next(request)
```

## Troubleshooting

### Common Issues

1. **"View not found" error**

   - Ensure view names follow `v_` prefix convention
   - Check that views have a JSONB `data` column
   - Verify camelCase field names in JSONB

2. **Type mismatch errors**

   - Use proper Python type hints (3.10+ syntax)
   - Map PostgreSQL types correctly (UUID → UUID, not str)
   - Check nullable fields match (`| None`)

3. **N+1 query detection**

   - Enable query analysis to identify issues
   - Create composed views for related data
   - Use DataLoader for remaining cases

### Getting Help

- **Documentation**: Full reference at [docs.fraiseql.com](https://docs.fraiseql.com)
- **Examples**: Working code in `/examples` directory
- **Community**: Discord server for questions
- **Issues**: GitHub issues for bugs

## Next Steps

After completing the tutorials:

1. **Explore Advanced Topics**

   - [Subscriptions](../advanced/subscriptions.md) for real-time updates
   - [DataLoader Integration](../advanced/dataloader.md) for batching
   - [Performance Monitoring](../advanced/monitoring.md)

2. **Build Your Application**

   - Start with the [blog API](./blog-api.md) as a template
   - Customize types and views for your domain
   - Add authentication and authorization

3. **Deploy to Production**

   - Review [deployment guide](../deployment/index.md)
   - Configure monitoring and logging
   - Set up database backups and migrations

## Contributing Tutorials

We welcome tutorial contributions! If you've built something interesting with FraiseQL:

1. Fork the repository
2. Add your tutorial to `/docs/tutorials/`
3. Include working code in `/examples/`
4. Submit a pull request

Tutorial guidelines:

- Include complete, runnable code
- Explain the "why" not just the "how"
- Add performance considerations
- Include testing examples

Happy building with FraiseQL! 🚀
