# Migration Guide: From Hasura/PostGraphile to FraiseQL

This guide helps you migrate existing GraphQL APIs from Hasura or PostGraphile to FraiseQL, leveraging FraiseQL's unique advantages while maintaining compatibility.

## Why Migrate to FraiseQL?

### Advantages over Hasura
- **No separate runtime**: FraiseQL generates pure Python code, no Haskell runtime required
- **Type safety**: Full Python type hints and compile-time checking
- **Lower resource usage**: ~10x less memory, faster startup times
- **Simpler deployment**: Standard Python application, works anywhere Python runs
- **Better debugging**: Native Python debugging tools, no black box

### Advantages over PostGraphile
- **Modern Python ecosystem**: Use FastAPI, async/await, modern Python features
- **Flexible schema**: Not tied to database schema, customize as needed
- **Better performance**: Optimized query generation, connection pooling
- **Easier customization**: Pure Python code, no plugin system needed

## Migration Overview

### From Hasura

```
Hasura Architecture:              FraiseQL Architecture:
┌─────────────────┐              ┌─────────────────┐
│  GraphQL Client │              │  GraphQL Client │
└────────┬────────┘              └────────┬────────┘
         │                                │
┌────────▼────────┐              ┌────────▼────────┐
│  Hasura Engine  │              │  Python FastAPI │
│   (Haskell)     │              │   + FraiseQL    │
└────────┬────────┘              └────────┬────────┘
         │                                │
┌────────▼────────┐              ┌────────▼────────┐
│   PostgreSQL    │              │   PostgreSQL    │
└─────────────────┘              └─────────────────┘
```

### From PostGraphile

```
PostGraphile Architecture:        FraiseQL Architecture:
┌─────────────────┐              ┌─────────────────┐
│  GraphQL Client │              │  GraphQL Client │
└────────┬────────┘              └────────┬────────┘
         │                                │
┌────────▼────────┐              ┌────────▼────────┐
│  Node.js Server │              │  Python FastAPI │
│ + PostGraphile  │              │   + FraiseQL    │
└────────┬────────┘              └────────┬────────┘
         │                                │
┌────────▼────────┐              ┌────────▼────────┐
│   PostgreSQL    │              │   PostgreSQL    │
└─────────────────┘              └─────────────────┘
```

## Step-by-Step Migration

### 1. Analyze Your Current Schema

#### Export from Hasura
```bash
# Export Hasura metadata
hasura metadata export

# The schema will be in metadata/tables.yaml
```

#### Export from PostGraphile
```graphql
# Run introspection query
{
  __schema {
    types {
      name
      fields {
        name
        type {
          name
        }
      }
    }
  }
}
```

### 2. Create FraiseQL Models

#### From Hasura Table

Hasura metadata:
```yaml
- table:
    schema: public
    name: users
  object_relationships:
    - name: orders
      using:
        foreign_key_constraint_on:
          column: user_id
          table:
            schema: public
            name: orders
```

FraiseQL equivalent:
```python
# models.py
from typing import Optional
from uuid import UUID
from datetime import datetime
import fraiseql

@fraiseql.type
class User:
    id: UUID
    email: str
    name: str
    created_at: datetime

    @fraiseql.field
    async def orders(self, info: fraiseql.Info) -> list["Order"]:
        # FraiseQL will handle the SQL generation
        return []

@fraiseql.type
class Order:
    id: UUID
    user_id: UUID
    total: fraiseql.Decimal
    created_at: datetime
```

#### From PostGraphile Schema

PostGraphile auto-generated type:
```graphql
type User {
  nodeId: ID!
  id: Int!
  email: String!
  name: String!
  createdAt: Datetime!
  ordersByUserId: OrdersConnection!
}
```

FraiseQL equivalent:
```python
@fraiseql.type
class User:
    id: int  # or UUID if using uuid-ossp
    email: str
    name: str
    created_at: datetime

    @fraiseql.field
    async def orders(
        self,
        info: fraiseql.Info,
        first: Optional[int] = None,
        after: Optional[str] = None
    ) -> "OrderConnection":
        # Implement pagination
        return OrderConnection(...)
```

### 3. Migrate Permissions

#### Hasura Permissions

Hasura permission:
```yaml
- role: user
  permission:
    columns:
      - id
      - email
      - name
    filter:
      id:
        _eq: X-Hasura-User-Id
```

FraiseQL equivalent:
```python
@fraiseql.type
class User:
    id: UUID
    email: str
    name: str

    @classmethod
    def can_read(cls, info: fraiseql.Info, obj: "User") -> bool:
        # Check if user can read this object
        current_user_id = info.context.get("user_id")
        return str(obj.id) == current_user_id or info.context.get("is_admin")
```

#### PostGraphile RLS

PostGraphile RLS policy:
```sql
CREATE POLICY users_select ON users
  FOR SELECT
  USING (id = current_setting('jwt.claims.user_id')::uuid);
```

FraiseQL handles this at the application level:
```python
@fraiseql.field
async def users(self, info: fraiseql.Info) -> list[User]:
    current_user_id = info.context.get("user_id")

    # Apply filtering based on permissions
    if info.context.get("is_admin"):
        return []  # Return all users
    else:
        # Return only current user
        return []  # Filtered by current_user_id
```

### 4. Migrate Mutations

#### Hasura Actions

Hasura action:
```yaml
- name: registerUser
  definition:
    kind: synchronous
    handler: https://api.example.com/register
  request_transform:
    template_engine: Kriti
    method: POST
    content_type: application/json
    body: |
      {
        "email": {{$body.input.email}},
        "password": {{$body.input.password}}
      }
```

FraiseQL mutation:
```python
@fraiseql.type
class RegisterInput:
    email: str
    password: str
    name: str

@fraiseql.type
class RegisterResult:
    success: bool
    user: Optional[User]
    token: Optional[str]
    message: str

@fraiseql.mutation
async def register(info: fraiseql.Info, input: RegisterInput) -> RegisterResult:
    # Direct implementation, no webhooks needed
    try:
        # Hash password
        password_hash = hash_password(input.password)

        # Create user in database
        user_id = await create_user(
            email=input.email,
            password_hash=password_hash,
            name=input.name
        )

        # Generate token
        token = generate_jwt(user_id)

        return RegisterResult(
            success=True,
            user=User(id=user_id, email=input.email, name=input.name),
            token=token,
            message="Registration successful"
        )
    except Exception as e:
        return RegisterResult(
            success=False,
            user=None,
            token=None,
            message=str(e)
        )
```

#### PostGraphile Custom Mutations

PostGraphile mutation:
```javascript
// PostGraphile plugin
module.exports = makeExtendSchemaPlugin({
  typeDefs: gql`
    input RegisterInput {
      email: String!
      password: String!
    }

    type RegisterPayload {
      user: User
      token: String
    }

    extend type Mutation {
      register(input: RegisterInput!): RegisterPayload
    }
  `,
  resolvers: {
    Mutation: {
      register: async (_, { input }, { pgClient }) => {
        // Implementation
      }
    }
  }
});
```

FraiseQL equivalent (same as above).

### 5. Migrate Subscriptions

#### From Hasura

Hasura subscription:
```graphql
subscription OnOrderUpdate($userId: uuid!) {
  orders(
    where: { user_id: { _eq: $userId } }
    order_by: { created_at: desc }
    limit: 10
  ) {
    id
    status
    total
  }
}
```

FraiseQL subscription:
```python
@fraiseql.subscription
async def order_updates(
    info: fraiseql.Info,
    user_id: UUID
) -> AsyncIterator[Order]:
    # Use PostgreSQL LISTEN/NOTIFY
    async with info.context.db.listen("order_updates") as listener:
        async for notification in listener:
            data = json.loads(notification.payload)
            if data["user_id"] == str(user_id):
                order = await get_order(data["order_id"])
                yield order
```

### 6. Migrate Database Functions

#### Hasura Computed Fields

Hasura computed field:
```sql
CREATE FUNCTION user_full_name(user_row users)
RETURNS TEXT AS $$
  SELECT user_row.first_name || ' ' || user_row.last_name
$$ LANGUAGE sql STABLE;
```

FraiseQL computed field:
```python
@fraiseql.type
class User:
    first_name: str
    last_name: str

    @fraiseql.field
    @property
    def full_name(self) -> str:
        return f"{self.first_name} {self.last_name}"
```

#### PostGraphile Smart Comments

PostGraphile smart comment:
```sql
COMMENT ON FUNCTION user_full_name(users) IS
  E'@fieldName fullName\n@computedField';
```

In FraiseQL, just use Python:
```python
@fraiseql.field
def full_name(self) -> str:
    """The user's full name."""
    return f"{self.first_name} {self.last_name}"
```

## Migration Patterns

### 1. Connection/Pagination Pattern

#### Hasura/PostGraphile Connection
```graphql
type UserConnection {
  edges: [UserEdge!]!
  pageInfo: PageInfo!
  totalCount: Int!
}
```

#### FraiseQL Implementation
```python
@fraiseql.type
class PageInfo:
    has_next_page: bool
    has_previous_page: bool
    start_cursor: Optional[str]
    end_cursor: Optional[str]

@fraiseql.type
class UserEdge:
    node: User
    cursor: str

@fraiseql.type
class UserConnection:
    edges: list[UserEdge]
    page_info: PageInfo
    total_count: int

@fraiseql.field
async def users(
    self,
    info: fraiseql.Info,
    first: Optional[int] = None,
    after: Optional[str] = None,
    last: Optional[int] = None,
    before: Optional[str] = None
) -> UserConnection:
    # Implement relay-style pagination
    pass
```

### 2. Filter Pattern

#### Hasura Where Clause
```graphql
query {
  users(where: {
    _and: [
      { age: { _gte: 18 } },
      { email: { _ilike: "%@example.com" } }
    ]
  }) {
    id
    email
  }
}
```

#### FraiseQL Implementation
```python
@fraiseql.type
class UserFilter:
    age_gte: Optional[int]
    age_lte: Optional[int]
    email_contains: Optional[str]
    email_ilike: Optional[str]

@fraiseql.field
async def users(
    self,
    info: fraiseql.Info,
    where: Optional[UserFilter] = None
) -> list[User]:
    # Build SQL query based on filters
    conditions = []

    if where:
        if where.age_gte:
            conditions.append(f"age >= {where.age_gte}")
        if where.email_ilike:
            conditions.append(f"email ILIKE {where.email_ilike}")

    # Execute query
    return await fetch_users(conditions)
```

### 3. Nested Mutations Pattern

#### Hasura Nested Insert
```graphql
mutation {
  insert_users_one(object: {
    email: "user@example.com"
    name: "John Doe"
    addresses: {
      data: [
        { street: "123 Main St", city: "NYC" }
      ]
    }
  }) {
    id
    addresses {
      id
    }
  }
}
```

#### FraiseQL Implementation
```python
@fraiseql.type
class CreateAddressInput:
    street: str
    city: str

@fraiseql.type
class CreateUserInput:
    email: str
    name: str
    addresses: Optional[list[CreateAddressInput]]

@fraiseql.mutation
async def create_user(
    info: fraiseql.Info,
    input: CreateUserInput
) -> User:
    # Create user
    user_id = await insert_user(
        email=input.email,
        name=input.name
    )

    # Create addresses if provided
    if input.addresses:
        for address in input.addresses:
            await insert_address(
                user_id=user_id,
                street=address.street,
                city=address.city
            )

    return await get_user(user_id)
```

## Testing Migration

### 1. Schema Compatibility Test

```python
# test_schema_migration.py
import pytest
from graphql import graphql_sync

def test_hasura_compatible_query():
    """Test that Hasura queries work with FraiseQL."""
    query = """
        query {
            users(where: { email: { _eq: "test@example.com" } }) {
                id
                email
                orders_aggregate {
                    aggregate {
                        count
                    }
                }
            }
        }
    """

    # FraiseQL handles the query
    result = graphql_sync(schema, query)
    assert not result.errors
```

### 2. Performance Comparison

```python
# benchmark_migration.py
import time
import asyncio

async def benchmark_query():
    # Hasura query time
    hasura_start = time.time()
    await fetch_from_hasura(query)
    hasura_time = time.time() - hasura_start

    # FraiseQL query time
    fraiseql_start = time.time()
    await fetch_from_fraiseql(query)
    fraiseql_time = time.time() - fraiseql_start

    print(f"Hasura: {hasura_time:.3f}s")
    print(f"FraiseQL: {fraiseql_time:.3f}s")
    print(f"Improvement: {hasura_time / fraiseql_time:.1f}x")
```

## Deployment Migration

### From Hasura

Replace Hasura Docker:
```yaml
# Before (docker-compose.yml)
services:
  hasura:
    image: hasura/graphql-engine:v2.x
    environment:
      HASURA_GRAPHQL_DATABASE_URL: postgres://...
    ports:
      - "8080:8080"
```

With FraiseQL:
```yaml
# After (docker-compose.yml)
services:
  api:
    build: .
    environment:
      DATABASE_URL: postgres://...
    ports:
      - "8080:8000"
```

### From PostGraphile

Replace PostGraphile PM2:
```javascript
// Before (ecosystem.config.js)
module.exports = {
  apps: [{
    name: 'postgraphile',
    script: 'postgraphile',
    args: '-c postgres://... --watch --enhance-graphiql'
  }]
}
```

With FraiseQL systemd:
```ini
# After (fraiseql.service)
[Unit]
Description=FraiseQL API
After=network.target

[Service]
Type=notify
ExecStart=/usr/bin/python -m uvicorn app:app
Environment="DATABASE_URL=postgres://..."
Restart=always

[Install]
WantedBy=multi-user.target
```

## Common Gotchas

### 1. Hasura Metadata
- FraiseQL doesn't use metadata YAML files
- All configuration is in Python code
- Use environment variables for configuration

### 2. PostGraphile Plugins
- No plugin system in FraiseQL
- Extend functionality with Python decorators
- Use middleware for cross-cutting concerns

### 3. Real-time Updates
- Hasura uses polling for subscriptions
- FraiseQL can use PostgreSQL LISTEN/NOTIFY
- Consider using WebSockets for better performance

### 4. Authentication
- Hasura uses webhook/JWT
- PostGraphile uses PostgreSQL roles
- FraiseQL integrates with Python auth libraries

## Performance Tips

1. **Connection Pooling**: FraiseQL uses asyncpg for efficient pooling
2. **Query Optimization**: Use FraiseQL's query analyzer
3. **Caching**: Integrate with Redis for query caching
4. **Monitoring**: Use built-in Prometheus metrics

## Need Help?

- [FraiseQL Documentation](https://fraiseql.dev/docs)
- [Migration Examples](https://github.com/fraiseql/examples)
- [Community Discord](https://discord.gg/fraiseql)
- [Professional Support](https://fraiseql.dev/support)
