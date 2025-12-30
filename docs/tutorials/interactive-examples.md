# Interactive Examples

Side-by-side examples showing how SQL database patterns translate to Python types and GraphQL operations.

## Basic User Query

### SQL: Database View

```sql
-- v_user view returns JSONB for GraphQL
CREATE VIEW v_user AS
SELECT
  id,
  jsonb_build_object(
      'id', u.id,
      'email', u.email,
      'name', u.name,
      'created_at', u.created_at
  ) as data
FROM tb_user u;
```

### Python: Type Definition

```python
import fraiseql
from uuid import UUID
from datetime import datetime

@type(sql_source="v_user")
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime
```

### GraphQL: Query Operation

```graphql
query GetUsers {
  users {
    id
    email
    name
    createdAt
  }
}

# Response:

{
  "data": {
    "users": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "email": "alice@example.com",
        "name": "Alice Johnson",
        "createdAt": "2024-01-15T10:30:00Z"
      }
    ]
  }
}
```

## Filtered Query with Arguments

### SQL: View with Filtering

```sql

-- Same v_user view, filtering happens in repository
-- Repository adds: WHERE data->>'email' LIKE '%@example.com'
```

### Python: Repository Method

```python
import fraiseql

@fraiseql.query
async def users(self, info, email_filter: str | None = None) -> list[User]:
    filters = {}
    if email_filter:
        filters['email__icontains'] = email_filter

    return await repo.find_rust("v_user", "users", info, **filters)
```

### GraphQL: Query with Arguments

```graphql
query GetFilteredUsers($emailFilter: String) {
  users(emailFilter: $emailFilter) {
    id
    email
    name
  }
}

# Variables:
{
  "emailFilter": "@example.com"
}

# Response:
{
  "data": {
    "users": [
      {
        "id": "550e8400-e29b-41d4-a716-446655440000",
        "email": "alice@example.com",
        "name": "Alice Johnson"
      }
    ]
  }
}
```

## Nested Object Query

### SQL: Joined View

```sql
-- v_post_with_author view with nested user data
CREATE VIEW v_post_with_author AS
SELECT jsonb_build_object(
    'id', p.id,
    'title', p.title,
    'content', p.content,
    'author', jsonb_build_object(
        'id', u.id,
        'name', u.name,
        'email', u.email
    ),
    'created_at', p.created_at
) as data
FROM tb_post p
JOIN tb_user u ON p.author_id = u.id;
```

### Python: Nested Types

```python
import fraiseql

@type(sql_source="v_post_with_author")
class Post:
    id: UUID
    title: str
    content: str
    author: User  # Nested User type
    created_at: datetime

# User type defined separately
@type(sql_source="v_user")
class User:
    id: UUID
    name: str
    email: str
```

### GraphQL: Nested Query

```graphql
query GetPostsWithAuthors {
  posts {
    id
    title
    content
    author {
      id
      name
      email
    }
    createdAt
  }
}

# Response:
{
  "data": {
    "posts": [
      {
        "id": "123e4567-e89b-12d3-a456-426614174000",
        "title": "My First Post",
        "content": "Hello world!",
        "author": {
          "id": "550e8400-e29b-41d4-a716-446655440000",
          "name": "Alice Johnson",
          "email": "alice@example.com"
        },
        "createdAt": "2024-01-15T11:00:00Z"
      }
    ]
  }
}
```

## Mutation: Create Operation

### SQL: Business Logic Function

```sql
-- fn_create_post handles validation and insertion
CREATE FUNCTION fn_create_post(
    p_title text,
    p_content text,
    p_author_id uuid
) RETURNS uuid AS $$
DECLARE
    v_post_id uuid;
BEGIN
    -- Validation
    IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = p_author_id) THEN
        RAISE EXCEPTION 'Author does not exist';
    END IF;

    -- Insert post
    INSERT INTO tb_post (title, content, author_id)
    VALUES (p_title, p_content, p_author_id)
    RETURNING id INTO v_post_id;

    -- Return new post ID
    RETURN v_post_id;
END;
$$ LANGUAGE plpgsql;
```

### Python: Mutation Resolver

```python
from fraiseql import mutation, input

@input
class CreatePostInput:
    title: str
    content: str
    author_id: UUID

@fraiseql.mutation
async def create_post(self, info, input: CreatePostInput) -> Post:
    # Call database function
    post_id = await db.execute_scalar(
        "SELECT fn_create_post($1, $2, $3)",
        [input.title, input.content, input.author_id]
    )

    # Return created post
    return await self.post(info, id=post_id)
```

### GraphQL: Mutation Operation

```graphql
mutation CreateNewPost($input: CreatePostInput!) {
  createPost(input: $input) {
    id
    title
    content
    author {
      name
      email
    }
    createdAt
  }
}

# Variables:
{
  "input": {
    "title": "New Blog Post",
    "content": "This is my new post content.",
    "authorId": "550e8400-e29b-41d4-a716-446655440000"
  }
}

# Response:
{
  "data": {
    "createPost": {
      "id": "123e4567-e89b-12d3-a456-426614174000",
      "title": "New Blog Post",
      "content": "This is my new post content.",
      "author": {
        "name": "Alice Johnson",
        "email": "alice@example.com"
      },
      "createdAt": "2024-01-15T12:00:00Z"
    }
  }
}
```

## Advanced: Aggregation Query

### SQL: Table View (Projection)

```sql
-- tv_post_stats provides denormalized table view for efficient analytics queries
CREATE TABLE tv_post_stats AS
SELECT
    p.id as post_id,
    p.title,
    COUNT(c.id) as comment_count,
    AVG(c.rating) as avg_rating,
    MAX(c.created_at) as last_comment_at
FROM tb_post p
LEFT JOIN tb_comment c ON p.id = c.post_id
GROUP BY p.id, p.title;

-- Refresh function for updated stats
CREATE FUNCTION fn_refresh_post_stats() RETURNS void AS $$
BEGIN
    TRUNCATE tv_post_stats;
    INSERT INTO tv_post_stats
    SELECT ...; -- Same query as above
END;
$$ LANGUAGE plpgsql;
```

### Python: Stats Type

```python
import fraiseql

@type(sql_source="tv_post_stats")
class PostStats:
    post_id: UUID
    title: str
    comment_count: int
    avg_rating: float | None
    last_comment_at: datetime | None
```

### GraphQL: Analytics Query

```graphql
query GetPostAnalytics {
  postStats {
    postId
    title
    commentCount
    avgRating
    lastCommentAt
  }
}

# Response:
{
  "data": {
    "postStats": [
      {
        "postId": "123e4567-e89b-12d3-a456-426614174000",
        "title": "My First Post",
        "commentCount": 5,
        "avgRating": 4.2,
        "lastCommentAt": "2024-01-16T09:30:00Z"
      }
    ]
  }
}
```

## Try It Yourself

### Setup Instructions

1. **Database**: Create tables and views as shown above
2. **Python**: Define types with `@type` decorators
3. **GraphQL**: Use the query/mutation examples
4. **Test**: Execute queries in GraphQL playground

### Common Patterns

- **Views (v_*)**: For real-time queries with joins
- **Functions (fn_*)**: For mutations with business logic
- **Table Views (tv_*)**: For denormalized data and aggregations
- **Nested Types**: Automatic resolution from JSONB

### Next Steps

- [Quickstart Guide](../getting-started/quickstart.md) - Get running in 5 minutes
- [Understanding FraiseQL](../guides/understanding-fraiseql.md) - Architecture deep dive
- [Database API](../core/database-api.md) - Repository patterns
- Examples (../../examples/) - Complete working applications
