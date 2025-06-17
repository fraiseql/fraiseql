# FraiseQL Quick Start Guide

Get up and running with FraiseQL in 5 minutes!

## Installation

```bash
pip install fraiseql
```

## Basic Usage

### 1. Import FraiseQL

```python
import fraiseql
from fraiseql import fraise_field
```

### 2. Define Your Types

Use the `@fraiseql.type` decorator to define GraphQL types:

```python
@fraiseql.type
class User:
    id: int
    name: str = fraise_field(description="User's full name")
    email: str = fraise_field(description="User's email address")
    created_at: datetime
```

### 3. Create Queries

Use the `@fraiseql.query` decorator to define query functions:

```python
@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:
    """Get a user by ID"""
    # Your database logic here
    return User(id=id, name="John Doe", email="john@example.com", created_at=datetime.now())

@fraiseql.query
async def list_users(info, limit: int = 10) -> List[User]:
    """List all users"""
    # Your database logic here
    return [User(...), User(...)]
```

### 4. Create Mutations

Define input types and mutations:

```python
@fraiseql.input
class CreateUserInput:
    name: str
    email: str

@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    """Create a new user"""
    # Your database logic here
    return User(
        id=123,
        name=input.name,
        email=input.email,
        created_at=datetime.now()
    )
```

### 5. Create the App

**IMPORTANT**: Use `create_fraiseql_app()`, NOT `build_schema()`:

```python
app = fraiseql.create_fraiseql_app(
    # Optional: PostgreSQL connection
    database_url="postgresql://user:pass@localhost/dbname",
    
    # Register your types
    types=[User],  # Add all your @fraiseql.type decorated classes
    
    # App configuration
    title="My GraphQL API",
    production=False  # Enables GraphQL Playground
)
```

### 6. Run the Server

```python
if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Accessing Your API

- **GraphQL Endpoint**: `http://localhost:8000/graphql`
- **GraphQL Playground**: `http://localhost:8000/playground` (in development mode)

## Complete Example

```python
import fraiseql
from fraiseql import fraise_field
from datetime import datetime
from typing import List, Optional

# Define types
@fraiseql.type
class Post:
    id: int
    title: str = fraise_field(description="Post title")
    content: str = fraise_field(description="Post content")
    author_id: int
    created_at: datetime

# Define queries
@fraiseql.query
async def posts(info, limit: int = 20) -> List[Post]:
    """Get recent posts"""
    # Your database query here
    return []

@fraiseql.query
async def post(info, id: int) -> Optional[Post]:
    """Get a post by ID"""
    # Your database query here
    return None

# Define mutations
@fraiseql.input
class CreatePostInput:
    title: str
    content: str
    author_id: int

@fraiseql.mutation
async def create_post(info, input: CreatePostInput) -> Post:
    """Create a new post"""
    # Your database insert here
    return Post(
        id=1,
        title=input.title,
        content=input.content,
        author_id=input.author_id,
        created_at=datetime.now()
    )

# Create and run the app
if __name__ == "__main__":
    import uvicorn
    
    app = fraiseql.create_fraiseql_app(
        types=[Post],
        title="Blog API",
        production=False
    )
    
    print("GraphQL Playground: http://localhost:8000/playground")
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Common Patterns

### Database Integration

```python
from fraiseql.repository import FraiseQLRepository

# In your queries/mutations
@fraiseql.query
async def get_user(info, id: int) -> Optional[User]:
    db: FraiseQLRepository = info.context["db"]
    user_data = await db.fetch_one(
        "SELECT * FROM users WHERE id = %s",
        (id,)
    )
    return User(**user_data) if user_data else None
```

### Authentication

```python
from fraiseql.auth import requires_auth

@fraiseql.query
@requires_auth
async def my_profile(info) -> User:
    user = info.context["user"]
    # Return current user's profile
```

### Using with FastAPI

```python
from fastapi import FastAPI
import fraiseql

# Create your FastAPI app
fastapi_app = FastAPI()

# Create FraiseQL app
graphql_app = fraiseql.create_fraiseql_app(
    types=[User, Post],
    app=fastapi_app  # Pass existing FastAPI app
)

# Now your FastAPI app has GraphQL endpoints!
```

## Troubleshooting

### "AttributeError: module 'fraiseql' has no attribute 'build_schema'"

You're using an incorrect API. Use `fraiseql.create_fraiseql_app()` instead.

### "Type Query must define one or more fields"

Make sure you've:
1. Decorated your query functions with `@fraiseql.query`
2. Imported the modules containing your queries before creating the app

### GraphQL Playground not showing

Make sure `production=False` when creating the app:

```python
app = fraiseql.create_fraiseql_app(
    types=[...],
    production=False  # This enables Playground
)
```

## Next Steps

- Check out the [examples](../examples/) directory for more complex scenarios
- Read about [DataLoader integration](./DATALOADER.md) for N+1 query prevention
- Learn about [WebSocket subscriptions](./SUBSCRIPTIONS.md) for real-time updates