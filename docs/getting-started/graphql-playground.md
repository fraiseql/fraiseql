# GraphQL Playground

The GraphQL Playground is your interactive development environment for exploring and testing your FraiseQL API. It provides schema introspection, query autocompletion, and real-time execution.

## Accessing the Playground

### Development Mode

In development, the playground is available at your GraphQL endpoint:

```python
from fraiseql.fastapi import create_fraiseql_app

app = create_fraiseql_app(
    database_url="postgresql://localhost/mydb",
    types=[User, Post],
    environment="development"  # Enables playground
)

# Access at: http://localhost:8000/graphql
```

### Production Mode

The playground is automatically disabled in production for security. To explicitly control it:

```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    database_url=DATABASE_URL,
    enable_playground=False,  # Explicit control
    enable_introspection=False,  # Disable schema introspection
    environment="production"
)

app = create_fraiseql_app(config=config, types=[...])
```

### Playground Options

FraiseQL supports two playground tools:

```python
config = FraiseQLConfig(
    playground_tool="graphiql"  # Default: GraphiQL with FraiseQL branding
    # playground_tool="apollo-sandbox"  # Alternative: Apollo Studio Sandbox
)
```

## Schema Introspection

The playground automatically introspects your schema to provide:

- **Type exploration**: Browse all types, queries, and mutations
- **Field documentation**: See descriptions and types for each field
- **Autocompletion**: Press `Ctrl+Space` for suggestions
- **Type checking**: Real-time validation of your queries

### Exploring the Schema

Click the "Docs" tab to browse your schema:

```graphql
# The schema explorer shows:
type Query {
  users(where: UserWhereInput): [User!]!
  posts(first: Int, after: String): PostConnection!
}

type User {
  id: ID!
  name: String!
  email: String
  posts: [Post!]!
}
```

## Writing Queries

### Basic Query

```graphql
query GetUsers {
  users {
    id
    name
    email
  }
}
```

### Query with Variables

Use the "Query Variables" panel for dynamic values:

```graphql
query GetUserById($userId: ID!) {
  user(id: $userId) {
    id
    name
    posts {
      title
      published
    }
  }
}
```

Variables (JSON):
```json
{
  "userId": "123e4567-e89b-12d3-a456-426614174000"
}
```

### Filtering and Pagination

```graphql
query GetActiveUsers {
  users(
    where: {
      status: ACTIVE
      createdAt: { gte: "2024-01-01" }
    }
    first: 10
    orderBy: { createdAt: DESC }
  ) {
    id
    name
    lastLogin
  }
}
```

## Testing Mutations

### Simple Mutation

```graphql
mutation CreateUser {
  createUser(input: {
    name: "Jane Doe"
    email: "jane@example.com"
  }) {
    id
    name
    email
  }
}
```

### Mutation with Error Handling

```graphql
mutation UpdatePost($postId: ID!, $title: String!) {
  updatePost(id: $postId, input: { title: $title }) {
    ... on Post {
      id
      title
      updatedAt
    }
    ... on ValidationError {
      message
      field
    }
  }
}
```

## Working with Subscriptions

Enable real-time updates with subscriptions:

```graphql
subscription OnPostCreated {
  postCreated {
    id
    title
    author {
      name
    }
    createdAt
  }
}
```

The playground automatically establishes a WebSocket connection for subscriptions.

## Authentication

### Adding Headers

Click the "HTTP Headers" panel to add authentication:

```json
{
  "Authorization": "Bearer eyJhbGciOiJIUzI1NiIs..."
}
```

### Testing Protected Queries

```graphql
query MyProfile {
  me {  # Requires authentication
    id
    name
    email
    role
  }
}
```

## Configuration Options

### Environment Variables

```bash
# Enable/disable playground
FRAISEQL_ENABLE_PLAYGROUND=true

# Choose playground tool
FRAISEQL_PLAYGROUND_TOOL=graphiql  # or apollo-sandbox

# Control introspection
FRAISEQL_ENABLE_INTROSPECTION=true
```

### Programmatic Configuration

```python
from fraiseql import FraiseQLConfig

config = FraiseQLConfig(
    enable_playground=True,
    playground_tool="graphiql",
    enable_introspection=True,
    # Custom playground settings
    playground_settings={
        "defaultQuery": "query { users { id name } }",
        "theme": "dark"
    }
)
```

## Tips and Tricks

### Keyboard Shortcuts

- **Ctrl+Space**: Autocomplete
- **Ctrl+Enter**: Execute query
- **Ctrl+Shift+P**: Prettify query
- **Ctrl+Shift+H**: Show history
- **Ctrl+/**: Comment/uncomment lines

### Query History

The playground saves your query history locally. Access previous queries using:
- History panel (clock icon)
- Keyboard shortcut: `Ctrl+Shift+H`

### Query Collections

Save frequently used queries:

```graphql
# Save this as "Get User Details"
query GetUserDetails($id: ID!) {
  user(id: $id) {
    id
    name
    email
    posts {
      id
      title
    }
    comments {
      id
      content
    }
  }
}
```

### Performance Testing

Use the playground's timing information to optimize queries:

```graphql
# Check execution time in response
{
  "data": { ... },
  "extensions": {
    "tracing": {
      "duration": 23000000,  # 23ms
      "execution": { ... }
    }
  }
}
```

### Multi-Operation Documents

Define multiple operations in one document:

```graphql
query GetUsers {
  users { id name }
}

query GetPosts {
  posts { id title }
}

mutation CreateUser($input: UserInput!) {
  createUser(input: $input) { id }
}
```

Select which operation to run from the dropdown.

## Production Considerations

### Disabling in Production

```python
# Automatic based on environment
app = create_fraiseql_app(
    environment="production"  # Disables playground
)

# Or explicit control
config = FraiseQLConfig(
    enable_playground=False,
    enable_introspection=False
)
```

### Security Headers

When playground is enabled in production (not recommended):

```python
from fraiseql.security import secure_playground

app = create_fraiseql_app(...)
app = secure_playground(app, allowed_origins=["https://admin.example.com"])
```

### Alternative Access Methods

For production environments, consider:
- **GraphQL clients**: Insomnia, Postman
- **Apollo Studio**: Cloud-based schema registry
- **Custom admin panel**: Restricted access playground

## Troubleshooting

### Playground Not Loading

1. Check environment configuration:
```python
print(config.enable_playground)  # Should be True
print(config.environment)  # Should not be "production"
```

2. Verify endpoint is accessible:
```bash
curl http://localhost:8000/graphql
```

3. Check browser console for errors

### Schema Not Updating

Clear playground cache:
- Hard refresh: `Ctrl+Shift+R`
- Clear localStorage: `localStorage.clear()`

### Authentication Issues

Verify headers are being sent:
```graphql
# Add this to your query to debug
{
  __typename
  me {  # Or your auth check query
    id
  }
}
```

## Next Steps

- [First API](first-api.md): Build a complete user management API
- [Type System](../core-concepts/type-system.md): Understanding FraiseQL's type system
- [Queries Guide](../guides/queries.md): Advanced query patterns