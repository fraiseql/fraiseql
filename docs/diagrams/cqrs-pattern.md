# CQRS Pattern in FraiseQL

## Overview
FraiseQL implements the Command Query Responsibility Segregation (CQRS) pattern to optimize read and write operations separately. This separation allows for different optimization strategies for queries (reads) and mutations (writes).

## ASCII Art Diagram

```
┌─────────────────────────────────────┐
│         GraphQL API                 │
├──────────────────┬──────────────────┤
│   QUERIES        │   MUTATIONS      │
│   (Reads)        │   (Writes)       │
├──────────────────┼──────────────────┤
│  tv_* tables     │  fn_* functions  │
│  v_* views       │  tb_* tables     │
│  (JSONB)         │  (Business Logic)│
├──────────────────┼──────────────────┤
│  Fast reads      │  ACID compliance │
│  Denormalized    │  Validation      │
│  Pre-computed    │  Side effects    │
└──────────────────┴──────────────────┘
```

## Detailed CQRS Separation

### Query Path (Reads)
```
GraphQL Query ──▶ tv_* JSONB Table ──▶ Direct Result
                     │
                     ▼
               PostgreSQL Table
               - Generated JSONB columns
               - Pre-computed joins
               - Optimized for reads
```

### Command Path (Writes)
```
GraphQL Mutation ──▶ fn_* Function ──▶ Business Logic + Write
                        │
                        ▼
                  PostgreSQL Function
                  - Input validation
                  - Business rules
                  - tb_* table updates
                  - Transaction handling
```

## Mermaid Diagram

```mermaid
graph TD
    subgraph "GraphQL Layer"
        Q[Queries] --> RQ[Read Operations]
        M[Mutations] --> CQ[Command Operations]
    end

    subgraph "Database Layer"
        RQ --> TV[tv_* Tables<br/>JSONB Results]
        CQ --> F[fn_* Functions<br/>Business Logic]
        F --> T[tb_* Tables<br/>Normalized Data]
    end

    TV --> R[Fast Response]
    T --> R

    style Q fill:#e3f2fd
    style M fill:#fce4ec
    style TV fill:#e8f5e8
    style F fill:#fff3e0
    style T fill:#f3e5f5
```

## Component Roles

### Queries (Read Operations)
**Purpose**: Retrieve data efficiently
**Database Objects**:
- `tv_*` tables: Primary read source with generated JSONB (optimal for GraphQL)
- `v_*` views: Alternative for simple queries or small datasets

**Characteristics**:
- Optimized for speed with pre-computed JSONB
- May use denormalized data
- Read-only operations
- No side effects

### Mutations (Write Operations)
**Purpose**: Modify data with business logic
**Database Objects**:
- `fn_*` functions: Business logic functions
- `tb_*` tables: Normalized storage tables

**Characteristics**:
- ACID compliant
- Input validation
- Business rule enforcement
- May have side effects (triggers, logging)

## Example: Blog Post System

### Read Operations (Queries)
```sql
-- Primary read source: tv_* table with generated JSONB
CREATE TABLE tv_post (
    id UUID PRIMARY KEY,
    author_id UUID,
    title TEXT,
    created_at TIMESTAMPTZ,

    -- Generated JSONB with complete nested data
    data JSONB GENERATED ALWAYS AS (
        jsonb_build_object(
            '__typename', 'Post',
            'id', id,
            'title', title,
            'createdAt', created_at,
            'author', (
                SELECT jsonb_build_object(
                    'id', u.id,
                    'name', u.name,
                    'email', u.email
                )
                FROM tb_user u
                WHERE u.id = tv_post.author_id
            ),
            'comments', COALESCE(
                (
                    SELECT jsonb_agg(jsonb_build_object(
                        'id', c.id,
                        'content', c.content,
                        'author', jsonb_build_object('name', cu.name)
                    ) ORDER BY c.created_at)
                    FROM tb_comment c
                    JOIN tb_user cu ON c.user_id = cu.id
                    WHERE c.post_id = tv_post.id
                ),
                '[]'::jsonb
            )
        )
    ) STORED
);

-- Alternative: v_* view for simple cases
CREATE VIEW v_post_simple AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'authorName', u.name
    ) as data
FROM tb_post p
JOIN tb_user u ON p.author_id = u.id;
```

### Write Operations (Mutations)
```sql
-- Create post function
CREATE FUNCTION fn_create_post(
    p_title text,
    p_content text,
    p_author_id uuid
) RETURNS uuid AS $$
DECLARE
    v_post_id uuid;
BEGIN
    -- Validation
    IF NOT EXISTS (SELECT 1 FROM tb_user WHERE id = p_author_id) THEN
        RAISE EXCEPTION 'Author does not exist';
    END IF;

    -- Business logic
    INSERT INTO tb_post (title, content, author_id, created_at)
    VALUES (p_title, p_content, p_author_id, now())
    RETURNING id INTO v_post_id;

    -- Side effects (audit logging)
    INSERT INTO tb_audit (action, entity_type, entity_id, user_id)
    VALUES ('create', 'post', v_post_id, p_author_id);

    RETURN v_post_id;
END;
$$ LANGUAGE plpgsql;
```

## Performance Benefits

### Read Optimization
- **Pre-computed joins**: Views eliminate N+1 query problems
- **JSONB aggregation**: Single query returns complete object graphs
- **Materialized views**: For expensive computations
- **Indexing**: Optimized for common query patterns

### Write Optimization
- **Stored procedures**: Reduce network round trips
- **Transaction grouping**: Related changes in single transaction
- **Validation at database level**: Prevents invalid data
- **Audit trails**: Automatic logging of changes

## When to Use Each Pattern

### Use tv_* Tables (Reads) - Recommended for Production
- GraphQL APIs with complex nested data
- High-traffic applications needing sub-millisecond queries
- Large datasets (> 100k rows)
- Complex aggregations and relationships
- Real-time consistency requirements

### Use v_* Views (Reads) - For Simple Cases
- Small datasets (< 10k rows) where JOIN overhead is acceptable
- Development/prototyping (quick setup)
- Simple queries without heavy aggregations
- Cases requiring absolute freshness (no caching)

### Use fn_* Functions (Writes)
- Business logic required
- Multiple table updates
- Validation needed
- Audit trails required

### Use tb_* Tables (Direct Writes)
- Simple data insertion
- No business logic
- Bulk operations
- Migration scripts

## Consistency Considerations

### Eventual Consistency
- Some views may lag behind table updates
- Materialized views refresh on schedule
- Real-time views always current

### Transactional Consistency
- Mutations use database transactions
- All-or-nothing operations
- Rollback on errors

## Migration from Traditional ORM

### Before (Traditional)
```
User → ORM → SQL → Database → ORM → User
    ↓       ↓       ↓       ↓       ↓
   Load   Generate  Execute  Return  Map
```

### After (CQRS)
```
User → GraphQL → tv_* Table → JSONB → Response
User → GraphQL → fn_* Function → Transaction → Success
```

## Monitoring and Observability

### Read Metrics
- Query execution time
- View refresh frequency
- Cache hit rates
- Data freshness

### Write Metrics
- Transaction success rate
- Function execution time
- Validation failure rates
- Audit log volume
