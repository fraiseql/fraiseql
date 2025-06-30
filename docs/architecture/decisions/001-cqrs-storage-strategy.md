# ADR-001: CQRS Storage Strategy

## Status
Accepted

## Context
FraiseQL needs to balance schema flexibility for GraphQL with data integrity and query performance. Pure JSONB storage provides flexibility but lacks referential integrity, while traditional normalized tables are rigid but provide strong consistency guarantees.

## Decision
We will implement a CQRS (Command Query Responsibility Segregation) pattern with:
- **Write side**: Normalized tables with `tb_` prefix, full foreign key constraints
- **Read side**: Views (`v_`), materialized views (`mv_`), and projection tables (`tv_`) with JSONB
- **Synchronization**: Mutation functions directly call refresh/sync functions (no LISTEN/NOTIFY)

## Consequences

### Positive
- **Data integrity**: Foreign keys ensure referential integrity on writes
- **Query flexibility**: JSONB views optimize for GraphQL query patterns
- **Performance**: Materialized views and projections enable fast reads
- **Best of both worlds**: ACID compliance for writes, flexibility for reads
- **Self-contained**: Each mutation handles its own projection updates
- **Predictable**: No async events, everything happens in the same transaction

### Negative
- **Complexity**: More moving parts than a simple CRUD system
- **Storage overhead**: Data is stored multiple times
- **Maintenance**: Views and projections need to be kept in sync
- **Transaction size**: Large updates might create long transactions

### Mitigation
- Keep refresh functions efficient and well-indexed
- Use partial refresh strategies where possible
- Monitor transaction duration and lock contention
- Consider batch updates for bulk operations

## Implementation

### Write Side (Commands)
```sql
-- Normalized tables with foreign keys
CREATE TABLE tb_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE tb_posts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES tb_users(id),
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### Read Side (Queries)
```sql
-- View with JSONB for GraphQL queries
CREATE VIEW v_users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,
        'createdAt', created_at
    ) as data
FROM tb_users;

-- Materialized view for performance
CREATE MATERIALIZED VIEW mv_user_stats AS
SELECT
    u.id,
    jsonb_build_object(
        'userId', u.id,
        'postCount', COUNT(p.id),
        'lastPostAt', MAX(p.created_at)
    ) as data
FROM tb_users u
LEFT JOIN tb_posts p ON u.id = p.user_id
GROUP BY u.id;

-- Projection table for complex aggregations
CREATE TABLE tv_user_activity (
    user_id UUID PRIMARY KEY,
    data JSONB NOT NULL,
    last_updated TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### Mutation Functions
```sql
-- Create user mutation function
CREATE FUNCTION fn_create_user(input JSONB) RETURNS JSONB AS $$
DECLARE
    user_id UUID;
    result JSONB;
BEGIN
    -- Insert into normalized table
    INSERT INTO tb_users (email, name)
    VALUES (input->>'email', input->>'name')
    RETURNING id INTO user_id;

    -- Refresh projections
    PERFORM fn_refresh_user_activity(user_id);
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_user_stats;

    -- Return result
    SELECT data INTO result FROM v_users WHERE id = user_id;
    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- Refresh function for projection table
CREATE FUNCTION fn_refresh_user_activity(p_user_id UUID) RETURNS void AS $$
BEGIN
    INSERT INTO tv_user_activity (user_id, data, last_updated)
    SELECT
        u.id,
        jsonb_build_object(
            'totalPosts', COUNT(p.id),
            'recentPosts', jsonb_agg(
                jsonb_build_object('id', p.id, 'title', p.title)
                ORDER BY p.created_at DESC
            ) FILTER (WHERE p.created_at > NOW() - INTERVAL '7 days')
        ),
        NOW()
    FROM tb_users u
    LEFT JOIN tb_posts p ON u.id = p.user_id
    WHERE u.id = p_user_id
    GROUP BY u.id
    ON CONFLICT (user_id) DO UPDATE
    SET data = EXCLUDED.data,
        last_updated = EXCLUDED.last_updated;
END;
$$ LANGUAGE plpgsql;
```
