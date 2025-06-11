# Quick Start

This guide will help you create your first FraiseQL API in under 5 minutes.

## Step 1: Define Your Database Schema

First, let's create a simple database schema with a view that returns JSON:

```sql
-- Create a users table
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    data JSONB NOT NULL
);

-- Insert some sample data
INSERT INTO users (data) VALUES
    ('{"name": "Alice", "email": "alice@example.com"}'::jsonb),
    ('{"name": "Bob", "email": "bob@example.com"}'::jsonb);

-- Create a view that returns JSON
CREATE VIEW users_view AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', data->>'name',
        'email', data->>'email'
    ) as data
FROM users;
```

## Step 2: Define Your GraphQL Types

Create a file `schema.py`:

```python
import fraiseql
from fraiseql import fraise_field

@fraiseql.type
class User:
    """A user in the system"""
    id: int
    name: str = fraise_field(description="User's full name")
    email: str = fraise_field(description="User's email address")
```

## Step 3: Create Your API

Create a file `app.py`:

```python
import fraiseql
from schema import User

# Create the FastAPI app with GraphQL endpoint
app = fraiseql.create_fraiseql_app(
    database_url="postgresql://localhost/myapp",
    types=[User],
)

if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
```

## Step 4: Run Your API

```bash
python app.py
```

Your GraphQL API is now running at `http://localhost:8000/graphql`!

## Step 5: Explore Your API with GraphQL Playground

FraiseQL includes an interactive GraphQL Playground for exploring your API. Open your browser to:

- **GraphQL Playground**: `http://localhost:8000/playground` - Interactive query editor with schema exploration
- **Direct GraphQL endpoint**: `http://localhost:8000/graphql` - For programmatic access

In the GraphQL Playground, try this query:

```graphql
query {
  users {
    id
    name
    email
  }
}
```

You'll get:

```json
{
  "data": {
    "users": [
      {
        "id": 1,
        "name": "Alice",
        "email": "alice@example.com"
      },
      {
        "id": 2,
        "name": "Bob",
        "email": "bob@example.com"
      }
    ]
  }
}
```

## How It Works

1. **Database View**: The `users_view` returns each user as a JSON object
2. **Type Definition**: The `@fraiseql.type` decorator registers the User type
3. **Automatic Resolver**: FraiseQL creates a resolver that queries `users_view`
4. **Field Selection**: Only requested fields are extracted from the JSON

## What's Next?

- Learn about [relationships and nested objects](./first-api.md)
- Explore [Core Concepts](../core-concepts/index.md)
- Build a complete [Blog API Tutorial](../tutorials/blog-api.md)
