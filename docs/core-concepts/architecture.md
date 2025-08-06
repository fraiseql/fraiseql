# Architecture

FraiseQL implements a unique architecture that combines Domain-Driven Design (DDD) principles with the Command Query Responsibility Segregation (CQRS) pattern, all built on top of PostgreSQL's powerful features.

## High-Level Architecture

```mermaid
graph TB
    Client[GraphQL Client]
    API[GraphQL API Layer]
    Schema[Schema Registry]
    Resolver[Query/Mutation Resolvers]
    Translator[Query Translator]
    Repo[Repository Layer]
    PG[(PostgreSQL Database)]
    
    Client -->|GraphQL Query| API
    API --> Schema
    Schema --> Resolver
    Resolver --> Translator
    Translator -->|SQL with JSONB| Repo
    Repo --> PG
    
    PG -->|JSONB Result| Repo
    Repo -->|Domain Objects| Resolver
    Resolver -->|GraphQL Response| API
    API --> Client
    
    subgraph "CQRS Pattern"
        Views[Views<br/>v_*, tv_*]
        Functions[Functions<br/>fn_*]
        Tables[Tables<br/>tb_*]
        
        Views -.->|READ| Tables
        Functions -.->|WRITE| Tables
    end
    
    Repo --> Views
    Repo --> Functions
```

## Request Flow

A typical request flows through FraiseQL in these stages:

### 1. GraphQL Query Reception
```graphql
query {
  user(id: "123") {
    id
    name
    posts {
      title
      createdAt
    }
  }
}
```

### 2. Schema Validation
The query is validated against the registered GraphQL schema, which is automatically generated from your Python type definitions.

### 3. Query Translation
The GraphQL query is translated directly to SQL in **sub-millisecond time**:

```sql
SELECT jsonb_build_object(
    'id', data->>'id',
    'name', data->>'name',
    'posts', data->'posts'
) AS result
FROM v_user_with_post
WHERE id = $1;
```

### 4. Single Database Query
One query is executed against PostgreSQL, returning JSONB data that already matches the GraphQL structure.

### 5. Response Formatting
The JSONB result is directly returned to the client with minimal processing.

## CQRS Implementation

FraiseQL strictly separates read and write operations:

### Write Side (Commands)
- PostgreSQL functions handle all mutations
- Prefix: `fn_` (e.g., `fn_create_user`)
- Transactional consistency guaranteed
- Business logic encapsulated in database

```sql
CREATE OR REPLACE FUNCTION fn_create_user(
    p_email text,
    p_name text
) RETURNS jsonb AS $$
BEGIN
    -- Business logic and validation
    INSERT INTO tb_users (email, name)
    VALUES (p_email, p_name)
    RETURNING jsonb_build_object(
        'success', true,
        'user', row_to_json(tb_users)
    );
END;
$$ LANGUAGE plpgsql;
```

### Read Side (Queries)
- PostgreSQL views provide data projection
- Prefix: `v_` for views, `tv_` for table views
- Pre-aggregated data eliminates N+1 queries
- JSONB format matches GraphQL structure

```sql
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,  -- Used for filtering
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'createdAt', created_at
    ) AS data
FROM tb_users;
```

## Component Responsibilities

### Schema Registry
- **Location**: `src/fraiseql/gql/builders/registry.py`
- **Responsibility**: Central registration of all GraphQL types
- **Features**:
  - Automatic type discovery via decorators
  - Type validation and conflict detection
  - Schema generation from Python types

### Query Translator
- **Location**: `src/fraiseql/core/translate_query.py`
- **Responsibility**: GraphQL to SQL translation
- **Features**:
  - AST parsing and analysis
  - Field path extraction
  - WHERE clause generation
  - JSONB projection building

### Repository Layer
- **Location**: `src/fraiseql/cqrs/repository.py`
- **Responsibility**: Database abstraction
- **Features**:
  - CQRS command/query separation
  - Connection pooling
  - Transaction management
  - Result mapping

### Type System
- **Location**: `src/fraiseql/types/`
- **Responsibility**: Python-GraphQL type mapping
- **Features**:
  - Dataclass-based type definitions
  - Automatic GraphQL schema generation
  - Input/output type validation
  - Custom scalar support

## Caching Strategy

FraiseQL implements multiple caching layers:

### 1. PostgreSQL Query Cache
- Prepared statements for repeated queries
- Query plan caching
- Shared buffer cache for hot data

### 2. View Materialization
- Table views (`tv_*`) for expensive computations
- Incremental refresh strategies
- Background refresh jobs

### 3. Application-Level Caching
- DataLoader pattern for batch loading
- Request-scoped caching
- Optional Redis integration

```python
@fraiseql.dataloader_field
async def posts(self, info) -> list[Post]:
    # Automatically batched and cached within request
    return await info.context.db.load_posts_for_users([self.id])
```

## Multi-Tenancy Support

FraiseQL supports multiple multi-tenancy patterns:

### 1. Row-Level Security (RLS)
```sql
-- Enable RLS on tables
ALTER TABLE tb_posts ENABLE ROW LEVEL SECURITY;

-- Create policies
CREATE POLICY tenant_isolation ON tb_posts
    FOR ALL
    USING (tenant_id = current_setting('app.tenant_id'));
```

### 2. Schema Separation
```python
# Per-tenant schema
async with db.transaction() as tx:
    await tx.execute(f"SET search_path TO tenant_{tenant_id}")
    result = await repo.select_from_json_view("v_posts")
```

### 3. Database Separation
```python
# Per-tenant database connection
db = await FraiseQLRepository.create(
    database_url=f"postgresql://user:pass@host/{tenant_db}"
)
```

## Performance Considerations

### Query Performance
- **Single Query Execution**: One database round-trip per request
- **JSONB Indexing**: GIN indexes for fast JSON operations
- **View Optimization**: Pre-computed aggregations
- **Connection Pooling**: Efficient connection reuse

### Scaling Strategies
- **Horizontal Scaling**: Read replicas for queries
- **Vertical Scaling**: PostgreSQL optimization
- **Caching**: Multi-level caching strategy
- **Partitioning**: Table partitioning for large datasets

### Monitoring
- Query execution time tracking
- Slow query logging
- Connection pool metrics
- Cache hit rates

## Security Architecture

### SQL Injection Prevention
- Parameterized queries only
- Type validation at schema level
- Input sanitization in repository layer

### Authentication & Authorization
- Context-based authentication
- Field-level authorization
- Row-level security in PostgreSQL
- JWT token validation

```python
@fraiseql.query
@requires_auth
async def sensitive_data(info, id: str) -> SensitiveData:
    user = info.context.user
    if not user.has_permission("read:sensitive"):
        raise Unauthorized()
    return await info.context.db.get_sensitive_data(id)
```

## Deployment Architecture

### Containerized Deployment
```yaml
services:
  api:
    image: fraiseql-app:latest
    environment:
      DATABASE_URL: postgresql://...
    deploy:
      replicas: 3
      
  postgres:
    image: postgres:16
    volumes:
      - pgdata:/var/lib/postgresql/data
```

### Connection Management
- Connection pooling via asyncpg
- Configurable pool size
- Health checks and circuit breakers
- Automatic reconnection

## Key Design Decisions

### Why PostgreSQL-Only?
1. **Performance**: Native JSONB operations are incredibly fast
2. **Consistency**: Single source of truth
3. **Features**: Rich feature set (RLS, triggers, functions)
4. **Simplicity**: No ORM impedance mismatch

### Why CQRS?
1. **Scalability**: Read and write sides can scale independently
2. **Performance**: Optimized views for queries
3. **Clarity**: Clear separation of concerns
4. **Flexibility**: Different models for different needs

### Why Database-First?
1. **Type Safety**: Database schema as source of truth
2. **Performance**: Leverage PostgreSQL optimizations
3. **Maintainability**: Database migrations are explicit
4. **Portability**: Database logic is database-agnostic SQL

## Architecture Benefits

- **4-100x Performance**: Compared to traditional GraphQL servers
- **Zero N+1 Queries**: By design, not by optimization
- **Type Safety**: End-to-end from database to client
- **Simplicity**: Minimal abstraction layers
- **Reliability**: PostgreSQL's proven stability

## Next Steps

- Learn about [Database Views](./database-views.md) for query optimization
- Understand the [Type System](./type-system.md) for schema definition
- Explore [Query Translation](./query-translation.md) mechanics
- Review the [Blog API Tutorial](../tutorials/blog-api.md) for a complete example