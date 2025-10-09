---
← [Beginner](beginner.md) | [Learning Paths](index.md) | [Next: Frontend Developer](frontend-developer.md) →
---

# Learning Path: Backend Developer

> **For:** PostgreSQL experts and backend engineers
> **Time to complete:** 2-3 hours
> **Goal:** Master FraiseQL's database-first approach and advanced PostgreSQL features

As a backend developer, you'll appreciate FraiseQL's approach: your database is the source of truth, and PostgreSQL's power is fully leveraged. This path focuses on database optimization, CQRS patterns, and production deployment.

## Prerequisites

You should have:

- Strong PostgreSQL knowledge (views, functions, JSONB)
- Experience with database design and optimization
- Understanding of API design principles
- Python development experience

## Learning Journey

### 🏗️ Phase 1: Architecture Deep Dive (30 minutes)

Understand FraiseQL's database-centric architecture:

1. **[Architecture Overview](../core-concepts/architecture.md)** *(10 min)*

   - CQRS implementation
   - Domain-Driven Design with PostgreSQL
   - Request flow and optimization

2. **[Database Views](../core-concepts/database-views.md)** *(10 min)*

   - View patterns for APIs
   - JSONB aggregation techniques
   - Performance considerations

3. **[Query Translation](../core-concepts/query-translation.md)** *(10 min)*

   - GraphQL to SQL mapping
   - Query optimization strategies
   - Index utilization

### 💾 Phase 2: Database Patterns (45 minutes)

Master FraiseQL's database patterns:

4. **[Database API Patterns](../advanced/database-api-patterns.md)** *(15 min)*

   - View design principles
   - Denormalization strategies
   - Composite views for performance

5. **[PostgreSQL Functions](../mutations/postgresql-functions.md)** *(15 min)*

   - Function-based mutations
   - Transaction management
   - Business logic in database

6. **[CQRS Implementation](../advanced/cqrs.md)** *(15 min)*

   - Command vs Query separation
   - Event sourcing patterns
   - Bounded contexts

### ⚡ Phase 3: Performance & Optimization (45 minutes)

Optimize for production workloads:

7. **[Performance Guide](../advanced/performance.md)** *(15 min)*

   - Query optimization
   - Index strategies
   - EXPLAIN ANALYZE usage

8. **[Lazy Caching](../advanced/lazy-caching.md)** *(15 min)*

   - Database-native caching
   - Cache invalidation strategies
   - Version tracking

9. **[TurboRouter](../advanced/turbo-router.md)** *(15 min)*

   - Bypass GraphQL parsing
   - Direct SQL execution
   - 50-1000x performance gains

### 🔒 Phase 4: Production Deployment (30 minutes)

Deploy with confidence:

10. **[Security Best Practices](../advanced/security.md)** *(10 min)*

    - SQL injection prevention
    - Row-level security
    - Field authorization

11. **[Authentication](../advanced/authentication.md)** *(10 min)*

    - JWT implementation
    - Session management
    - Role-based access

12. **[Production Readiness](../advanced/production-readiness.md)** *(10 min)*

    - Health checks
    - Monitoring
    - Deployment strategies

## Advanced Database Techniques

### Optimized View Patterns

#### Pattern 1: Aggregated JSONB Views
```sql
-- Efficient aggregation with filtering
CREATE VIEW v_user_with_stats AS
SELECT
    u.id,
    u.email,
    u.created_at,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'name', u.name,
        'stats', jsonb_build_object(
            'post_count', COUNT(DISTINCT p.id),
            'comment_count', COUNT(DISTINCT c.id),
            'last_activity', MAX(GREATEST(
                p.created_at,
                c.created_at
            ))
        ),
        'recent_posts', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', rp.id,
                    'title', rp.title,
                    'created_at', rp.created_at
                )
                ORDER BY rp.created_at DESC
            )
            FROM (
                SELECT * FROM posts
                WHERE user_id = u.id
                ORDER BY created_at DESC
                LIMIT 5
            ) rp
        )
    ) AS data
FROM users u
LEFT JOIN posts p ON p.user_id = u.id
LEFT JOIN comments c ON c.user_id = u.id
GROUP BY u.id;

-- Create indexes for performance
CREATE INDEX idx_posts_user_created
ON posts(user_id, created_at DESC);

CREATE INDEX idx_comments_user
ON comments(user_id);
```

#### Pattern 2: Materialized Views for Heavy Queries
```sql
-- Materialized view for expensive aggregations
CREATE MATERIALIZED VIEW mv_dashboard_stats AS
SELECT
    DATE_TRUNC('day', created_at) AS date,
    jsonb_build_object(
        'total_users', COUNT(DISTINCT user_id),
        'total_posts', COUNT(DISTINCT post_id),
        'total_comments', COUNT(*),
        'avg_comments_per_post',
            AVG(comments_per_post)::numeric(10,2)
    ) AS data
FROM (
    -- Complex aggregation logic
) stats
GROUP BY DATE_TRUNC('day', created_at);

-- Refresh strategy
CREATE OR REPLACE FUNCTION refresh_dashboard_stats()
RETURNS void AS $$
BEGIN
    REFRESH MATERIALIZED VIEW CONCURRENTLY mv_dashboard_stats;
END;
$$ LANGUAGE plpgsql;

-- Schedule refresh (using pg_cron or similar)
SELECT cron.schedule(
    'refresh-dashboard-stats',
    '0 * * * *',  -- Every hour
    'SELECT refresh_dashboard_stats();'
);
```

### Advanced Mutation Patterns

#### Pattern 1: Domain Functions with Validation
```sql
CREATE OR REPLACE FUNCTION fn_create_post(
    p_user_id UUID,
    p_title TEXT,
    p_content TEXT,
    p_tags TEXT[] DEFAULT '{}'
) RETURNS UUID AS $$
DECLARE
    v_post_id UUID;
    v_user_exists BOOLEAN;
BEGIN
    -- Validate user exists and is active
    SELECT EXISTS(
        SELECT 1 FROM users
        WHERE id = p_user_id
        AND status = 'active'
    ) INTO v_user_exists;

    IF NOT v_user_exists THEN
        RAISE EXCEPTION 'User not found or inactive'
            USING ERRCODE = 'P0001';
    END IF;

    -- Validate input
    IF LENGTH(p_title) < 3 THEN
        RAISE EXCEPTION 'Title too short'
            USING ERRCODE = 'P0002';
    END IF;

    -- Create post with audit trail
    INSERT INTO posts (
        user_id, title, content, tags,
        created_at, updated_at
    ) VALUES (
        p_user_id, p_title, p_content, p_tags,
        NOW(), NOW()
    ) RETURNING id INTO v_post_id;

    -- Log event for event sourcing
    INSERT INTO events (
        aggregate_id,
        aggregate_type,
        event_type,
        payload
    ) VALUES (
        v_post_id,
        'post',
        'post_created',
        jsonb_build_object(
            'user_id', p_user_id,
            'title', p_title,
            'tags', p_tags
        )
    );

    -- Invalidate caches
    UPDATE cache_versions
    SET version = version + 1
    WHERE context = 'posts';

    RETURN v_post_id;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
```

#### Pattern 2: Batch Operations
```sql
CREATE OR REPLACE FUNCTION fn_batch_update_posts(
    p_updates JSONB[]
) RETURNS JSONB AS $$
DECLARE
    v_update JSONB;
    v_results JSONB[] := '{}';
    v_result JSONB;
BEGIN
    -- Process each update in transaction
    FOREACH v_update IN ARRAY p_updates
    LOOP
        BEGIN
            UPDATE posts
            SET
                title = COALESCE(
                    v_update->>'title',
                    title
                ),
                content = COALESCE(
                    v_update->>'content',
                    content
                ),
                updated_at = NOW()
            WHERE id = (v_update->>'id')::UUID
            RETURNING jsonb_build_object(
                'id', id,
                'success', true
            ) INTO v_result;

            v_results := v_results || v_result;
        EXCEPTION WHEN OTHERS THEN
            v_results := v_results ||
                jsonb_build_object(
                    'id', v_update->>'id',
                    'success', false,
                    'error', SQLERRM
                );
        END;
    END LOOP;

    RETURN jsonb_build_object(
        'results', v_results,
        'total', array_length(p_updates, 1),
        'successful', (
            SELECT COUNT(*)
            FROM unnest(v_results) r
            WHERE r->>'success' = 'true'
        )
    );
END;
$$ LANGUAGE plpgsql;
```

### Performance Optimization Strategies

#### 1. Strategic Indexing
```sql
-- Partial indexes for common filters
CREATE INDEX idx_posts_published
ON posts(created_at DESC)
WHERE status = 'published';

-- JSONB GIN indexes for search
CREATE INDEX idx_posts_tags
ON posts USING GIN (tags);

-- Composite indexes for joins
CREATE INDEX idx_comments_post_user
ON comments(post_id, user_id);
```

#### 2. Query Analysis
```sql
-- Analyze view performance
EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON)
SELECT * FROM v_user_with_stats
WHERE id = 'some-uuid';

-- Monitor slow queries
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

SELECT
    query,
    mean_exec_time,
    calls,
    total_exec_time
FROM pg_stat_statements
WHERE query LIKE '%v_%'
ORDER BY mean_exec_time DESC
LIMIT 10;
```

## Production Checklist

### Database Setup

- [ ] Connection pooling configured (pgBouncer/pgPool)
- [ ] Read replicas for scaling queries
- [ ] Backup strategy implemented
- [ ] Monitoring and alerting setup
- [ ] Query performance baselines established

### FraiseQL Configuration

- [ ] Lazy caching enabled for hot paths
- [ ] TurboRouter configured for known queries
- [ ] Error handling and logging configured
- [ ] Health check endpoints implemented
- [ ] Rate limiting configured

### Security

- [ ] Row-level security policies defined
- [ ] Field-level authorization implemented
- [ ] SQL injection prevention verified
- [ ] Authentication system tested
- [ ] API rate limiting enabled

## Common Backend Patterns

### Multi-tenancy
```python
@fraiseql.query
async def tenant_data(info) -> TenantData:
    repo = info.context["repo"]
    tenant_id = info.context["tenant_id"]

    # RLS automatically filters by tenant
    return await repo.find_one(
        "v_tenant_dashboard",
        where={"tenant_id": tenant_id}
    )
```

### Event Sourcing
```python
@fraiseql.mutation
async def execute_command(
    info,
    command: Command
) -> CommandResult:
    repo = info.context["repo"]

    # Store command
    event_id = await repo.call_function(
        "fn_store_event",
        p_aggregate_id=command.aggregate_id,
        p_event_type=command.type,
        p_payload=command.payload
    )

    # Process projections
    await repo.call_function(
        "fn_update_projections",
        p_event_id=event_id
    )

    return CommandResult(
        success=True,
        event_id=event_id
    )
```

## Next Steps

### Continue Learning

- **[Frontend Developer Path](frontend-developer.md)** - API consumption patterns
- **[Migration Path](migrating.md)** - Migrating from other frameworks

### Advanced Topics

- **[Event Sourcing](../advanced/event-sourcing.md)** - Event-driven architecture
- **[Multi-tenancy](../advanced/multi-tenancy.md)** - Tenant isolation strategies
- **[Bounded Contexts](../advanced/bounded-contexts.md)** - Domain boundaries

### References

- **[PostgreSQL Documentation](https://www.postgresql.org/docs/)** - Official PostgreSQL docs
- **[EXPLAIN Visualizer](https://explain.depesz.com/)** - Query plan analysis
- **[pgBadger](https://pgbadger.darold.net/)** - Log analysis tool

## Tips for Backend Success

💡 **Think in sets** - PostgreSQL excels at set operations
💡 **Use EXPLAIN** - Always analyze query plans
💡 **Index strategically** - Not every column needs an index
💡 **Monitor everything** - Track query performance over time
💡 **Test at scale** - Use realistic data volumes

Congratulations! You now have the knowledge to build high-performance, scalable GraphQL APIs with FraiseQL and PostgreSQL.
