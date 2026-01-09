# Architecture Overview

Complete architectural overview of FraiseQL's design principles, system components, and design decisions.

## System Architecture

FraiseQL is a PostgreSQL-native GraphQL framework that combines the power of relational databases with the flexibility of GraphQL APIs. The architecture is designed for performance, type safety, and developer experience.

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   GraphQL API   │────│  Query Engine   │────│   PostgreSQL    │
│                 │    │   (Rust)        │    │   Database      │
│ • Type Safety   │    │ • Performance   │    │ • ACID          │
│ • Schema-first  │    │ • Zero-copy     │    │ • Transactions   │
│ • Auto-generated│    │ • Async         │    │ • Indexing       │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         └───────────────────────┼───────────────────────┘
                                 │
                    ┌─────────────────┐
                    │  Application    │
                    │   (Python)      │
                    │ • Business Logic│
                    │ • HTTP Layer    │
                    │ • Middleware    │
                    └─────────────────┘
```

## Core Principles

### Database-First Design

FraiseQL treats PostgreSQL as the source of truth for data and types:

- **Schema Reflection**: Automatically infers GraphQL types from database schema
- **Type Safety**: Compile-time guarantees between GraphQL and SQL
- **Performance**: Direct SQL execution without ORM overhead
- **Migrations**: Schema changes automatically reflected in GraphQL API

### Rust Performance Layer

The query execution engine is implemented in Rust for maximum performance:

- **Zero-copy serialization** between PostgreSQL and GraphQL
- **Async execution** with tokio runtime
- **Memory safety** guarantees
- **Concurrent processing** of complex queries

### Python Developer Experience

The framework provides Python APIs while leveraging Rust performance:

- **Schema Definition**: Python decorators for type-safe schemas
- **Resolver Functions**: Async Python functions with automatic Rust bridging
- **Middleware System**: Extensible request/response processing
- **Testing Utilities**: Comprehensive test helpers and fixtures

## Component Architecture

### GraphQL Layer

```
GraphQL Schema (Python)
        ↓
Type System (Rust)
        ↓
Query Parser (Rust)
        ↓
Validation Engine (Rust)
        ↓
Execution Engine (Rust)
```

**Key Components:**

- **Schema Builder**: Converts Python types to GraphQL schema
- **Type System**: Manages type relationships and validation
- **Query Parser**: Parses GraphQL queries into execution plans
- **Validation**: Ensures query correctness and security
- **Execution**: Orchestrates query execution across components

### Database Layer

```
Connection Pool (Python/Rust)
        ↓
Query Builder (Rust)
        ↓
SQL Execution (PostgreSQL)
        ↓
Result Processing (Rust)
        ↓
GraphQL Serialization (Rust)
```

**Database Integration:**

- **Connection Management**: Efficient pooling and lifecycle management
- **Query Optimization**: Automatic query planning and optimization
- **Result Caching**: PostgreSQL-based caching layer
- **Transaction Support**: ACID compliance for mutations
- **Migration Handling**: Schema change detection and adaptation

### HTTP Server Layer

```
HTTP Request (Framework)
        ↓
GraphQL Parsing (Rust)
        ↓
Query Execution (Rust)
        ↓
Response Formatting (Framework)
        ↓
HTTP Response (Framework)
```

**Server Options:**

- **Axum**: Maximum performance, Rust-native HTTP server
- **Starlette**: Modern async Python with Rust acceleration
- **FastAPI**: Enterprise features with automatic documentation
- **Custom**: Extensible framework for specialized needs

## Design Patterns

### Trinity Pattern

FraiseQL's core architectural pattern combining three essential components:

```python
# 1. Schema Definition (Python)
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str

    @fraiseql.field
    async def posts(self, info) -> list[Post]:
        # 2. Business Logic (Python)
        return await db.get_posts_by_author(self.id)

# 3. Database Mapping (Automatic)
# CREATE TABLE users (id UUID, name TEXT, email TEXT);
# GraphQL types automatically derived from schema
```

**Benefits:**
- **Type Safety**: End-to-end type checking
- **Performance**: Optimized execution paths
- **Maintainability**: Clear separation of concerns
- **Extensibility**: Easy to modify and extend

### CQRS Pattern

Command Query Responsibility Segregation for optimal read/write patterns:

```python
# Query Side - Optimized for reads
@fraiseql.query
async def get_users(info, limit: int = 10) -> list[User]:
    # Fast read path, potentially cached
    return await user_repository.get_recent(limit)

# Command Side - Optimized for writes
@fraiseql.mutation
async def create_user(info, input: CreateUserInput) -> User:
    # Validation and business logic
    user = await user_service.create(input)

    # Event publishing for cache invalidation
    await event_bus.publish("user.created", user.id)

    return user
```

### Repository Pattern

Data access abstraction for testability and maintainability:

```python
class UserRepository:
    def __init__(self, db_pool: DatabasePool):
        self.db = db_pool

    async def get_by_id(self, user_id: UUID) -> User | None:
        """Get user by ID with optimized query."""
        return await self.db.fetch_one(
            "SELECT id, name, email FROM users WHERE id = $1",
            user_id
        )

    async def create(self, user_data: dict) -> User:
        """Create user with validation."""
        # Business logic validation
        validated_data = await self.validate_user_data(user_data)

        # Database insertion
        user_id = await self.db.fetch_val(
            "INSERT INTO users (name, email) VALUES ($1, $2) RETURNING id",
            validated_data["name"], validated_data["email"]
        )

        return await self.get_by_id(user_id)
```

## Type System Architecture

### Type Inference

Automatic type derivation from database schema:

```sql
-- Database schema
CREATE TABLE users (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    created_at TIMESTAMP DEFAULT NOW()
);

-- Automatically becomes GraphQL type
type User {
    id: ID!
    name: String!
    email: String!
    createdAt: DateTime!
}
```

### Type Operators

Advanced type operations for complex schemas:

```python
# Union types
SearchResult = Union[User, Post, Comment]

# Interface implementation
@fraiseql.interface
class Node:
    id: UUID

@fraiseql.type
class User(Node):
    id: UUID
    name: str

# Generic types
@fraiseql.type
class PaginatedResult[T]:
    items: list[T]
    total_count: int
    has_next_page: bool
```

### Schema Evolution

Safe schema changes with backward compatibility:

```python
# Versioned schema changes
@fraiseql.schema_version("2.0.0")
class UserV2:
    id: UUID
    name: str
    email: str
    profile_picture_url: str | None  # New field

# Automatic migration handling
migrator = SchemaMigrator()
await migrator.apply_changes([
    AddField("User", "profilePictureUrl", "String"),
    MakeFieldNullable("User", "phoneNumber")
])
```

## Performance Architecture

### Query Execution Pipeline

```
GraphQL Query
     ↓
Parse GraphQL (Phase 1 - Rust)
     ↓
Process Advanced Selections (Phase 5 - Rust)
├─ Resolve Fragments (@see fragment_resolver.rs)
├─ Evaluate Directives (@see directive_evaluator.rs)
└─ Finalize Selections (@see advanced_selections.rs)
     ↓
Validate Schema (Phase 2-3 - Rust)
     ↓
Query Planning (Phase 4 - Rust)
     ↓
SQL Generation (Phase 4 - Rust)
     ↓
Execution (Phase 6 - PostgreSQL)
     ↓
Result Processing (Phase 7 - Rust)
     ↓
GraphQL Serialization (Phase 7 - Rust)
```

#### Phase 5: Advanced Selections (Fragment & Directive Processing)

**Newly Integrated Phase** (January 2026):

Fragment resolution and directive evaluation are now core parts of the execution pipeline:

- **Fragment Resolution**: Handles named fragments (`...FragmentName`) and inline fragments (`... on Type`)
- **Directive Evaluation**: Processes `@skip` and `@include` directives with variable support
- **Selection Coordination**: Builds final field selection set from fragments, directives, and explicit fields

These features enable complex GraphQL operations to be executed efficiently within the Rust pipeline with zero Python GIL overhead.

### Caching Layers

Multi-level caching for optimal performance:

```python
# 1. Result Caching (PostgreSQL-based)
@fraiseql.query(cache=True, ttl=300)
async def get_popular_posts(info) -> list[Post]:
    return await db.get_popular_posts()

# 2. Field-level Caching
@fraiseql.field(cache_key="user:{user_id}:profile")
async def user_profile(self, info) -> UserProfile:
    return await db.get_user_profile(self.id)

# 3. Application-level Caching
from fraiseql.cache import CacheManager

cache = CacheManager(
    l1_cache=RedisCache(),      # Fast L1 cache
    l2_cache=PostgresCache(),   # Durable L2 cache
    l3_cache=FileCache()        # Fallback cache
)
```

### Connection Pool Optimization

```python
# Adaptive connection pooling
pool = AdaptivePool(
    min_size=5,
    max_size=100,
    growth_factor=1.5,
    shrink_threshold=0.3,
    health_check_interval=30
)

# Automatic scaling based on load
pool.start_adaptive_scaling()
```

## Security Architecture

### Authentication & Authorization

Multi-layer security with defense in depth:

```python
# Authentication providers
auth_providers = [
    JWTAuthProvider(secret_key="jwt_secret"),
    OAuth2Provider(client_id="oauth_client"),
    ApiKeyProvider(valid_keys=["key1", "key2"])
]

# Authorization middleware
@fraiseql.middleware
async def authorization_middleware(info, next):
    user = info.context.user

    # Role-based access control
    if not user.has_role("admin"):
        raise ForbiddenError("Admin access required")

    return await next(info)
```

### Data Protection

Row-level security and data isolation:

```python
# Tenant isolation
@fraiseql.type
class Organization:
    id: UUID
    name: str
    tenant_id: UUID = Field(hidden=True)

    # Automatic tenant filtering
    @classmethod
    async def get_queryset(cls, info):
        tenant_id = info.context.tenant_id
        return f"SELECT * FROM organizations WHERE tenant_id = '{tenant_id}'"

# Field-level encryption
@fraiseql.type
class User:
    id: UUID
    name: str
    email: str

    @fraiseql.field(encrypted=True)
    async def ssn(self, info) -> str:
        # Automatic encryption/decryption
        return await db.get_encrypted_field(self.id, "ssn")
```

## Deployment Architecture

### Horizontal Scaling

```yaml
# Kubernetes deployment for scalability
apiVersion: apps/v1
kind: Deployment
metadata:
  name: fraiseql-api
spec:
  replicas: 10  # Horizontal scaling
  template:
    spec:
      containers:
      - name: api
        image: fraiseql/api:latest
        resources:
          requests:
            memory: "256Mi"
            cpu: "200m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
        # Readiness and liveness probes
        readinessProbe:
          httpGet:
            path: /health
            port: 8000
        livenessProbe:
          httpGet:
            path: /health
            port: 8000
```

### Database Scaling

```sql
-- Read replicas for horizontal read scaling
-- Primary: writes
-- Replicas: reads

-- Connection routing
if operation.is_read_only:
    connection = replica_pool.get_connection()
else:
    connection = primary_pool.get_connection()
```

### Service Mesh Integration

```yaml
# Istio service mesh configuration
apiVersion: networking.istio.io/v1alpha3
kind: VirtualService
metadata:
  name: fraiseql-api
spec:
  http:
  - match:
    - uri:
        prefix: "/graphql"
    route:
    - destination:
        host: fraiseql-api
    # Circuit breaker
    retries:
      attempts: 3
      perTryTimeout: 2s
    # Load balancing
    trafficPolicy:
      loadBalancer:
        simple: ROUND_ROBIN
```

## Monitoring & Observability

### Metrics Collection

Comprehensive metrics for operational visibility:

```python
from fraiseql.monitoring import MetricsCollector

metrics = MetricsCollector()

# Query performance metrics
@metrics.timer("graphql_query_duration")
async def execute_graphql(query: str) -> dict:
    return await engine.execute(query)

# Business metrics
metrics.counter("users_created").increment()
metrics.gauge("active_connections").set(pool.active_count)
metrics.histogram("query_complexity").observe(complexity_score)
```

### Distributed Tracing

End-to-end request tracing across services:

```python
from fraiseql.tracing import Tracer

tracer = Tracer(service_name="fraiseql-api")

@fraiseql.query
async def get_user_posts(info, user_id: UUID) -> list[Post]:
    with tracer.span("get_user_posts") as span:
        span.set_attribute("user_id", str(user_id))

        # Trace database call
        with tracer.span("db_query") as db_span:
            posts = await db.get_posts_by_user(user_id)
            db_span.set_attribute("result_count", len(posts))

        return posts
```

### Health Checks

Automated health monitoring:

```python
from fraiseql.health import HealthChecker

health_checker = HealthChecker()

# Database connectivity
health_checker.add_check("database", check_db_connection)

# External service dependencies
health_checker.add_check("redis", check_redis_connection)
health_checker.add_check("email_service", check_email_service)

# Application health
@app.get("/health")
async def health_endpoint():
    status = await health_checker.run_checks()
    return {"status": "healthy" if status.all_passed else "unhealthy"}
```

## Design Decisions

### Why PostgreSQL-Native?

**Decision**: Build directly on PostgreSQL instead of using ORMs

**Rationale**:
- **Performance**: Direct SQL execution eliminates ORM overhead
- **Type Safety**: Schema reflection provides compile-time guarantees
- **Flexibility**: Full access to PostgreSQL features and extensions
- **Ecosystem**: Leverages existing PostgreSQL tooling and expertise

**Trade-offs**:
- Higher learning curve than ORM-based frameworks
- Requires SQL knowledge for complex queries
- Less abstraction from database-specific features

### Why Rust for Performance Layer?

**Decision**: Implement core execution engine in Rust

**Rationale**:
- **Performance**: Native speed for CPU-intensive operations
- **Memory Safety**: Prevents memory corruption and leaks
- **Concurrency**: Excellent async runtime with tokio
- **Interoperability**: Seamless Python integration via PyO3

**Trade-offs**:
- Increased complexity for contributors
- Rust learning curve for core development
- Compilation time overhead during development

### Ultra-Direct Mutation Path

**Decision**: Skip Python parsing for mutations, use PostgreSQL JSONB → Rust → Direct Response

**Before (Slow)**: PostgreSQL → Python dict → GraphQL serializer → JSON → Client

**After (Fast)**: PostgreSQL JSONB → Rust Pipeline → GraphQL JSON Response → Client

**Impact**: 10-80x performance improvement for mutations, same optimization as queries

### CQRS Pattern Implementation

**Decision**: Separate read and write models for optimal performance

**Queries**: Optimized read models with caching and aggregation
**Mutations**: Direct database operations with event publishing
**Synchronization**: Event-driven cache invalidation and data consistency

**Benefits**: Optimized read/write patterns, scalable architecture

### Trinity Pattern

**Decision**: Combine schema definition, business logic, and database mapping

```python
@fraiseql.type
class User:
    id: UUID
    name: str

    @fraiseql.field
    async def posts(self, info) -> list[Post]:
        # Business logic
        return await db.get_posts_by_author(self.id)

# Automatic SQL generation and type mapping
```

**Benefits**: Type safety, performance, maintainability

### Why Schema-First GraphQL?

**Decision**: Database schema drives GraphQL schema generation

**Rationale**:
- **Consistency**: Single source of truth for data types
- **Type Safety**: Automatic synchronization between DB and API
- **Maintainability**: Schema changes automatically reflected
- **Performance**: Optimized queries based on actual schema

**Trade-offs**:
- Less flexibility than code-first approaches
- Migration complexity for schema changes
- Requires database design upfront

## Next Steps

- [Getting Started](../getting-started/README.md) - Start building with FraiseQL
- [API Reference](../api/python-api.md) - Complete API documentation
- [Performance Tuning](../guides/performance-tuning.md) - Optimize for production
- [Contributing](../contributing/setup.md) - Join the development community

---

**FraiseQL's architecture combines the reliability of relational databases with the flexibility of GraphQL, powered by Rust performance and Python developer experience.**
