# Database-API Architecture Patterns

One of FraiseQL's key strengths is the ability to decouple your database schema design from your GraphQL API representation. This allows you to design properly normalized databases while providing client-optimized API structures.

## Core Philosophy

**Use PostgreSQL views as the transformation layer** between your normalized database schema and your GraphQL API:

```
Database Tables (Normalized) → PostgreSQL Views (API-Optimized) → GraphQL Schema
```

This approach provides:
- **Database design freedom** - Normalize for data integrity and performance
- **API design flexibility** - Structure for client consumption patterns
- **Independent evolution** - Change database or API without affecting the other
- **Performance optimization** - Views can be optimized for specific access patterns

## Common Architectural Patterns

### Pattern 1: Flattening Normalized Data

Often you'll have data spread across multiple normalized tables that you want to present as a single GraphQL type.

**Database Schema (Normalized)**:
```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email VARCHAR(255) UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE user_profiles (
    user_id UUID REFERENCES users(id),
    first_name VARCHAR(255),
    last_name VARCHAR(255),
    bio TEXT,
    avatar_url TEXT
);

CREATE TABLE user_preferences (
    user_id UUID REFERENCES users(id),
    theme VARCHAR(50) DEFAULT 'light',
    language VARCHAR(10) DEFAULT 'en',
    notifications_enabled BOOLEAN DEFAULT true
);
```

**API-Optimized View**:
```sql
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'first_name', COALESCE(up.first_name, ''),
        'last_name', COALESCE(up.last_name, ''),
        'display_name', COALESCE(
            NULLIF(TRIM(up.first_name || ' ' || up.last_name), ''),
            SPLIT_PART(u.email, '@', 1)
        ),
        'bio', up.bio,
        'avatar_url', up.avatar_url,
        'theme', COALESCE(upr.theme, 'light'),
        'language', COALESCE(upr.language, 'en'),
        'notifications_enabled', COALESCE(upr.notifications_enabled, true),
        'created_at', u.created_at
    ) as data
FROM users u
LEFT JOIN user_profiles up ON u.id = up.user_id
LEFT JOIN user_preferences upr ON u.id = upr.user_id;
```

**GraphQL Type**:
```python
@fraiseql.type
class User:
    """A user with flattened profile and preference data."""
    id: UUID
    email: str
    first_name: str
    last_name: str
    display_name: str  # Computed field
    bio: Optional[str]
    avatar_url: Optional[str]
    theme: str
    language: str
    notifications_enabled: bool
    created_at: datetime
```

### Pattern 2: Aggregating Related Data

Compute metrics and aggregate related data in views to avoid N+1 queries.

**Database Schema**:
```sql
CREATE TABLE products (
    id UUID PRIMARY KEY,
    name VARCHAR(255),
    description TEXT,
    category_id UUID REFERENCES categories(id),
    current_price DECIMAL(10,2)
);

CREATE TABLE product_reviews (
    id UUID PRIMARY KEY,
    product_id UUID REFERENCES products(id),
    rating INTEGER CHECK (rating BETWEEN 1 AND 5),
    comment TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE order_items (
    order_id UUID REFERENCES orders(id),
    product_id UUID REFERENCES products(id),
    quantity INTEGER,
    price_at_time DECIMAL(10,2)
);
```

**API-Optimized View with Aggregations**:
```sql
CREATE VIEW v_products AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'description', p.description,
        'current_price', p.current_price,
        'category_name', c.name,
        'category_path', c.full_path,
        -- Aggregated review data
        'average_rating', COALESCE(ROUND(AVG(pr.rating), 2), 0),
        'review_count', COUNT(DISTINCT pr.id),
        'rating_distribution', jsonb_build_object(
            '5', COUNT(CASE WHEN pr.rating = 5 THEN 1 END),
            '4', COUNT(CASE WHEN pr.rating = 4 THEN 1 END),
            '3', COUNT(CASE WHEN pr.rating = 3 THEN 1 END),
            '2', COUNT(CASE WHEN pr.rating = 2 THEN 1 END),
            '1', COUNT(CASE WHEN pr.rating = 1 THEN 1 END)
        ),
        -- Sales data
        'total_sold', COALESCE(SUM(oi.quantity), 0),
        'revenue', COALESCE(SUM(oi.quantity * oi.price_at_time), 0),
        -- Recent reviews
        'recent_reviews', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'rating', rating,
                    'comment', comment,
                    'created_at', created_at
                )
                ORDER BY created_at DESC
            )
            FROM (
                SELECT rating, comment, created_at
                FROM product_reviews
                WHERE product_id = p.id
                ORDER BY created_at DESC
                LIMIT 3
            ) recent
        )
    ) as data
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN product_reviews pr ON p.id = pr.product_id
LEFT JOIN order_items oi ON p.id = oi.product_id
GROUP BY p.id, p.name, p.description, p.current_price, c.name, c.full_path;
```

### Pattern 3: Hierarchical Data Transformation

Transform hierarchical database structures into client-friendly formats.

**Database Schema**:
```sql
CREATE TABLE categories (
    id UUID PRIMARY KEY,
    name VARCHAR(255),
    parent_id UUID REFERENCES categories(id),
    sort_order INTEGER DEFAULT 0
);
```

**View with Path Generation**:
```sql
CREATE VIEW v_categories AS
WITH RECURSIVE category_paths AS (
    -- Base case: root categories
    SELECT
        id,
        name,
        parent_id,
        ARRAY[name] as path,
        name as path_string,
        0 as level,
        sort_order
    FROM categories
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case: child categories
    SELECT
        c.id,
        c.name,
        c.parent_id,
        cp.path || c.name,
        cp.path_string || ' > ' || c.name,
        cp.level + 1,
        c.sort_order
    FROM categories c
    JOIN category_paths cp ON c.parent_id = cp.id
)
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'path', path,
        'path_string', path_string,
        'level', level,
        'is_leaf', NOT EXISTS(
            SELECT 1 FROM categories
            WHERE parent_id = category_paths.id
        ),
        'child_count', (
            SELECT COUNT(*) FROM categories
            WHERE parent_id = category_paths.id
        ),
        'product_count', (
            SELECT COUNT(*) FROM products
            WHERE category_id = category_paths.id
        )
    ) as data
FROM category_paths
ORDER BY level, sort_order, name;
```

### Pattern 4: Temporal Data and History

Handle time-sensitive data and historical snapshots.

**Database Schema**:
```sql
CREATE TABLE orders (
    id UUID PRIMARY KEY,
    user_id UUID REFERENCES users(id),
    status VARCHAR(50),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE order_items (
    order_id UUID REFERENCES orders(id),
    product_id UUID REFERENCES products(id),
    quantity INTEGER,
    price_at_time DECIMAL(10,2)  -- Historical pricing
);

CREATE TABLE price_history (
    product_id UUID REFERENCES products(id),
    price DECIMAL(10,2),
    effective_date TIMESTAMP
);
```

**View with Historical Context**:
```sql
CREATE VIEW v_orders AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id,
        'status', o.status,
        'created_at', o.created_at,
        'customer', (
            SELECT jsonb_build_object('id', id, 'name', display_name)
            FROM v_users WHERE id = o.user_id
        ),
        'items', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'product_id', oi.product_id,
                    'product_name', p.name,
                    'quantity', oi.quantity,
                    'price_at_time', oi.price_at_time,
                    'current_price', p.current_price,
                    'price_change_pct', ROUND(
                        ((p.current_price - oi.price_at_time) / oi.price_at_time) * 100, 2
                    ),
                    'line_total', oi.quantity * oi.price_at_time
                )
            )
            FROM order_items oi
            JOIN products p ON oi.product_id = p.id
            WHERE oi.order_id = o.id
        ),
        'totals', (
            SELECT jsonb_build_object(
                'subtotal', SUM(oi.quantity * oi.price_at_time),
                'item_count', SUM(oi.quantity),
                'unique_products', COUNT(DISTINCT oi.product_id)
            )
            FROM order_items oi
            WHERE oi.order_id = o.id
        )
    ) as data
FROM orders o;
```

## Performance Optimization Patterns

### Materialized Views for Heavy Computations

For expensive aggregations that don't need real-time updates:

```sql
CREATE MATERIALIZED VIEW v_user_analytics AS
SELECT
    u.id,
    jsonb_build_object(
        'user_id', u.id,
        'registration_date', u.created_at,
        'days_since_registration', EXTRACT(days FROM NOW() - u.created_at),
        'total_orders', COUNT(DISTINCT o.id),
        'total_spent', COALESCE(SUM(
            (oi.quantity * oi.price_at_time)
        ), 0),
        'average_order_value', COALESCE(AVG(
            (SELECT SUM(quantity * price_at_time)
             FROM order_items WHERE order_id = o.id)
        ), 0),
        'favorite_category', (
            SELECT c.name
            FROM categories c
            JOIN products p ON c.id = p.category_id
            JOIN order_items oi ON p.id = oi.product_id
            JOIN orders ord ON oi.order_id = ord.id
            WHERE ord.user_id = u.id
            GROUP BY c.id, c.name
            ORDER BY SUM(oi.quantity) DESC
            LIMIT 1
        ),
        'last_order_date', MAX(o.created_at),
        'order_frequency_days', CASE
            WHEN COUNT(o.id) > 1 THEN
                EXTRACT(days FROM MAX(o.created_at) - MIN(o.created_at)) /
                NULLIF(COUNT(o.id) - 1, 0)
            ELSE NULL
        END
    ) as data
FROM users u
LEFT JOIN orders o ON u.id = o.user_id
LEFT JOIN order_items oi ON o.id = oi.order_id
GROUP BY u.id, u.created_at;

-- Create unique index for concurrent refresh
CREATE UNIQUE INDEX idx_user_analytics_id ON v_user_analytics(id);

-- Schedule refresh
SELECT cron.schedule('refresh-user-analytics', '0 2 * * *',
    'REFRESH MATERIALIZED VIEW CONCURRENTLY v_user_analytics;');
```

### Selective Views for Different Use Cases

Create different views optimized for different access patterns:

```sql
-- Lightweight view for product listings
CREATE VIEW v_products_list AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'name', name,
        'price', current_price,
        'image_url', primary_image_url,
        'rating', average_rating,
        'review_count', review_count,
        'in_stock', inventory_count > 0
    ) as data
FROM products_summary;  -- Pre-computed summary table

-- Detailed view for individual product pages
CREATE VIEW v_products_detail AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'description', p.description,
        'price', p.current_price,
        'category', (SELECT data FROM v_categories WHERE id = p.category_id),
        'images', (
            SELECT jsonb_agg(
                jsonb_build_object('url', url, 'alt', alt_text)
                ORDER BY sort_order
            )
            FROM product_images WHERE product_id = p.id
        ),
        'specifications', p.specifications,  -- JSONB column
        'reviews_summary', (SELECT data FROM v_product_reviews WHERE product_id = p.id),
        'related_products', (
            -- Complex recommendation logic
            SELECT jsonb_agg(
                jsonb_build_object('id', rp.id, 'name', rp.name, 'price', rp.current_price)
            )
            FROM products rp
            WHERE rp.category_id = p.category_id
            AND rp.id != p.id
            ORDER BY similarity(rp.name, p.name) DESC
            LIMIT 4
        )
    ) as data
FROM products p;
```

## Security and Access Control Patterns

### Row-Level Security Integration

Views automatically inherit row-level security from base tables:

```sql
-- Enable RLS on base tables
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE orders ENABLE ROW LEVEL SECURITY;

-- Create RLS policies
CREATE POLICY users_own_data ON users
    FOR ALL TO app_user
    USING (id = current_setting('app.current_user_id')::uuid);

CREATE POLICY orders_own_data ON orders
    FOR ALL TO app_user
    USING (user_id = current_setting('app.current_user_id')::uuid);

-- Views automatically respect RLS
CREATE VIEW v_user_orders AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id,
        'status', o.status,
        'user', (SELECT data FROM v_users WHERE id = o.user_id), -- RLS applied
        'items', (/* ... */)
    ) as data
FROM orders o;  -- RLS applied here too
```

### Role-Based View Access

Create different views for different access levels:

```sql
-- Public view - minimal data
CREATE VIEW v_users_public AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'display_name', display_name,
        'avatar_url', avatar_url,
        'member_since', DATE_PART('year', created_at)
    ) as data
FROM v_users_full
WHERE is_public_profile = true;

-- Private view - full data for authenticated users
CREATE VIEW v_users_private AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'display_name', display_name,
        'preferences', preferences,
        'activity_summary', activity_summary,
        'private_notes', private_notes
    ) as data
FROM v_users_full;

-- Admin view - includes sensitive data
CREATE VIEW v_users_admin AS
SELECT
    id,
    data || jsonb_build_object(
        'last_login_ip', last_login_ip,
        'registration_ip', registration_ip,
        'support_tickets', support_ticket_count,
        'account_flags', account_flags
    ) as data
FROM v_users_private;
```

## Development Best Practices

### 1. View Design Guidelines

- **One concern per view**: Each view should serve a specific GraphQL type or use case
- **Consistent JSON structure**: Follow snake_case naming that auto-converts to camelCase
- **Handle NULLs gracefully**: Use COALESCE for optional fields with sensible defaults
- **Document complex logic**: Add comments explaining business rules and calculations

### 2. Performance Considerations

- **Index appropriately**: Create indexes on columns used in JOINs and WHERE clauses
- **Limit aggregations**: Use materialized views for expensive computations
- **Test with realistic data**: Ensure views perform well with production data volumes
- **Monitor query plans**: Use EXPLAIN ANALYZE to optimize view performance

### 3. Migration Strategies

When moving from tightly coupled designs:

```sql
-- Phase 1: Create views that mirror existing structure
CREATE VIEW v_legacy_products AS
SELECT id, data FROM products;

-- Phase 2: Gradually add computed fields
CREATE OR REPLACE VIEW v_legacy_products AS
SELECT
    id,
    data || jsonb_build_object(
        'computed_rating', (SELECT AVG(rating) FROM reviews WHERE product_id = products.id),
        'stock_status', CASE WHEN inventory > 0 THEN 'in_stock' ELSE 'out_of_stock' END
    ) as data
FROM products;

-- Phase 3: Full restructuring with normalized base tables
CREATE OR REPLACE VIEW v_legacy_products AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'category', c.name,
        'price', p.current_price,
        'rating', COALESCE(r.avg_rating, 0),
        'in_stock', i.quantity > 0
    ) as data
FROM products_normalized p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN product_ratings r ON p.id = r.product_id
LEFT JOIN inventory i ON p.id = i.product_id;
```

## Key Benefits

This view-based architecture provides:

1. **Database Design Freedom**: Design for data integrity, not API constraints
2. **API Optimization**: Structure data exactly as clients need it
3. **Performance**: Leverage PostgreSQL's query optimization
4. **Maintainability**: Clear separation between data storage and presentation
5. **Flexibility**: Support multiple API views of the same data
6. **Security**: Implement access control at the appropriate layer

By mastering these patterns, you can build robust, scalable applications that leverage both PostgreSQL's strengths and GraphQL's client optimization benefits.
