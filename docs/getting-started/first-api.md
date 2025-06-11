# Building Your First Complete API

In this guide, we'll build a complete blog API with posts, authors, and comments. You'll learn how FraiseQL handles relationships through view composition.

## Database Schema

Let's start with a more complete schema:

```sql
-- Authors table
CREATE TABLE authors (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Posts table
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    author_id INTEGER REFERENCES authors(id),
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Comments table
CREATE TABLE comments (
    id SERIAL PRIMARY KEY,
    post_id INTEGER REFERENCES posts(id),
    author_id INTEGER REFERENCES authors(id),
    data JSONB NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Sample data
INSERT INTO authors (data) VALUES
    ('{"name": "Jane Doe", "email": "jane@example.com", "bio": "Tech writer"}'::jsonb),
    ('{"name": "John Smith", "email": "john@example.com", "bio": "Developer"}'::jsonb);

INSERT INTO posts (author_id, data) VALUES
    (1, '{"title": "Getting Started with FraiseQL", "content": "FraiseQL makes GraphQL APIs simple...", "published": true}'::jsonb),
    (1, '{"title": "Advanced FraiseQL Patterns", "content": "Let us explore advanced patterns...", "published": true}'::jsonb),
    (2, '{"title": "My FraiseQL Journey", "content": "How I learned to love FraiseQL...", "published": false}'::jsonb);

INSERT INTO comments (post_id, author_id, data) VALUES
    (1, 2, '{"content": "Great article!"}'::jsonb),
    (1, 1, '{"content": "Thanks! Glad you enjoyed it."}'::jsonb);
```

## Creating Composed Views

The key to FraiseQL's efficiency is creating views that compose data from other views:

```sql
-- Base author view
CREATE VIEW authors_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'email', data->>'email',
        'bio', data->>'bio',
        'createdAt', created_at
    ) as data
FROM authors;

-- Comments view with author info
CREATE VIEW comments_view AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id,
        'content', c.data->>'content',
        'createdAt', c.created_at,
        'author', a.data
    ) as data
FROM comments c
JOIN authors_view a ON a.id = c.author_id;

-- Posts view with author and comments
CREATE VIEW posts_view AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.data->>'title',
        'content', p.data->>'content',
        'published', (p.data->>'published')::boolean,
        'createdAt', p.created_at,
        'author', a.data,
        'comments', COALESCE(
            (SELECT jsonb_agg(c.data ORDER BY c.id)
             FROM comments_view c
             WHERE c.data->>'post_id' = p.id::text),
            '[]'::jsonb
        )
    ) as data
FROM posts p
JOIN authors_view a ON a.id = p.author_id;
```

Notice how:
- Each view returns a complete JSON object
- `posts_view` composes data from `authors_view` and `comments_view`
- No N+1 queries - everything is handled at the database level

## GraphQL Schema

Now let's define our GraphQL types:

```python
# schema.py
import fraiseql
from fraiseql import fraise_field
from datetime import datetime
from typing import Optional

@fraiseql.type
class Author:
    """Blog post author"""
    id: int
    name: str = fraise_field(description="Author's full name")
    email: str = fraise_field(description="Author's email")
    bio: Optional[str] = fraise_field(description="Author biography")
    created_at: datetime = fraise_field(description="Account creation date")

@fraiseql.type
class Comment:
    """Comment on a blog post"""
    id: int
    content: str = fraise_field(description="Comment text")
    author: Author = fraise_field(description="Comment author")
    created_at: datetime = fraise_field(description="Comment timestamp")

@fraiseql.type
class Post:
    """Blog post"""
    id: int
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    published: bool = fraise_field(description="Publication status")
    author: Author = fraise_field(description="Post author")
    comments: list[Comment] = fraise_field(description="Post comments")
    created_at: datetime = fraise_field(description="Publication date")
```

## Adding Queries

Let's add some queries to fetch our data:

```python
# queries.py
import fraiseql
from fraiseql import CQRSRepository
from typing import Optional
from schema import Post, Author

@fraiseql.type
class Query:
    @fraiseql.field
    async def posts(
        self,
        published: Optional[bool] = None,
        info: fraiseql.Info = None
    ) -> list[Post]:
        """Get all posts, optionally filtered by publication status"""
        repo = CQRSRepository(info.context["db"])

        # FraiseQL automatically generates the WHERE clause
        filters = {}
        if published is not None:
            filters["published"] = published

        return await repo.get_many(Post, where=filters)

    @fraiseql.field
    async def post(self, id: int, info: fraiseql.Info = None) -> Optional[Post]:
        """Get a single post by ID"""
        repo = CQRSRepository(info.context["db"])
        return await repo.get_by_id(Post, id)

    @fraiseql.field
    async def authors(self, info: fraiseql.Info = None) -> list[Author]:
        """Get all authors"""
        repo = CQRSRepository(info.context["db"])
        return await repo.get_many(Author)
```

## Adding Mutations

Let's add mutations to create and update content:

```python
# mutations.py
import fraiseql
from fraiseql import fraise_field, result, success, failure
from schema import Post, Author, Comment

@fraiseql.input
class CreatePostInput:
    title: str
    content: str
    published: bool = False

@result
class CreatePostResult:
    """Result of creating a post"""

@success
class CreatePostSuccess:
    post: Post = fraise_field(description="The created post")

@failure
class CreatePostError:
    message: str = fraise_field(description="Error message")

@fraiseql.type
class Mutation:
    @fraiseql.mutation
    async def create_post(
        self,
        author_id: int,
        input: CreatePostInput,
        info: fraiseql.Info = None
    ) -> CreatePostResult:
        """Create a new blog post"""
        # This would call a PostgreSQL function
        # For now, showing the pattern
        try:
            # Your database logic here
            post = Post(
                id=1,
                title=input.title,
                content=input.content,
                published=input.published,
                # ... other fields
            )
            return CreatePostSuccess(post=post)
        except Exception as e:
            return CreatePostError(message=str(e))
```

## Complete Application

Put it all together:

```python
# app.py
import fraiseql
from schema import Post, Author, Comment
from queries import Query
from mutations import Mutation

app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/blog",
    types=[Post, Author, Comment, Query, Mutation],
    # Development mode with full introspection
    production=False,
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Example Queries

Now you can run complex queries efficiently:

```graphql
query GetPosts {
  posts(published: true) {
    id
    title
    author {
      name
      email
    }
    comments {
      content
      author {
        name
      }
    }
  }
}
```

FraiseQL will:
1. Query the `posts_view`
2. Extract only the requested fields from the JSON
3. Return the nested data structure

All with a single database query!

## Key Takeaways

- **One View Per Entity**: Each entity type has one corresponding view
- **View Composition**: Complex relationships are handled by composing views
- **No N+1**: All data fetching happens in the database layer
- **Field Selection**: FraiseQL extracts only requested fields from JSON

## Next Steps

- Learn about [authentication](../advanced/authentication.md)
- Explore [custom scalars](../advanced/custom-scalars.md)
- Read about [performance optimization](../advanced/performance.md)
