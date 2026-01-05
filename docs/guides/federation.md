# Federation Guide

Complete guide to implementing GraphQL federation with FraiseQL for distributed GraphQL architectures.

## Overview

GraphQL federation allows you to compose multiple GraphQL services into a single unified API. FraiseQL provides native federation support through schema stitching and entity resolution.

**Key Benefits:**
- **Service Decomposition**: Break monolithic APIs into focused microservices
- **Independent Deployment**: Deploy services independently without breaking clients
- **Type Safety**: Maintain type safety across service boundaries
- **Performance**: Efficient entity resolution and query planning

## Quick Start

### Basic Federation Setup

```python
# Service A: Users service
from fraiseql import create_app
from fraiseql.federation import FederationSchema, Entity

app_a = create_app(
    schema=FederationSchema(
        entities=[UserEntity],
        extends_from=["service-b"]
    )
)

# Service B: Posts service
app_b = create_app(
    schema=FederationSchema(
        entities=[PostEntity],
        extends_from=["service-a"]
    )
)
```

### Entity Definition

```python
from fraiseql.federation import Entity, Key

@fraiseql.type
@Entity(keys=[Key("id")])
class User:
    id: UUID
    name: str
    email: str

    @fraiseql.field
    async def posts(self, info) -> list[Post]:
        # Resolve posts from posts service
        return await federation.resolve_entity("Post", {"author_id": self.id})

@fraiseql.type
@Entity(keys=[Key("id")])
class Post:
    id: UUID
    title: str
    content: str
    author_id: UUID

    @fraiseql.field
    async def author(self, info) -> User:
        # Resolve author from users service
        return await federation.resolve_entity("User", {"id": self.author_id})
```

## Architecture

### Federation Components

```
Gateway Layer
├── Schema Registry (SDLs from all services)
├── Query Planner (routes requests to services)
└── Response Composer (merges results)

Service Layer
├── Subgraph Schemas (service-specific types)
├── Entity Resolvers (cross-service references)
└── Federation Directives (@key, @extends, @external)
```

### Service Types

**Subgraph Services:**
- Own specific domain entities
- Define @key directives for entity references
- Implement resolvers for external fields

**Gateway Service:**
- Composes all subgraph schemas
- Plans and routes queries
- Composes responses from multiple services

## Schema Design

### Entity Keys

```graphql
# Users service
type User @key(fields: "id") {
  id: ID!
  name: String!
  email: String!
}

# Posts service
type Post @key(fields: "id") {
  id: ID!
  title: String!
  content: String!
  author: User!  # Reference to User entity
}

# Extend User in Posts service
extend type User @key(fields: "id") {
  id: ID! @external
  posts: [Post!]!
}
```

### Reference Resolution

```python
from fraiseql.federation import EntityResolver

class PostResolver(EntityResolver):
    @resolver("User.posts")
    async def resolve_user_posts(self, user_id: UUID) -> list[Post]:
        """Resolve posts for a user from posts service."""
        return await db.get_posts_by_author(user_id)

    @resolver("Post.author")
    async def resolve_post_author(self, post_id: UUID) -> User:
        """Resolve author for a post from users service."""
        post = await db.get_post(post_id)
        return await federation.resolve_entity("User", {"id": post.author_id})
```

## Gateway Configuration

### Apollo Gateway Setup

```python
from apollo_gateway import ApolloGateway
from fraiseql.federation import FederationGateway

# Configure subgraph services
gateway = FederationGateway(
    services=[
        {
            "name": "users",
            "url": "http://users-service:4001/graphql"
        },
        {
            "name": "posts",
            "url": "http://posts-service:4002/graphql"
        }
    ]
)

app = create_app(gateway=gateway)
```

### Custom Gateway

```python
from fraiseql.federation import CustomGateway

class MyGateway(CustomGateway):
    async def plan_query(self, query: str) -> QueryPlan:
        """Custom query planning logic."""
        # Analyze query and determine service routing
        plan = self.analyzer.analyze(query)

        # Route to appropriate services
        return QueryPlan(
            services=["users", "posts"],
            operations=plan.operations
        )

    async def compose_response(self, results: dict) -> dict:
        """Compose final response from service results."""
        return self.composer.merge(results)
```

## Cross-Service Communication

### Entity Resolution

```python
from fraiseql.federation import FederationClient

client = FederationClient()

# Resolve entities from other services
@fraiseql.field
async def author(self, info) -> User:
    # Fetch from users service
    return await client.resolve_entity(
        service="users",
        type="User",
        key={"id": self.author_id}
    )
```

### Batch Resolution

```python
from fraiseql.federation import DataLoader

class UserLoader(DataLoader):
    async def batch_load(self, keys):
        # Single request to users service for multiple users
        return await client.batch_resolve_entities(
            service="users",
            type="User",
            keys=[{"id": key} for key in keys]
        )

loader = UserLoader()

@fraiseql.field
async def authors(self, info) -> list[User]:
    # Efficient batch loading
    return await loader.load_many(self.author_ids)
```

## Error Handling

### Federation Errors

```python
from fraiseql.federation import FederationError

@fraiseql.field
async def user_profile(self, info) -> UserProfile:
    try:
        return await federation.resolve_entity("User", {"id": self.user_id})
    except FederationError as e:
        if e.service_unavailable:
            # Fallback to local data
            return await db.get_cached_user_profile(self.user_id)
        elif e.entity_not_found:
            # Return null for missing entities
            return None
        else:
            raise GraphQLError(f"Federation error: {e.message}")
```

### Circuit Breaker Pattern

```python
from fraiseql.federation import CircuitBreaker

breaker = CircuitBreaker(
    failure_threshold=5,
    recovery_timeout=60,
    expected_exception=FederationError
)

@fraiseql.field
async def federated_data(self, info):
    async with breaker:
        return await federation.resolve_entity("RemoteType", {"id": self.id})
```

## Performance Optimization

### Query Planning

```python
from fraiseql.federation import QueryPlanner

planner = QueryPlanner()

# Analyze query for optimal execution
plan = await planner.plan("""
    query GetUserWithPosts($userId: ID!) {
        user(id: $userId) {
            name
            posts {
                title
                author {
                    name
                }
            }
        }
    }
""", variables={"userId": "123"})

# Plan shows:
# 1. Fetch user from users service
# 2. Fetch posts from posts service
# 3. Resolve author references
```

### Caching Strategies

```python
from fraiseql.federation import FederationCache

cache = FederationCache(
    # Cache entity resolutions
    entity_cache_ttl=300,
    # Cache query plans
    plan_cache_ttl=3600
)

federation = FederationClient(cache=cache)
```

### Parallel Execution

```python
import asyncio

@fraiseql.field
async def user_stats(self, info) -> UserStats:
    # Execute in parallel across services
    user_future = federation.resolve_entity("User", {"id": self.user_id})
    posts_future = federation.resolve_entities("Post", {"author_id": self.user_id})
    comments_future = federation.resolve_entities("Comment", {"user_id": self.user_id})

    user, posts, comments = await asyncio.gather(
        user_future, posts_future, comments_future
    )

    return UserStats(
        user=user,
        post_count=len(posts),
        comment_count=len(comments)
    )
```

## Testing Federation

### Unit Testing

```python
import pytest
from fraiseql.federation.testing import FederationTestClient

@pytest.fixture
async def federation_client():
    return FederationTestClient(services=["users", "posts"])

@pytest.mark.asyncio
async def test_cross_service_resolution(federation_client):
    # Mock service responses
    federation_client.mock_service("users", {
        "User": {"id": "1", "name": "John"}
    })

    federation_client.mock_service("posts", {
        "Post": [{"id": "1", "title": "Hello", "author_id": "1"}]
    })

    # Test federated query
    result = await federation_client.execute("""
        query {
            posts {
                title
                author {
                    name
                }
            }
        }
    """)

    assert result.data.posts[0].author.name == "John"
```

### Integration Testing

```python
from fraiseql.federation.testing import FederationTestSuite

suite = FederationTestSuite()

@suite.test_federation
async def test_user_posts_federation():
    """Test user-posts relationship across services."""
    # Start test services
    users_service = await suite.start_service("users")
    posts_service = await suite.start_service("posts")

    # Create test data
    user = await users_service.create_user(name="Alice")
    post = await posts_service.create_post(title="Hello", author_id=user.id)

    # Test federated query
    result = await suite.query_gateway("""
        query GetUserPosts($userId: ID!) {
            user(id: $userId) {
                name
                posts {
                    title
                }
            }
        }
    """, variables={"userId": user.id})

    assert len(result.data.user.posts) == 1
    assert result.data.user.posts[0].title == "Hello"
```

## Deployment & Operations

### Service Discovery

```python
from fraiseql.federation import ServiceRegistry

registry = ServiceRegistry(
    discovery_method="kubernetes",
    namespace="production"
)

gateway = FederationGateway(
    service_registry=registry,
    # Services auto-discovered from Kubernetes
)
```

### Health Checks

```python
from fraiseql.federation import FederationHealthChecker

health_checker = FederationHealthChecker(
    services=["users", "posts", "comments"],
    timeout=5  # seconds
)

@app.get("/health/federation")
async def federation_health():
    results = await health_checker.check_all()

    return {
        "status": "healthy" if all(r.healthy for r in results) else "degraded",
        "services": {r.service: r.healthy for r in results}
    }
```

### Monitoring

```python
from fraiseql.federation.metrics import FederationMetrics

metrics = FederationMetrics()

# Track federation performance
federation = FederationClient(metrics=metrics)

# Available metrics:
# - federation_entity_resolution_duration
# - federation_service_request_count
# - federation_error_rate
# - federation_cache_hit_rate
```

## Migration Strategy

### From Monolithic to Federated

**Phase 1: Extract Services**
```
Monolithic API
├── Users domain → Users service
├── Posts domain → Posts service
└── Comments domain → Comments service
```

**Phase 2: Add Federation Layer**
```
Gateway
├── Users service (subgraph)
├── Posts service (subgraph)
└── Comments service (subgraph)
```

**Phase 3: Gradual Migration**
- Start with read-only federation
- Add write operations gradually
- Maintain backward compatibility

### Schema Evolution

```python
from fraiseql.federation import SchemaEvolution

evolver = SchemaEvolution()

# Safe schema changes
@evolver.migrate(version="2.0.0")
async def add_user_profile():
    """Add profile field to User entity."""
    await federation.update_schema("""
        extend type User {
            profile: UserProfile
        }
    """)

# Breaking changes with compatibility
@evolver.migrate(version="3.0.0", breaking=True)
async def rename_field():
    """Rename field with backward compatibility."""
    await federation.add_field_alias("oldField", "newField")
    # Keep old field for compatibility period
```

## Best Practices

### Schema Design

1. **Clear Entity Ownership**
   - Each entity owned by one service
   - Use @key for cross-service references

2. **Minimize Cross-Service Calls**
   - Design schemas to minimize entity resolution
   - Use batch loading for multiple entities

3. **Version Compatibility**
   - Use semantic versioning for subgraph APIs
   - Maintain backward compatibility during migrations

### Performance

1. **Caching Strategy**
   - Cache entity resolutions
   - Cache query plans
   - Use appropriate TTL values

2. **Monitoring**
   - Track cross-service latency
   - Monitor error rates
   - Set up alerts for service degradation

### Operations

1. **Deployment Coordination**
   - Deploy gateway last
   - Roll back subgraph deployments safely
   - Use canary deployments for critical changes

2. **Testing**
   - Test entity resolution thoroughly
   - Use contract testing between services
   - Test failure scenarios

## Troubleshooting

### Common Issues

**Entity Resolution Failures:**
- Check service connectivity
- Verify @key directives match
- Ensure entity resolvers are implemented

**Query Planning Issues:**
- Review schema composition
- Check for circular dependencies
- Validate type extensions

**Performance Problems:**
- Monitor cross-service call latency
- Implement proper caching
- Consider query optimization

### Debug Tools

```python
from fraiseql.federation.debug import FederationDebugger

debugger = FederationDebugger()

# Trace entity resolution
with debugger.trace():
    result = await federation.execute_query(query)

# View resolution path
print(debugger.resolution_path)
# Output: users → posts → comments

# Check service dependencies
dependencies = debugger.analyze_dependencies()
print(dependencies)
# Output: {"users": [], "posts": ["users"], "comments": ["users", "posts"]}
```

## Next Steps

- [Schema Design Guide](../architecture/schema-design.md) - Federation schema patterns
- [Performance Tuning](../guides/performance-tuning.md) - Optimize federation performance
- [Testing Guide](../contributing/testing.md) - Federation testing strategies
- [Migration Examples](../examples/federation-setup.md) - Complete migration examples

---

**Federation enables scalable, maintainable GraphQL architectures. Start with clear entity boundaries and gradual decomposition for best results.**
