# GraphQL Development Tools

FraiseQL includes interactive GraphQL development tools that provide a powerful interface for exploring and testing your API during development. You can choose between GraphiQL 2.0 (default) or Apollo Sandbox.

## Available Tools

### GraphiQL 2.0 (Default)

GraphiQL is the reference implementation of a GraphQL IDE, offering:

- **Interactive query editor** with syntax highlighting and auto-completion
- **Schema explorer** to browse all available types, queries, and mutations
- **Query history** to revisit previous queries
- **Real-time error highlighting** and helpful error messages
- **Documentation browser** with descriptions from your schema
- **Keyboard shortcuts** for productivity

### Apollo Sandbox

Apollo Sandbox is Apollo's cloud-powered GraphQL IDE that provides:

- **Modern interface** with enhanced UI/UX
- **Schema visualization** with interactive graphs
- **Advanced query building** with field search
- **Response mocking** for testing edge cases
- **Operation collections** for organizing queries
- **Cloud sync** for accessing queries across devices

## Accessing the Playground

When running FraiseQL in development mode, the GraphQL Playground is available at:

```
http://localhost:8000/playground
```

The playground automatically connects to your GraphQL endpoint at `/graphql`.

## Features

### Query Editor

The left panel provides a full-featured editor where you can:

- Write GraphQL queries with auto-completion
- Use keyboard shortcuts (Cmd/Ctrl + Enter to execute)
- Format queries automatically
- Access query variables in the bottom panel

### Schema Documentation

Click the "DOCS" tab on the right to explore your schema:

- Browse all available queries and mutations
- View field descriptions and types
- Navigate through nested object relationships
- See required vs optional fields

### Query Variables

Use the "QUERY VARIABLES" panel at the bottom to pass variables:

```json
{
  "userId": "123",
  "includeDeleted": false
}
```

Then reference them in your query:

```graphql
query GetUser($userId: ID!, $includeDeleted: Boolean = false) {
  user(id: $userId, includeDeleted: $includeDeleted) {
    id
    name
    email
  }
}
```

### HTTP Headers

Add custom headers for authentication or other purposes:

```json
{
  "Authorization": "Bearer your-token-here"
}
```

## Configuration

### Choosing Your Development Tool

You can select which GraphQL IDE to use through the `playground_tool` configuration:

#### Via Configuration Object

```python
from fraiseql.fastapi import create_fraiseql_app, FraiseQLConfig

# Use GraphiQL (default)
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    config=FraiseQLConfig(
        enable_playground=True,
        playground_tool="graphiql",  # Options: "graphiql" or "apollo-sandbox"
        enable_introspection=True
    )
)

# Use Apollo Sandbox
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    config=FraiseQLConfig(
        enable_playground=True,
        playground_tool="apollo-sandbox",
        enable_introspection=True
    )
)
```

#### Via Environment Variables

```bash
# Enable playground with GraphiQL (default)
export FRAISEQL_ENABLE_PLAYGROUND=true
export FRAISEQL_PLAYGROUND_TOOL=graphiql
export FRAISEQL_ENABLE_INTROSPECTION=true

# Enable playground with Apollo Sandbox
export FRAISEQL_ENABLE_PLAYGROUND=true
export FRAISEQL_PLAYGROUND_TOOL=apollo-sandbox
export FRAISEQL_ENABLE_INTROSPECTION=true

# Disable playground (automatic in production)
export FRAISEQL_ENVIRONMENT=production
```

#### Via App Parameters

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=False  # Enables playground (default)
)
```

### Production Mode

In production mode, both the playground and introspection are **automatically disabled** for security. To override this (not recommended):

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    production=True,
    config=FraiseQLConfig(
        enable_playground=True,  # Force enable in production
        enable_introspection=True
    )
)
```

## Security Considerations

### Development Authentication

If you've enabled development authentication, the playground will require the same credentials:

```python
app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    dev_auth_username="admin",
    dev_auth_password="secret123"
)
```

Access the playground with HTTP Basic Auth:
- Username: `admin`
- Password: `secret123`

### Production Security

For production environments:

1. **Disable playground**: Set `production=True` or `FRAISEQL_ENVIRONMENT=production`
2. **Use proper authentication**: Implement Auth0 or custom authentication
3. **Restrict introspection**: Prevent schema exploration in production
4. **Use HTTPS**: Always serve your API over HTTPS in production

## Tips and Tricks

### Useful Keyboard Shortcuts

- **Cmd/Ctrl + Enter**: Execute query
- **Cmd/Ctrl + Space**: Trigger auto-completion
- **Shift + Cmd/Ctrl + P**: Prettify query
- **Shift + Cmd/Ctrl + M**: Merge fragments

### Query Fragments

Reuse common field selections with fragments:

```graphql
fragment UserFields on User {
  id
  name
  email
}

query GetUsers {
  users {
    ...UserFields
    posts {
      id
      title
    }
  }
}
```

### Introspection Queries

Explore your schema programmatically:

```graphql
# Get all types
{
  __schema {
    types {
      name
      description
    }
  }
}

# Get fields for a specific type
{
  __type(name: "User") {
    fields {
      name
      type {
        name
      }
      description
    }
  }
}
```

## Troubleshooting

### Playground Not Loading

1. **Check the URL**: Ensure you're accessing `/playground`, not `/graphql`
2. **Verify it's enabled**: Check `enable_playground` configuration
3. **Check authentication**: If dev auth is enabled, provide credentials
4. **Check console errors**: Open browser developer tools for error messages

### Can't See Schema Documentation

1. **Enable introspection**: Set `enable_introspection=True`
2. **Check permissions**: Ensure your auth token has appropriate access
3. **Refresh the page**: Sometimes the schema needs to be reloaded

### Queries Not Working

1. **Check the endpoint**: Playground should point to `/graphql`
2. **Verify database connection**: Ensure your database is accessible
3. **Check error messages**: The playground shows detailed GraphQL errors
4. **Validate JSON**: Ensure query variables are valid JSON

## Next Steps

- Learn about [Type Definitions](../core-concepts/type-system.md)
- Explore [Query Translation](../core-concepts/query-translation.md)
- Build complex queries with [Database Views](../core-concepts/database-views.md)
