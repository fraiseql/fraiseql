# Comparing FraiseQL to Other GraphQL Solutions

FraiseQL offers a unique approach to GraphQL APIs by leveraging PostgreSQL as the source of truth. Instead of complex ORM mappings or manual resolvers, FraiseQL generates optimal SQL queries directly from your database schema. This PostgreSQL-first philosophy delivers sub-millisecond query performance with minimal setup.

## Quick Decision Guide

### Choose FraiseQL when you:
- ✅ Have an existing PostgreSQL database
- ✅ Want sub-millisecond query performance  
- ✅ Need type-safe APIs without manual schema writing
- ✅ Prefer SQL views over ORM abstractions
- ✅ Value simplicity and maintainability
- ✅ Build multi-tenant SaaS applications
- ✅ Want caching built into the database

### Consider alternatives when you:
- ❌ Need to support multiple database types
- ❌ Require complex real-time subscriptions across many clients
- ❌ Have primarily non-relational data sources
- ❌ Build social networks with highly dynamic feeds
- ❌ Need federation across multiple GraphQL services

## Comparison Matrix

| Feature | FraiseQL | Hasura | PostGraphile | Strawberry | Prisma + Nexus |
|---------|----------|---------|--------------|------------|----------------|
| **Setup Time** | 5 minutes | 15 minutes | 10 minutes | 30 minutes | 45 minutes |
| **Database First** | ✅ Yes | ✅ Yes | ✅ Yes | ❌ Code first | ❌ Code first |
| **Type Safety** | ✅ Python types | ⚠️ Limited | ✅ TypeScript | ✅ Python types | ✅ TypeScript |
| **Performance** | Sub-ms (cached) | 10-50ms | 5-30ms | 20-100ms | 30-150ms |
| **Learning Curve** | Low | Medium | Medium | High | High |
| **PostgreSQL Native** | ✅ Full | ✅ Full | ✅ Full | ⚠️ Via ORM | ⚠️ Via ORM |
| **Caching Strategy** | DB-native | Redis/Memory | Memory | Manual | Manual |
| **Multi-tenant** | ✅ Built-in | ⚠️ Manual | ⚠️ Manual | ❌ DIY | ❌ DIY |

## Key Differentiators

### 1. PostgreSQL-First Philosophy

**FraiseQL**: Everything lives in PostgreSQL - your data, views, functions, and even cached responses. One source of truth.

**Others**: Typically layer abstractions on top, requiring you to think in both database and application terms.

### 2. Performance Through Storage

**FraiseQL**: Aggressively trades storage (7-10x) for massive performance gains (50-100x). Pre-computed table views and cached GraphQL responses live in the database.

**Others**: Focus on query optimization and external caching layers (Redis, memory).

### 3. Simplicity at Scale

**FraiseQL**: 
```python
# Complete API in minutes
from fraiseql import FraiseQL

app = FraiseQL(database_url="postgresql://...")
# That's it - your API is ready
```

**Others**: Often require extensive configuration, resolver definitions, and schema management.

### 4. Developer Experience

| Task | FraiseQL | Others |
|------|----------|--------|
| Add a field | Update SQL view | Update schema + resolver + types |
| Optimize query | Create indexed view | Add DataLoader + cache logic |
| Add pagination | SQL LIMIT/OFFSET | Implement cursor logic |
| Handle N+1 | Table views handle it | Configure DataLoader |

## Performance Comparison

*Based on typical e-commerce queries with 3-level nesting*

| Operation | FraiseQL (cached) | FraiseQL (fresh) | Hasura | PostGraphile | Strawberry |
|-----------|-------------------|------------------|---------|--------------|------------|
| Single entity | < 1ms | 5ms | 15ms | 10ms | 50ms |
| List (100 items) | < 1ms | 20ms | 60ms | 50ms | 300ms |
| Complex aggregate | < 1ms | 15ms | 50ms | 40ms | 150ms |
| Mutation + query | 5ms | 10ms | 30ms | 25ms | 100ms |

## Real-World Use Cases

### When FraiseQL Shines

**Multi-tenant SaaS**: Built-in tenant isolation and per-tenant caching
```sql
-- Tenant-aware view with proper structure
CREATE VIEW v_tenant_post AS
SELECT 
    p.tenant_id,
    jsonb_build_object(
        'id', p.id,
        'title', p.title,
        'content', p.content,
        'author', (
            SELECT jsonb_build_object('id', u.id, 'name', u.name)
            FROM tb_users u 
            WHERE u.id = p.author_id
        ),
        'comments', (
            SELECT jsonb_agg(
                jsonb_build_object('id', c.id, 'text', c.text)
            )
            FROM tb_comments c 
            WHERE c.post_id = p.id
        )
    ) as data
FROM tb_posts p;
```

**Admin Dashboards**: Complex queries with aggregations
```sql
-- Pre-computed analytics
CREATE VIEW v_dashboard_metrics AS
SELECT jsonb_build_object(
    'total_revenue', (SELECT SUM(amount) FROM tb_orders WHERE created_at > NOW() - '30 days'::interval),
    'active_users', (SELECT COUNT(DISTINCT user_id) FROM tb_sessions WHERE last_seen > NOW() - '7 days'::interval),
    'top_products', (SELECT jsonb_agg(p.*) FROM (
        SELECT product_id, COUNT(*) as sales 
        FROM tb_order_items 
        GROUP BY product_id 
        ORDER BY sales DESC 
        LIMIT 10
    ) p)
) as data;
```

**API Gateways**: Predictable query patterns with extreme performance needs

### When to Use Alternatives

**Hasura**: When you need a UI for non-developers to manage the API

**PostGraphile**: When you have an existing PostgreSQL schema with complex permissions

**Strawberry/Graphene**: When you need deep integration with existing Python business logic

**Prisma + Nexus**: When your team prefers TypeScript and code-first development

## Migration Paths

### From Hasura to FraiseQL

1. Export your PostgreSQL schema
2. Create views for your main queries
3. Install FraiseQL: `pip install fraiseql`
4. Point to your database
5. Gradually migrate endpoints

### From ORM-based (Strawberry/Graphene)

1. Keep your existing models
2. Create PostgreSQL views matching your GraphQL queries
3. Replace resolvers with FraiseQL views
4. Remove ORM overhead progressively

### From PostGraphile

1. Most similar migration path
2. Convert computed columns to view columns
3. Replace PostGraphile plugins with SQL functions
4. Benefit from Python ecosystem

## Total Cost of Ownership

| Aspect | FraiseQL | Traditional Stack |
|--------|----------|-------------------|
| **Database Storage** | $100/month (700GB) | $15/month (100GB) |
| **Cache Infrastructure** | $0 (in-database) | $200/month (Redis) |
| **Application Servers** | 3 servers ($150) | 10 servers ($500) |
| **Development Time** | 1 developer | 2-3 developers |
| **Maintenance** | Minimal | Significant |
| **Total Monthly** | ~$250 | ~$715+ |

## Quick Start Comparison

### FraiseQL
```python
# 1. Install
pip install fraiseql

# 2. Create view
"""
CREATE VIEW v_user AS
SELECT jsonb_build_object(
    'users', (SELECT jsonb_agg(u.*) FROM tb_users u)
) as data;
"""

# 3. Run
from fraiseql import FraiseQL
app = FraiseQL(database_url="postgresql://...")
# Done! Full GraphQL API ready
```

### Hasura
```bash
# 1. Run Hasura
docker run -p 8080:8080 hasura/graphql-engine

# 2. Connect database via UI
# 3. Track tables via UI
# 4. Set permissions via UI
# 5. Configure relationships via UI
```

### PostGraphile
```javascript
// 1. Install
npm install postgraphile

// 2. Configure
const express = require('express');
const { postgraphile } = require('postgraphile');

app.use(postgraphile(
  DATABASE_URL,
  'public',
  {
    watchPg: true,
    graphiql: true,
    enhanceGraphiql: true,
  }
));
```

## Making the Decision

### Choose FraiseQL for:
- **Performance-critical applications** where every millisecond counts
- **PostgreSQL-heavy projects** that embrace database features
- **Rapid prototyping** with existing databases
- **Multi-tenant SaaS** with complex isolation requirements
- **Teams comfortable with SQL** who want to leverage that knowledge

### Choose Hasura for:
- **No-code/low-code environments** where non-developers manage the API
- **Real-time subscriptions** as the primary feature
- **Multi-database setups** beyond PostgreSQL

### Choose PostGraphile for:
- **Existing PostGraphile projects** (similar philosophy to FraiseQL)
- **Complex PostgreSQL RLS** requirements
- **Teams preferring Node.js** ecosystem

### Choose Strawberry/Graphene for:
- **Complex business logic** in Python that can't be expressed in SQL
- **Existing Django/Flask applications** with deep integration needs
- **Machine learning pipelines** integrated with GraphQL

## Summary

FraiseQL represents a fundamental shift in how we think about GraphQL APIs. Instead of treating the database as a dumb store and rebuilding query logic in application code, FraiseQL embraces PostgreSQL's power. The result is dramatically simpler code, better performance, and lower operational costs.

The trade-off is clear: use more storage (cheap) to gain massive performance improvements (valuable). For most read-heavy applications, this is a winning formula.

Ready to see the difference? Check out our [Getting Started Guide](/getting-started/quickstart) to build your first FraiseQL API in under 5 minutes, or dive into the [detailed technical comparison](alternatives) for an in-depth analysis.