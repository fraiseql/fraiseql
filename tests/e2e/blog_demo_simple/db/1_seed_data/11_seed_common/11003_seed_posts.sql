-- Common seed data for posts
-- Always loaded for all environments

-- Create sample blog posts
INSERT INTO tb_post (
    pk_post,
    identifier,
    fk_author,
    title,
    content,
    excerpt,
    status,
    featured,
    published_at,
    seo_metadata,
    created_at
) VALUES
(
    'post1111-1111-1111-1111-111111111111'::UUID,
    'getting-started-with-graphql',
    '22222222-2222-2222-2222-222222222222'::UUID, -- johndoe
    'Getting Started with GraphQL',
    'GraphQL is a query language for APIs and a runtime for fulfilling those queries with your existing data. In this comprehensive guide, we''ll explore how to get started with GraphQL and build your first API.

## What is GraphQL?

GraphQL provides a complete and understandable description of the data in your API, gives clients the power to ask for exactly what they need and nothing more, makes it easier to evolve APIs over time, and enables powerful developer tools.

## Key Benefits

- **Single endpoint**: Unlike REST APIs with multiple endpoints, GraphQL uses a single endpoint
- **Flexible queries**: Clients can request exactly the data they need
- **Strong type system**: GraphQL APIs are organized in terms of types and fields
- **Real-time subscriptions**: Built-in support for real-time updates

## Basic Example

Here''s a simple GraphQL query:

```graphql
query {
  user(id: "1") {
    name
    email
    posts {
      title
      createdAt
    }
  }
}
```

This query fetches a user by ID and includes their name, email, and a list of their posts with titles and creation dates.

## Next Steps

In the next post, we''ll dive deeper into GraphQL schema design and explore more advanced features like mutations and subscriptions.',
    'Learn the fundamentals of GraphQL and how to build your first API with this comprehensive beginner''s guide.',
    'published',
    true,
    '2024-01-15 10:00:00+00',
    jsonb_build_object(
        'meta_title', 'Getting Started with GraphQL - Complete Beginner Guide',
        'meta_description', 'Learn GraphQL fundamentals, key benefits, and build your first API. Perfect for developers new to GraphQL.',
        'keywords', ARRAY['graphql', 'api', 'tutorial', 'beginner']
    ),
    '2024-01-15 09:30:00+00'
),
(
    'post2222-2222-2222-2222-222222222222'::UUID,
    'advanced-postgresql-indexing',
    '33333333-3333-3333-3333-333333333333'::UUID, -- janesmit
    'Advanced PostgreSQL Indexing Strategies',
    'Proper indexing is crucial for PostgreSQL performance. This guide covers advanced indexing strategies that will help you optimize your database queries.

## Types of Indexes

PostgreSQL supports several index types, each optimized for different use cases:

### B-tree Indexes
The most common index type, perfect for equality and range queries:

```sql
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_posts_created_at ON posts(created_at);
```

### GIN Indexes
Great for complex data types like JSONB, arrays, and full-text search:

```sql
CREATE INDEX idx_posts_tags_gin ON posts USING gin(tags);
CREATE INDEX idx_posts_content_gin ON posts USING gin(to_tsvector(''english'', content));
```

### Partial Indexes
Indexes that only cover rows meeting specific conditions:

```sql
CREATE INDEX idx_active_users ON users(email) WHERE is_active = true;
```

## Performance Tips

1. **Monitor index usage**: Use `pg_stat_user_indexes` to track which indexes are being used
2. **Avoid over-indexing**: Every index has a cost for writes
3. **Consider composite indexes**: Multiple columns in the right order can be very effective
4. **Use EXPLAIN ANALYZE**: Always analyze your query execution plans

## Maintenance

Regular maintenance is essential:

```sql
-- Reindex to rebuild potentially bloated indexes
REINDEX INDEX CONCURRENTLY idx_users_email;

-- Analyze tables to update statistics
ANALYZE users;
```

Proper indexing can make the difference between millisecond and second-long queries. Take time to understand your query patterns and index accordingly.',
    'Master PostgreSQL indexing with advanced strategies for optimal database performance. Covers B-tree, GIN, partial indexes and maintenance tips.',
    'published',
    false,
    '2024-01-20 14:30:00+00',
    jsonb_build_object(
        'meta_title', 'Advanced PostgreSQL Indexing Strategies for Performance',
        'meta_description', 'Learn advanced PostgreSQL indexing techniques including B-tree, GIN, and partial indexes for optimal database performance.',
        'keywords', ARRAY['postgresql', 'indexing', 'performance', 'database', 'optimization']
    ),
    '2024-01-20 13:45:00+00'
),
(
    'post3333-3333-3333-3333-333333333333'::UUID,
    'draft-post-web-components',
    '22222222-2222-2222-2222-222222222222'::UUID, -- johndoe
    'Building Reusable Web Components',
    'This is a draft post about building reusable web components. Still working on the content...',
    'Learn how to create reusable web components that work across different frameworks.',
    'draft',
    false,
    NULL,
    jsonb_build_object(),
    '2024-01-25 16:00:00+00'
);

-- Set the sequence to avoid conflicts
SELECT setval('tb_post_id_seq', 1000, true);
