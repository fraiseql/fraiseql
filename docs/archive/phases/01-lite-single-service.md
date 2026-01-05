# LITE Example: Single Service with Auto-Key Detection

**Complexity Level**: LITE (80% of federation users)
**Setup Time**: 5 minutes
**Key Features**: Auto-detected entity keys, minimal config

This is the **canonical onboarding example**. If you copy this and it works, you've successfully set up FraiseQL federation.

## The Problem

You have a Users service. Other services want to resolve users by ID without directly querying your database.

```
Frontend → GraphQL Gateway → Users Service
                         → Orders Service (wants User data)
                         → Products Service (wants User data)
```

## The Solution

Mark your `User` type as an entity. FraiseQL handles the rest.

## Complete Working Example

### 1. Define Your Entity

```python
# users/models.py
from fraiseql import entity

@entity  # ← That's it. "id" is auto-detected as the key.
class User:
    id: str
    name: str
    email: str
    created_at: str
```

**What `@entity` does:**
- Registers `User` as a federated entity
- Auto-detects `id` field as the key
- Prepares it for federation resolution

### 2. Implement the Resolver

```python
# users/resolvers.py
from fraiseql.federation import EntitiesResolver, PerRequestBatchExecutor
from fraiseql.federation import Presets

# Create resolver instance (once at startup)
entities_resolver = EntitiesResolver()
batch_executor = PerRequestBatchExecutor(
    batch_window_ms=1.0,
    max_batch_size=None
)

async def resolve_entities(info, representations):
    """
    Called by GraphQL Gateway for _entities query.

    Receives list of: {"__typename": "User", "id": "123"}
    Returns list of: User objects
    """

    # Get the database pool from context
    db_pool = info.context.get("db_pool")

    # Use batch executor for request-scoped batching
    async def handle_request(loader):
        results = []
        for representation in representations:
            typename = representation["__typename"]
            entity_id = representation["id"]

            # Load user with automatic batching
            user = await loader.load(typename, "id", entity_id)
            results.append(user)

        return results

    return await batch_executor.execute_request(
        handle_request,
        entities_resolver,
        db_pool
    )
```

### 3. Register the _entities Query

```python
# users/schema.py
import strawberry
from strawberry.fastapi import GraphQLRouter
from users.resolvers import resolve_entities

@strawberry.type
class Query:
    @strawberry.field
    async def _entities(
        self,
        info,
        representations: list[dict]
    ) -> list:
        """Federation _entities query."""
        return await resolve_entities(info, representations)

schema = strawberry.Schema(query=Query)
router = GraphQLRouter(schema)
```

### 4. FastAPI Integration

```python
# main.py
from fastapi import FastAPI
from asyncpg import create_pool
from users.schema import router

app = FastAPI()

# Create database pool at startup
@app.on_event("startup")
async def startup():
    app.db_pool = await create_pool("postgresql://localhost/users")

# Add GraphQL router
app.include_router(router, prefix="/graphql")

# Pass db_pool to context
@app.middleware("http")
async def add_db_pool(request, call_next):
    request.state.db_pool = app.db_pool
    return await call_next(request)
```

### 5. Register with Apollo Gateway

```yaml
# apollo-gateway config
services:
  - name: users
    url: http://localhost:8001/graphql

  - name: orders
    url: http://localhost:8002/graphql
```

## That's It!

Your Users service now supports federation. The gateway can now:

```graphql
query {
  _entities(representations: [
    { __typename: "User", id: "user-1" },
    { __typename: "User", id: "user-2" }
  ]) {
    __typename
    ... on User {
      id
      name
      email
    }
  }
}
```

**What happens internally:**
1. Gateway calls `_entities` with 2 User representations
2. Your resolver calls `loader.load()` twice
3. DataLoader batches both into one query: `SELECT * FROM users WHERE id IN ('user-1', 'user-2')`
4. FraiseQL returns both users
5. Gateway gets the response

## Configuration Defaults

You didn't specify any configuration, but you got:

```python
FederationConfig(
    enabled=True,
    version="2.5",
    auto_keys=True,           # ← Detects 'id' field
    auto_entities_resolver=True,
    auto_service_resolver=True,
    directives=["key", "external"],
    batch_size=100,
    batch_window_ms=10,
    cache_sdl=True,
    cache_ttl_seconds=3600,
)
```

All preset for LITE federation. No tweaking needed.

## Common Questions

### Q: What if my key isn't `id`?

Specify it explicitly:

```python
@entity(key="user_id")
class User:
    user_id: str
    name: str
```

### Q: What if I have multiple entity types?

Just add more `@entity` decorators:

```python
@entity
class User:
    id: str
    name: str

@entity
class Post:
    id: str
    title: str
    author_id: str
```

FraiseQL batches each type separately. Efficient and automatic.

### Q: What if federation breaks?

Enable debug logging:

```python
import logging
logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)
```

You'll see:
- Every `load()` call
- When batch flushes
- Cache hits/misses
- Database queries executed

### Q: How do I test this?

```python
# tests/test_federation.py
import pytest
from fraiseql.federation import EntitiesResolver, PerRequestBatchExecutor

@pytest.mark.asyncio
async def test_user_federation(db_pool):
    executor = PerRequestBatchExecutor()
    resolver = EntitiesResolver()

    async def load_users(loader):
        user1 = await loader.load("User", "id", "user-1")
        user2 = await loader.load("User", "id", "user-2")
        return [user1, user2]

    users = await executor.execute_request(load_users, resolver, db_pool)
    assert len(users) == 2
    assert users[0].name == "Alice"
```

## Next Steps

- **Ready to extend types?** See [STANDARD Example](./02-standard-multi-subgraph.md)
- **Need performance tuning?** See [Performance Guide](../performance-tuning.md)
- **Debugging issues?** See [Error Handling](../error-handling.md)

---

**Success metric**: This example should take <5 minutes to copy-paste and run. If it takes longer, we've failed in clarity.
