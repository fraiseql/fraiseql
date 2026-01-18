# FraiseQL Examples

This directory contains example schemas and GraphQL queries demonstrating FraiseQL server usage.

## Quick Start

### 1. Basic Example

**Schema**: `basic_schema.json`

A simple schema with User and Post types for learning the fundamentals.

**Queries**: `queries/basic.graphql`

- Get all users
- Get user by ID
- Create new user
- Update user profile

**Run**:

```bash
# Compile schema
fraiseql-cli compile examples/basic_schema.json -o schema.compiled.json

# Start server with compiled schema
export FRAISEQL_SCHEMA_PATH=schema.compiled.json
export DATABASE_URL=postgresql://user:pass@localhost/db
cargo run -p fraiseql-server

# Execute query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d @examples/queries/basic.graphql
```

## Example Files

### basic_schema.json

Simple schema with:

- **User** type: id, name, email, createdAt
- **Post** type: id, title, content, author (User), createdAt
- **Query**: users, user(id), posts, post(id)
- **Mutation**: createUser, createPost, updateUser

**Use Cases**:

- Learning the basics
- Prototype applications
- Testing query validation
- API demonstration

### Blog Schema (future)

More complex schema with:

- Users, Posts, Comments, Tags
- Relationships and connections
- Pagination examples
- Filter and sort examples

### E-commerce Schema (future)

Real-world schema with:

- Products, Categories, Orders, Users
- Cart management
- Inventory tracking
- Full text search

## Query Examples

### Basic Queries

```graphql
# Get all users (paginated)
query GetUsers {
  users(first: 10) {
    edges {
      node {
        id
        name
        email
      }
      cursor
    }
    pageInfo {
      hasNextPage
      endCursor
    }
  }
}

# Get specific user with posts
query GetUserWithPosts($id: ID!) {
  user(id: $id) {
    id
    name
    email
    posts {
      id
      title
      createdAt
    }
  }
}
```

### Mutations

```graphql
# Create new user
mutation CreateUser($input: CreateUserInput!) {
  createUser(input: $input) {
    id
    name
    email
    createdAt
  }
}

# Variables:
{
  "input": {
    "name": "Jane Doe",
    "email": "jane@example.com"
  }
}
```

### Fragments

```graphql
query GetUserProfile($id: ID!) {
  user(id: $id) {
    ...userFields
    posts {
      ...postFields
    }
  }
}

fragment userFields on User {
  id
  name
  email
  createdAt
}

fragment postFields on Post {
  id
  title
  content
  createdAt
}
```

### Aliases & Multiple Queries

```graphql
query MultipleUsers {
  user1: user(id: "1") {
    ...userFields
  }
  user2: user(id: "2") {
    ...userFields
  }
  user3: user(id: "3") {
    ...userFields
  }
}

fragment userFields on User {
  id
  name
  email
}
```

## Error Examples

### Query Too Deep

**Error**: Nesting exceeds default limit of 10 levels

```graphql
query {
  user {
    posts {
      comments {
        author {
          posts {
            comments {
              author {
                posts {
                  comments {
                    author {
                      posts {
                        comments {
                          author {
                            name
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
```

**Response**:

```json
{
  "errors": [
    {
      "message": "Query exceeds maximum depth of 10: depth = 11",
      "code": "VALIDATION_ERROR"
    }
  ]
}
```

### Query Too Complex

**Error**: Query complexity score exceeds default limit of 100

```graphql
query {
  users [
    posts [
      comments [
        author [
          posts [
            comments [
              author
            ]
          ]
        ]
      ]
    ]
  ]
}
```

**Response**:

```json
{
  "errors": [
    {
      "message": "Query exceeds maximum complexity of 100: score = 157",
      "code": "VALIDATION_ERROR"
    }
  ]
}
```

### Field Not Found

```graphql
query {
  user(id: "999") {
    id
    name
    nonexistentField
  }
}
```

**Response**:

```json
{
  "errors": [
    {
      "message": "Field 'nonexistentField' not found on type User",
      "code": "PARSE_ERROR"
    }
  ]
}
```

## Testing Examples

### cURL

```bash
# Simple query
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d '{"query": "{ users { id name } }"}'

# Query with file
curl -X POST http://localhost:8000/graphql \
  -H "Content-Type: application/json" \
  -d @examples/queries/basic.graphql
```

### Node.js

```javascript
const query = `
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
      email
    }
  }
`;

fetch('http://localhost:8000/graphql', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    query,
    variables: { id: '123' }
  })
})
.then(r => r.json())
.then(data => console.log(data));
```

### Python

```python
import requests
import json

query = """
  query GetUser($id: ID!) {
    user(id: $id) {
      id
      name
      email
    }
  }
"""

response = requests.post('http://localhost:8000/graphql',
  headers={'Content-Type': 'application/json'},
  json={
    'query': query,
    'variables': {'id': '123'}
  }
)

print(json.dumps(response.json(), indent=2))
```

## Performance Testing

### Load Testing with Apache Bench

```bash
# Create query file
cat > /tmp/query.json << 'EOF'
{"query": "{ users { id name email } }"}
EOF

# Run load test (100 requests, 10 concurrent)
ab -n 100 -c 10 -T application/json -p /tmp/query.json \
  http://localhost:8000/graphql
```

### Load Testing with wrk

```bash
# Create Lua script
cat > request.lua << 'EOF'
wrk.method = "POST"
wrk.headers["Content-Type"] = "application/json"
wrk.body = '{"query": "{ users { id name } }"}'
EOF

# Run load test (12 threads, 400 connections, 60 seconds)
wrk -t 12 -c 400 -d 60s -s request.lua \
  http://localhost:8000/graphql
```

## Health Check Examples

```bash
# Check server health
curl http://localhost:8000/health | jq .

# Monitor connection pool
while true; do
  echo "=== $(date) ==="
  curl -s http://localhost:8000/health | jq .database.connection_pool
  sleep 5
done
```

## Learning Path

1. **Start with basic_schema.json**
   - Understand User and Post types
   - Try simple queries
   - Test variables

2. **Move to queries/basic.graphql**
   - Execute provided examples
   - Modify queries
   - Add your own fields

3. **Test error handling**
   - Exceed query depth limit
   - Exceed complexity limit
   - Request non-existent fields

4. **Test performance**
   - Run simple queries
   - Run complex queries
   - Use load testing tools

5. **Explore deployment**
   - Containerize with Docker
   - Deploy to Kubernetes
   - Set up monitoring

## Contributing Examples

To add new examples:

1. Create schema file: `examples/{name}_schema.json`
2. Create queries: `examples/queries/{name}.graphql`
3. Document in this README
4. Test with real server

## Next Steps

- See [HTTP_SERVER.md](../docs/HTTP_SERVER.md) for server configuration
- See [GRAPHQL_API.md](../docs/GRAPHQL_API.md) for API reference
- See [DEPLOYMENT.md](../docs/DEPLOYMENT.md) for deployment guide
