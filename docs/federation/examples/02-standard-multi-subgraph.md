# STANDARD Example: Multi-Subgraph with Type Extensions

**Complexity Level**: STANDARD (15% of federation users)
**Setup Time**: 20 minutes
**Key Features**: Type extensions, external fields, `@requires` directive

This example shows how two services **collaborate** without tight coupling:
- Service A defines User and Order
- Service B extends User with reviews and computed rating

## Architecture

```
┌─────────────────┐       ┌──────────────────┐
│  Users Service  │       │  Reviews Service │
│                 │       │                  │
│ @entity User    │◄──────┤ @extend_entity   │
│ @entity Order   │       │   User (extended)│
│                 │       │                  │
│ Users, Orders   │       │ Reviews, Ratings │
│ Table           │       │ Table            │
└─────────────────┘       └──────────────────┘
```

## Part 1: Users Service (Base Entity)

### Define Entities

```python
# users/models.py
from fraiseql import entity

@entity  # Auto-key on 'id'
class User:
    id: str
    name: str
    email: str
    created_at: str

@entity  # Auto-key on 'id'
class Order:
    id: str
    user_id: str
    total: float
    created_at: str
```

### Resolver

```python
# users/resolvers.py
from fraiseql.federation import EntitiesResolver, PerRequestBatchExecutor, Presets

resolver = EntitiesResolver()
executor = PerRequestBatchExecutor(
    # Use LITE preset (auto-keys only)
    # federation_config=Presets.LITE
)

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []
        for rep in representations:
            typename = rep["__typename"]
            entity_id = rep["id"]

            entity = await loader.load(typename, "id", entity_id)
            results.append(entity)

        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

### Schema Registration

```python
# users/schema.py
import strawberry

@strawberry.type
class Query:
    @strawberry.field
    async def _entities(self, info, representations: list[dict]) -> list:
        return await resolve_entities(info, representations)

schema = strawberry.Schema(query=Query)
```

## Part 2: Reviews Service (Type Extension)

### Extend User Type

```python
# reviews/models.py
from dataclasses import dataclass, field as dataclass_field
from fraiseql import extend_entity, external, requires

@dataclass
class Review:
    id: str
    user_id: str
    rating: int
    comment: str

@extend_entity(key="id")  # Key matches Users service
class User:
    # External: fields from Users service (read-only in this service)
    id: str = external()
    name: str = external()
    email: str = external()

    # New: fields this service provides
    reviews: list[Review] = dataclass_field(default_factory=list)
    average_rating: float = dataclass_field(default=0.0)

    # Computed field: requires external field
    @requires("reviews")
    async def review_count(self) -> int:
        """Count of reviews for this user."""
        return len(self.reviews)

    @requires("reviews")
    async def average_rating_computed(self) -> float:
        """Computed from reviews, not stored."""
        if not self.reviews:
            return 0.0
        return sum(r.rating for r in self.reviews) / len(self.reviews)
```

### Resolver with Extension

```python
# reviews/resolvers.py
from fraiseql.federation import EntitiesResolver, PerRequestBatchExecutor, Presets
from reviews.models import User

resolver = EntitiesResolver()
executor = PerRequestBatchExecutor(
    # Use STANDARD preset (supports extensions, requires)
    # federation_config=Presets.STANDARD
)

async def resolve_entities(info, representations):
    db_pool = info.context.get("db_pool")

    async def handle_request(loader):
        results = []

        for rep in representations:
            typename = rep["__typename"]

            if typename == "User":
                user_id = rep["id"]

                # Load user stub (with external fields)
                user = User(
                    id=user_id,
                    name=rep.get("name"),
                    email=rep.get("email"),
                )

                # Load reviews for this user
                # In a real app: reviews = await db.fetch(...)
                reviews = await db.fetch(
                    "SELECT * FROM reviews WHERE user_id = ?",
                    user_id
                )

                user.reviews = [Review(**r) for r in reviews]

                results.append(user)
            else:
                # Handle other types if extended
                pass

        return results

    return await executor.execute_request(handle_request, resolver, db_pool)
```

### Schema

```python
# reviews/schema.py
import strawberry
from reviews.models import Review

@strawberry.type
class ReviewType:
    id: str
    rating: int
    comment: str

@strawberry.type
class Query:
    @strawberry.field
    async def _entities(self, info, representations: list[dict]) -> list:
        return await resolve_entities(info, representations)

schema = strawberry.Schema(query=Query)
```

## How They Work Together

### Step 1: Frontend Queries

```graphql
query {
  users {
    id
    name
    email
    reviews {
      rating
      comment
    }
    average_rating
  }
}
```

### Step 2: Apollo Gateway Plans

The gateway understands:
- `User` is defined in Users Service
- `reviews` and `average_rating` are defined in Reviews Service
- Plan: **Fetch from Users first, then extend with Reviews**

```
Query Plan:
1. Fetch User{ id, name, email } from Users Service
2. Fetch User{ reviews, average_rating } from Reviews Service
3. Merge results
```

### Step 3: Federation Resolution

**Request 1: Users Service**
```python
_entities(representations: [
  { __typename: "User", id: "user-1" },
  { __typename: "User", id: "user-2" }
])
```

Users Service returns:
```json
{
  "entities": [
    { "__typename": "User", "id": "user-1", "name": "Alice", "email": "alice@..." },
    { "__typename": "User", "id": "user-2", "name": "Bob", "email": "bob@..." }
  ]
}
```

**Request 2: Reviews Service** (with References)
```python
_entities(representations: [
  { __typename: "User", id: "user-1", name: "Alice", email: "alice@..." },
  { __typename: "User", id: "user-2", name: "Bob", "email": "bob@..." }
])
```

Reviews Service receives the **full User object** (including external fields from Users Service) and extends it:
```json
{
  "entities": [
    {
      "__typename": "User",
      "id": "user-1",
      "name": "Alice",
      "email": "alice@...",
      "reviews": [
        { "rating": 5, "comment": "Great!" },
        { "rating": 4, "comment": "Good" }
      ],
      "average_rating": 4.5
    },
    ...
  ]
}
```

### Step 4: Gateway Merges

Final result sent to frontend:
```json
{
  "users": [
    {
      "id": "user-1",
      "name": "Alice",
      "email": "alice@...",
      "reviews": [
        { "rating": 5, "comment": "Great!" },
        { "rating": 4, "comment": "Good" }
      ],
      "average_rating": 4.5
    },
    ...
  ]
}
```

## Batching Across Services

**The magic**: Each service batches independently within its request scope:

```
Frontend Query → Gateway

[Users Service Request]
  loader.load("User", "id", "user-1")
  loader.load("User", "id", "user-2")
  loader.load("User", "id", "user-3")
  [Batch window expires]
  ↓
  SELECT * FROM users WHERE id IN (1, 2, 3)  ← ONE query

[Reviews Service Request]
  loader.load("Review", "user_id", "user-1")
  loader.load("Review", "user_id", "user-2")
  loader.load("Review", "user_id", "user-3")
  [Batch window expires]
  ↓
  SELECT * FROM reviews WHERE user_id IN (1, 2, 3)  ← ONE query

Total: 2 DB queries (vs. 3+ without federation)
```

## Key Differences from LITE

| Aspect | LITE | STANDARD |
|--------|------|----------|
| Entity types | `@entity` | `@entity` + `@extend_entity` |
| Keys | Auto-detected | Explicit key + matching |
| External fields | N/A | `external()` marker required |
| Computed fields | N/A | `@requires` directive |
| Preset | `Presets.LITE` | `Presets.STANDARD` |
| Config | Minimal | Enables extensions |

## Common Patterns

### Pattern 1: Simple Extension (No Computed Fields)

```python
@extend_entity(key="id")
class User:
    id: str = external()
    name: str = external()

    # Just add new data, no computation
    profile: UserProfile  # New field from this service
```

### Pattern 2: Computed from External Fields

```python
@extend_entity(key="id")
class Product:
    id: str = external()
    price: float = external()

    @requires("price")
    async def price_in_cents(self) -> int:
        return int(self.price * 100)
```

### Pattern 3: Reference Another Entity

```python
@extend_entity(key="id")
class Post:
    id: str = external()
    author_id: str = external()

    # Other service can resolve author via ID
    author: User  # Gateway will resolve via _entities
```

## Debugging Multi-Service Calls

Enable logging on both services:

```python
import logging
logging.getLogger("fraiseql.federation").setLevel(logging.DEBUG)
```

You'll see:
- Each service's batch composition
- Dedup hits/misses per service
- Cross-service field references

## Testing

```python
@pytest.mark.asyncio
async def test_user_extension(db_pool):
    """Test Users service + Reviews extension."""
    executor = PerRequestBatchExecutor()

    # Users service loads
    users = await load_users_federation(executor, db_pool, ["user-1", "user-2"])
    assert len(users) == 2

    # Reviews service extends
    reviews = await load_reviews_extension(executor, db_pool, users)
    assert reviews[0].average_rating == 4.5
```

## Next Steps

- **Need advanced patterns?** See [ADVANCED Example](./03-advanced-composite-keys.md)
- **Performance issues?** See [Performance Guide](../performance-tuning.md)
- **Debugging errors?** See [Error Handling](../error-handling.md)

---

**Key insight**: STANDARD federation enables **teams to own their data** while building a **unified API**. No service needs to know about the others' implementation.
