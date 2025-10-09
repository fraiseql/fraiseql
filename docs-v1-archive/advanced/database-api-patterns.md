# Database API Design Patterns

FraiseQL enables sophisticated database-backed API patterns that leverage PostgreSQL's advanced features while maintaining clean separation of concerns. This guide explores proven patterns for building robust, scalable APIs directly from your database schema.

## Pattern Catalog Overview

This document covers essential patterns for:

- Schema evolution without breaking changes
- Complex view composition for optimal performance
- Multi-tenant architectures with isolation guarantees
- Temporal data handling and time-series queries
- Hierarchical and tree-structured data
- Polymorphic associations and flexible relationships
- Cache invalidation strategies for materialized views

## Schema Evolution Strategies

### Additive Changes Pattern

Always add, never remove or modify, to maintain backward compatibility:

```sql
-- Version 1: Initial schema
CREATE TABLE tb_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(200) NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,  -- Keep for filtering
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at
    ) AS data
FROM tb_users;

-- Version 2: Add new columns (backward compatible)
ALTER TABLE tb_users
ADD COLUMN IF NOT EXISTS avatar_url TEXT,
ADD COLUMN IF NOT EXISTS bio TEXT,
ADD COLUMN IF NOT EXISTS settings JSONB DEFAULT '{}';

-- Update view to include new fields with defaults
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'created_at', created_at,
        -- New fields with graceful defaults
        'avatar_url', COALESCE(avatar_url, 'https://api.example.com/default-avatar.png'),
        'bio', COALESCE(bio, ''),
        'settings', COALESCE(settings, '{}'::jsonb)
    ) AS data
FROM tb_users;
```

### Deprecation Pattern

Mark deprecated fields while maintaining them for compatibility:

```sql
-- Mark field as deprecated in view
CREATE OR REPLACE VIEW v_user AS
SELECT
    id,
    email,
    jsonb_build_object(
        '__typename', 'User',
        'id', id,
        'email', email,
        'name', name,
        'username', name,  -- Deprecated: use 'name' instead
        'created_at', created_at,
        '__deprecated', jsonb_build_object(
            'username', 'Use name field instead. Will be removed in v2.0'
        )
    ) AS data
FROM tb_users;

-- Add deprecation notice
COMMENT ON COLUMN v_user.data IS
'User data. Note: username field is deprecated, use name instead';
```

### Schema Versioning Pattern

Maintain multiple API versions simultaneously:

```sql
-- Schema for versioning
CREATE SCHEMA IF NOT EXISTS api_v1;
CREATE SCHEMA IF NOT EXISTS api_v2;

-- Version 1 API (stable)
CREATE OR REPLACE VIEW api_v1.users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'fullName', name  -- v1 uses fullName
    ) AS data
FROM tb_users;

-- Version 2 API (current)
CREATE OR REPLACE VIEW api_v2.users AS
SELECT
    id,
    jsonb_build_object(
        'id', id,
        'email', email,
        'name', name,  -- v2 uses name
        'profile', jsonb_build_object(
            'avatar', avatar_url,
            'bio', bio
        )
    ) AS data
FROM tb_users;

-- Default to latest version
CREATE OR REPLACE VIEW v_user AS
SELECT * FROM api_v2.users;
```

## View Composition Patterns

### Layered View Architecture

Build complex views from simpler components:

```sql
-- Layer 1: Base entity views
CREATE OR REPLACE VIEW v_base_products AS
SELECT
    id,
    sku,
    name,
    price,
    category_id,
    jsonb_build_object(
        '__typename', 'Product',
        'id', id,
        'sku', sku,
        'name', name,
        'price', price
    ) AS data
FROM tb_products
WHERE deleted_at IS NULL;

-- Layer 2: Views with single relationship
CREATE OR REPLACE VIEW v_products_with_category AS
SELECT
    p.id,
    p.sku,
    p.category_id,
    p.price,
    jsonb_build_object(
        '__typename', 'Product',
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'price', p.price,
        'category', c.data
    ) AS data
FROM v_base_products p
LEFT JOIN v_category c ON c.id = p.category_id;

-- Layer 3: Complete aggregate views
CREATE OR REPLACE VIEW v_products_full AS
SELECT
    p.id,
    p.sku,
    p.price,
    jsonb_build_object(
        '__typename', 'ProductFull',
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'price', p.price,
        'category', c.data,
        'inventory', (
            SELECT jsonb_build_object(
                'in_stock', SUM(quantity),
                'reserved', SUM(reserved),
                'available', SUM(quantity - reserved)
            )
            FROM tb_inventory
            WHERE product_id = p.id
        ),
        'reviews', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', r.id,
                    'rating', r.rating,
                    'comment', r.comment,
                    'user', u.data
                ) ORDER BY r.created_at DESC
            )
            FROM tb_reviews r
            JOIN v_user u ON u.id = r.user_id
            WHERE r.product_id = p.id
            LIMIT 10
        )
    ) AS data
FROM v_base_products p
LEFT JOIN v_category c ON c.id = p.category_id;
```

### Conditional Composition Pattern

Include different data based on context or permissions:

```sql
-- View with role-based data exposure
CREATE OR REPLACE FUNCTION v_user_contextual(
    p_viewer_role TEXT DEFAULT 'public',
    p_viewer_id UUID DEFAULT NULL
)
RETURNS TABLE (
    id UUID,
    email VARCHAR(255),
    data JSONB
) AS $$
BEGIN
    RETURN QUERY
    SELECT
        u.id,
        CASE
            WHEN p_viewer_role IN ('admin', 'moderator') THEN u.email
            WHEN u.id = p_viewer_id THEN u.email
            ELSE NULL
        END AS email,
        jsonb_build_object(
            '__typename', 'User',
            'id', u.id,
            'name', u.name,
            'email', CASE
                WHEN p_viewer_role IN ('admin', 'moderator') THEN u.email
                WHEN u.id = p_viewer_id THEN u.email
                ELSE '[hidden]'
            END,
            'phone', CASE
                WHEN p_viewer_role = 'admin' THEN u.phone
                WHEN u.id = p_viewer_id THEN u.phone
                ELSE NULL
            END,
            'private_notes', CASE
                WHEN p_viewer_role = 'admin' THEN u.private_notes
                ELSE NULL
            END,
            'stats', CASE
                WHEN p_viewer_role IN ('admin', 'moderator') OR u.id = p_viewer_id
                THEN (
                    SELECT jsonb_build_object(
                        'post_count', COUNT(*),
                        'follower_count', (
                            SELECT COUNT(*) FROM tb_follows
                            WHERE followed_id = u.id
                        )
                    )
                    FROM tb_posts WHERE author_id = u.id
                )
                ELSE NULL
            END
        ) AS data
    FROM tb_users u;
END;
$$ LANGUAGE plpgsql STABLE;
```

## Multi-Tenant Architecture Patterns

### Row-Level Security (RLS) Pattern

Implement tenant isolation at the database level:

```sql
-- Tenant configuration
CREATE TABLE tb_tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(200) NOT NULL,
    slug VARCHAR(50) NOT NULL UNIQUE,
    plan VARCHAR(50) DEFAULT 'free',
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Multi-tenant table with RLS
CREATE TABLE tb_projects (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tb_tenants(id),
    name VARCHAR(200) NOT NULL,
    description TEXT,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Enable Row Level Security
ALTER TABLE tb_projects ENABLE ROW LEVEL SECURITY;

-- Create policy for tenant isolation
CREATE POLICY tenant_isolation ON tb_projects
    FOR ALL
    USING (tenant_id = current_setting('app.current_tenant_id')::UUID);

-- Create policy for cross-tenant admin access
CREATE POLICY admin_access ON tb_projects
    FOR ALL
    USING (
        EXISTS (
            SELECT 1 FROM tb_users
            WHERE id = current_setting('app.current_user_id')::UUID
            AND role = 'super_admin'
        )
    );

-- Tenant-aware view
CREATE OR REPLACE VIEW v_projects AS
SELECT
    p.id,
    p.name,
    jsonb_build_object(
        '__typename', 'Project',
        'id', p.id,
        'name', p.name,
        'description', p.description,
        'settings', p.settings,
        'tenant', (
            SELECT jsonb_build_object(
                'id', t.id,
                'name', t.name,
                'plan', t.plan
            )
            FROM tb_tenants t
            WHERE t.id = p.tenant_id
        ),
        'member_count', (
            SELECT COUNT(*)
            FROM tb_project_members
            WHERE project_id = p.id
        ),
        'created_at', p.created_at
    ) AS data
FROM tb_projects p;

-- Function to set tenant context
CREATE OR REPLACE FUNCTION set_tenant_context(
    p_tenant_id UUID,
    p_user_id UUID
) RETURNS void AS $$
BEGIN
    PERFORM set_config('app.current_tenant_id', p_tenant_id::TEXT, true);
    PERFORM set_config('app.current_user_id', p_user_id::TEXT, true);
END;
$$ LANGUAGE plpgsql;
```

### Schema-Level Isolation Pattern

Separate schemas for complete tenant isolation:

```sql
-- Function to create tenant schema
CREATE OR REPLACE FUNCTION create_tenant_schema(
    p_tenant_slug VARCHAR(50)
) RETURNS void AS $$
DECLARE
    v_schema_name TEXT;
BEGIN
    v_schema_name := 'tenant_' || p_tenant_slug;

    -- Create schema
    EXECUTE format('CREATE SCHEMA IF NOT EXISTS %I', v_schema_name);

    -- Create tables in tenant schema
    EXECUTE format('
        CREATE TABLE %I.projects (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            name VARCHAR(200) NOT NULL,
            created_at TIMESTAMPTZ DEFAULT NOW()
        )', v_schema_name);

    -- Create views in tenant schema
    EXECUTE format('
        CREATE OR REPLACE VIEW %I.v_projects AS
        SELECT
            id,
            jsonb_build_object(
                ''__typename'', ''Project'',
                ''id'', id,
                ''name'', name,
                ''created_at'', created_at
            ) AS data
        FROM %I.projects
    ', v_schema_name, v_schema_name);
END;
$$ LANGUAGE plpgsql;

-- Dynamic tenant routing
CREATE OR REPLACE FUNCTION route_to_tenant(
    p_tenant_slug VARCHAR(50),
    p_query TEXT
) RETURNS SETOF RECORD AS $$
DECLARE
    v_schema_name TEXT;
BEGIN
    v_schema_name := 'tenant_' || p_tenant_slug;

    -- Set search path to tenant schema
    EXECUTE format('SET search_path TO %I, public', v_schema_name);

    -- Execute query in tenant context
    RETURN QUERY EXECUTE p_query;

    -- Reset search path
    SET search_path TO public;
END;
$$ LANGUAGE plpgsql;
```

## Temporal Data Patterns

### Bi-Temporal Pattern

Track both valid time and transaction time:

```sql
-- Bi-temporal table
CREATE TABLE tb_prices (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    product_id UUID NOT NULL REFERENCES tb_products(id),
    price NUMERIC(10,2) NOT NULL,
    currency VARCHAR(3) DEFAULT 'USD',
    -- Valid time (when the price is effective)
    valid_from DATE NOT NULL,
    valid_to DATE,
    -- Transaction time (when the record was added)
    tx_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    tx_to TIMESTAMPTZ DEFAULT 'infinity',
    -- Ensure no overlapping valid periods
    EXCLUDE USING gist (
        product_id WITH =,
        daterange(valid_from, valid_to, '[)') WITH &&
    ) WHERE (tx_to = 'infinity')
);

-- Current prices view
CREATE OR REPLACE VIEW v_current_prices AS
SELECT
    p.id AS product_id,
    jsonb_build_object(
        '__typename', 'CurrentPrice',
        'product_id', p.id,
        'price', pr.price,
        'currency', pr.currency,
        'valid_from', pr.valid_from,
        'valid_to', pr.valid_to
    ) AS data
FROM tb_products p
LEFT JOIN LATERAL (
    SELECT *
    FROM tb_prices
    WHERE product_id = p.id
        AND tx_to = 'infinity'
        AND valid_from <= CURRENT_DATE
        AND (valid_to IS NULL OR valid_to > CURRENT_DATE)
    ORDER BY valid_from DESC
    LIMIT 1
) pr ON true;

-- Historical prices at point in time
CREATE OR REPLACE FUNCTION get_price_at(
    p_product_id UUID,
    p_valid_date DATE,
    p_tx_time TIMESTAMPTZ DEFAULT NOW()
) RETURNS NUMERIC AS $$
    SELECT price
    FROM tb_prices
    WHERE product_id = p_product_id
        AND valid_from <= p_valid_date
        AND (valid_to IS NULL OR valid_to > p_valid_date)
        AND tx_from <= p_tx_time
        AND tx_to > p_tx_time
    ORDER BY valid_from DESC
    LIMIT 1;
$$ LANGUAGE sql STABLE;

-- Update price (maintaining history)
CREATE OR REPLACE FUNCTION update_price(
    p_product_id UUID,
    p_new_price NUMERIC,
    p_valid_from DATE
) RETURNS void AS $$
BEGIN
    -- Close current price record
    UPDATE tb_prices
    SET tx_to = NOW()
    WHERE product_id = p_product_id
        AND tx_to = 'infinity';

    -- Insert new price record
    INSERT INTO tb_prices (product_id, price, valid_from)
    VALUES (p_product_id, p_new_price, p_valid_from);
END;
$$ LANGUAGE plpgsql;
```

### Event Sourcing Pattern

Store all changes as events:

```sql
-- Event store
CREATE TABLE tb_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    stream_id UUID NOT NULL,
    stream_type VARCHAR(50) NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB NOT NULL,
    event_metadata JSONB DEFAULT '{}',
    event_version INT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    created_by UUID
);

-- Ensure event ordering
CREATE UNIQUE INDEX idx_event_ordering
ON tb_events(stream_id, event_version);

CREATE INDEX idx_stream ON tb_events(stream_id, event_version);

-- Current state projection
CREATE MATERIALIZED VIEW mv_order_projections AS
WITH latest_events AS (
    SELECT DISTINCT ON (stream_id)
        stream_id AS order_id,
        event_data,
        event_type,
        created_at
    FROM tb_events
    WHERE stream_type = 'Order'
    ORDER BY stream_id, event_version DESC
)
SELECT
    order_id,
    jsonb_build_object(
        '__typename', 'Order',
        'id', order_id,
        'status', CASE
            WHEN event_type = 'OrderCancelled' THEN 'cancelled'
            WHEN event_type = 'OrderShipped' THEN 'shipped'
            WHEN event_type = 'OrderDelivered' THEN 'delivered'
            ELSE 'pending'
        END,
        'data', event_data,
        'last_event', event_type,
        'updated_at', created_at
    ) AS data
FROM latest_events;

-- Rebuild projection from events
CREATE OR REPLACE FUNCTION rebuild_order_projection(
    p_order_id UUID
) RETURNS JSONB AS $$
DECLARE
    v_state JSONB := '{}'::jsonb;
    v_event RECORD;
BEGIN
    FOR v_event IN
        SELECT event_type, event_data
        FROM tb_events
        WHERE stream_id = p_order_id
            AND stream_type = 'Order'
        ORDER BY event_version
    LOOP
        -- Apply events to build current state
        CASE v_event.event_type
            WHEN 'OrderCreated' THEN
                v_state := v_event.event_data;
            WHEN 'OrderItemAdded' THEN
                v_state := jsonb_set(
                    v_state,
                    '{items}',
                    COALESCE(v_state->'items', '[]'::jsonb) ||
                    jsonb_build_array(v_event.event_data)
                );
            WHEN 'OrderShipped' THEN
                v_state := v_state ||
                    jsonb_build_object(
                        'status', 'shipped',
                        'shipped_at', v_event.event_data->>'shipped_at'
                    );
            ELSE
                v_state := v_state || v_event.event_data;
        END CASE;
    END LOOP;

    RETURN v_state;
END;
$$ LANGUAGE plpgsql;
```

## Hierarchical Data Patterns

### Adjacency List with Recursive CTE

Handle tree structures efficiently:

```sql
-- Category hierarchy
CREATE TABLE tb_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    parent_id UUID REFERENCES tb_categories(id),
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    sort_order INT DEFAULT 0,
    metadata JSONB DEFAULT '{}'
);

-- Recursive view for full tree
CREATE OR REPLACE VIEW v_category_tree AS
WITH RECURSIVE category_tree AS (
    -- Root categories
    SELECT
        id,
        parent_id,
        name,
        slug,
        sort_order,
        0 AS depth,
        ARRAY[id] AS path,
        ARRAY[name] AS breadcrumb
    FROM tb_categories
    WHERE parent_id IS NULL

    UNION ALL

    -- Child categories
    SELECT
        c.id,
        c.parent_id,
        c.name,
        c.slug,
        c.sort_order,
        ct.depth + 1,
        ct.path || c.id,
        ct.breadcrumb || c.name
    FROM tb_categories c
    JOIN category_tree ct ON c.parent_id = ct.id
    WHERE ct.depth < 10  -- Prevent infinite recursion
)
SELECT
    id,
    jsonb_build_object(
        '__typename', 'CategoryTree',
        'id', id,
        'name', name,
        'slug', slug,
        'depth', depth,
        'path', path,
        'breadcrumb', breadcrumb,
        'parent_id', parent_id,
        'children', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', c.id,
                    'name', c.name,
                    'slug', c.slug,
                    'sort_order', c.sort_order
                ) ORDER BY c.sort_order, c.name
            )
            FROM tb_categories c
            WHERE c.parent_id = category_tree.id
        )
    ) AS data
FROM category_tree
ORDER BY path;

-- Get ancestors of a category
CREATE OR REPLACE FUNCTION get_category_ancestors(
    p_category_id UUID
) RETURNS TABLE (
    id UUID,
    name VARCHAR(100),
    level INT
) AS $$
WITH RECURSIVE ancestors AS (
    SELECT
        id,
        parent_id,
        name,
        0 AS level
    FROM tb_categories
    WHERE id = p_category_id

    UNION ALL

    SELECT
        c.id,
        c.parent_id,
        c.name,
        a.level + 1
    FROM tb_categories c
    JOIN ancestors a ON c.id = a.parent_id
)
SELECT id, name, level
FROM ancestors
ORDER BY level DESC;
$$ LANGUAGE sql STABLE;
```

### Materialized Path Pattern

Store the full path for fast queries:

```sql
-- Using ltree extension for materialized paths
CREATE EXTENSION IF NOT EXISTS ltree;

CREATE TABLE tb_categories_ltree (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    path ltree NOT NULL,
    UNIQUE(path)
);

-- Index for path operations
CREATE INDEX idx_category_path ON tb_categories_ltree USING gist(path);

-- Insert with path
INSERT INTO tb_categories_ltree (name, path) VALUES
    ('Electronics', 'electronics'),
    ('Computers', 'electronics.computers'),
    ('Laptops', 'electronics.computers.laptops'),
    ('Gaming Laptops', 'electronics.computers.laptops.gaming');

-- Find all descendants
CREATE OR REPLACE VIEW v_category_descendants AS
SELECT
    c.id,
    c.name,
    c.path,
    jsonb_build_object(
        '__typename', 'CategoryNode',
        'id', c.id,
        'name', c.name,
        'path', c.path::text,
        'depth', nlevel(c.path),
        'descendants', (
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', d.id,
                    'name', d.name,
                    'path', d.path::text
                )
            )
            FROM tb_categories_ltree d
            WHERE d.path <@ c.path
                AND d.id != c.id
        )
    ) AS data
FROM tb_categories_ltree c;
```

## Polymorphic Associations

### Single Table Inheritance Pattern

Store different entity types in one table:

```sql
-- Polymorphic notifications
CREATE TABLE tb_notifications (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES tb_users(id),
    type VARCHAR(50) NOT NULL,
    -- Polymorphic reference
    entity_type VARCHAR(50),
    entity_id UUID,
    -- Type-specific data
    data JSONB NOT NULL,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE INDEX idx_user_notifications
ON tb_notifications(user_id, read_at, created_at DESC);

-- Type-specific views
CREATE OR REPLACE VIEW v_notifications AS
SELECT
    n.id,
    n.user_id,
    n.read_at,
    jsonb_build_object(
        '__typename', 'Notification',
        'id', n.id,
        'type', n.type,
        'read', n.read_at IS NOT NULL,
        'created_at', n.created_at,
        -- Polymorphic entity resolution
        'entity', CASE n.entity_type
            WHEN 'Post' THEN (
                SELECT jsonb_build_object(
                    '__typename', 'Post',
                    'id', p.id,
                    'title', p.title,
                    'author', a.data
                )
                FROM tb_posts p
                JOIN v_user a ON a.id = p.author_id
                WHERE p.id = n.entity_id
            )
            WHEN 'Comment' THEN (
                SELECT jsonb_build_object(
                    '__typename', 'Comment',
                    'id', c.id,
                    'content', LEFT(c.content, 100),
                    'author', a.data
                )
                FROM tb_comments c
                JOIN v_user a ON a.id = c.author_id
                WHERE c.id = n.entity_id
            )
            WHEN 'User' THEN (
                SELECT jsonb_build_object(
                    '__typename', 'User',
                    'id', u.id,
                    'name', u.name,
                    'avatar_url', u.avatar_url
                )
                FROM tb_users u
                WHERE u.id = n.entity_id
            )
            ELSE NULL
        END,
        -- Type-specific data
        'details', CASE n.type
            WHEN 'post_liked' THEN
                n.data || jsonb_build_object(
                    'message', n.data->>'liker_name' || ' liked your post'
                )
            WHEN 'comment_reply' THEN
                n.data || jsonb_build_object(
                    'message', n.data->>'replier_name' || ' replied to your comment'
                )
            WHEN 'new_follower' THEN
                n.data || jsonb_build_object(
                    'message', n.data->>'follower_name' || ' started following you'
                )
            ELSE n.data
        END
    ) AS data
FROM tb_notifications n
ORDER BY n.created_at DESC;
```

### Table Per Type with Union Pattern

Separate tables unified through views:

```sql
-- Different activity types
CREATE TABLE tb_page_views (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES tb_users(id),
    page_url TEXT NOT NULL,
    referrer TEXT,
    duration_seconds INT,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_button_clicks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES tb_users(id),
    button_id VARCHAR(100) NOT NULL,
    page_url TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

CREATE TABLE tb_form_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES tb_users(id),
    form_id VARCHAR(100) NOT NULL,
    form_data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Unified activity view
CREATE OR REPLACE VIEW v_user_activities AS
SELECT
    id,
    user_id,
    activity_type,
    created_at,
    jsonb_build_object(
        '__typename', 'UserActivity',
        'id', id,
        'type', activity_type,
        'user', (
            SELECT data FROM v_user WHERE id = user_id
        ),
        'details', details,
        'created_at', created_at
    ) AS data
FROM (
    SELECT
        id,
        user_id,
        'page_view' AS activity_type,
        jsonb_build_object(
            'page_url', page_url,
            'referrer', referrer,
            'duration', duration_seconds
        ) AS details,
        created_at
    FROM tb_page_views

    UNION ALL

    SELECT
        id,
        user_id,
        'button_click' AS activity_type,
        jsonb_build_object(
            'button_id', button_id,
            'page_url', page_url
        ) AS details,
        created_at
    FROM tb_button_clicks

    UNION ALL

    SELECT
        id,
        user_id,
        'form_submission' AS activity_type,
        jsonb_build_object(
            'form_id', form_id,
            'fields', form_data
        ) AS details,
        created_at
    FROM tb_form_submissions
) activities
ORDER BY created_at DESC;
```

## Cache Invalidation Strategies

### Table View (tv_) Sync Pattern

**CRITICAL: This pattern follows FraiseQL's core rule - triggers ONLY on tv_ tables for cache invalidation.**

Explicit sync functions for maintainable projection updates:

```sql
-- Table view with proper Sacred Trinity + Foreign Key pattern
CREATE TABLE tv_post_stats (
    -- Sacred Trinity pattern
    id INTEGER GENERATED BY DEFAULT AS IDENTITY,
    pk_post_stats UUID DEFAULT gen_random_uuid() NOT NULL,

    -- Foreign key to source entity
    fk_post INTEGER NOT NULL,

    -- Business data and versioning
    data JSONB NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at TIMESTAMPTZ DEFAULT NOW(),

    -- Constraints
    CONSTRAINT pk_tv_post_stats PRIMARY KEY (id),
    CONSTRAINT uq_tv_post_stats_pk UNIQUE (pk_post_stats),
    CONSTRAINT fk_tv_post_stats_post FOREIGN KEY (fk_post) REFERENCES tb_post(id),
    CONSTRAINT uq_tv_post_stats_post UNIQUE (fk_post)
);

-- ONLY trigger: cache invalidation on tv_ table
CREATE TRIGGER trg_tv_post_stats_version
AFTER INSERT OR UPDATE OR DELETE ON tv_post_stats
FOR EACH STATEMENT
EXECUTE FUNCTION fn_increment_version('post_stats');

-- Sync function called explicitly after mutations
CREATE OR REPLACE FUNCTION sync_post_stats(
    p_post_id INTEGER
) RETURNS void AS $$
BEGIN
    -- Upsert post statistics with fresh calculations
    INSERT INTO tv_post_stats (fk_post, data, version, updated_at)
    SELECT
        p.id AS fk_post,
        jsonb_build_object(
            '__typename', 'PostStatistics',
            'post_id', p.pk_post,
            'comment_count', COALESCE(c.comment_count, 0),
            'latest_comments', (
                SELECT jsonb_agg(v_comment.data ORDER BY com.created_at DESC)
                FROM tb_comment com
                JOIN v_comment ON v_comment.id = com.id
                WHERE com.fk_post = p.id
                LIMIT 5
            ),
            'view_count', p.view_count,
            'engagement_score', (
                COALESCE(c.comment_count, 0) * 10 +
                COALESCE(p.view_count, 0) * 1
            ),
            'last_activity', GREATEST(
                p.created_at,
                COALESCE(c.last_comment_at, p.created_at)
            )
        ) AS data,
        COALESCE(
            (SELECT version + 1 FROM tv_post_stats WHERE fk_post = p.id),
            1
        ) AS version,
        NOW() AS updated_at
    FROM tb_post p
    LEFT JOIN (
        SELECT fk_post,
               COUNT(*) AS comment_count,
               MAX(created_at) AS last_comment_at
        FROM tb_comment
        WHERE fk_post = p_post_id
        GROUP BY fk_post
    ) c ON c.fk_post = p.id
    WHERE p.id = p_post_id
    ON CONFLICT (fk_post) DO UPDATE SET
        data = EXCLUDED.data,
        version = EXCLUDED.version,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;

-- Mutation with explicit sync
CREATE OR REPLACE FUNCTION fn_create_comment(input_data JSONB)
RETURNS mutation_result AS $$
DECLARE
    v_comment_id INTEGER;
    v_post_id INTEGER;
    v_author_id INTEGER;
BEGIN
    -- Extract IDs
    v_post_id := (input_data->>'post_id')::INTEGER;
    v_author_id := (input_data->>'author_id')::INTEGER;

    -- Create comment (NO TRIGGERS will fire on tb_ table)
    INSERT INTO tb_comment (
        fk_post,
        fk_author,
        content
    ) VALUES (
        v_post_id,
        v_author_id,
        input_data->>'content'
    ) RETURNING id INTO v_comment_id;

    -- Explicitly sync post statistics
    PERFORM sync_post_stats(v_post_id);

    -- Sync any other affected projections (if applicable)
    PERFORM sync_user_activity_stats(v_author_id);

    RETURN ROW(
        true,
        'Comment created successfully',
        jsonb_build_object('comment_id', v_comment_id)
    )::mutation_result;
END;
$$ LANGUAGE plpgsql;

-- Python integration with sync calls
from fraiseql import mutation

@mutation
async def create_comment(input: CreateCommentInput, context) -> CreateCommentSuccess | CreateCommentError:
    """Create comment with automatic projection sync."""
    result = await context.db.execute_function('fn_create_comment', input)

    if result['success']:
        # The function already synced projections
        comment_data = await context.db.query_one(
            "SELECT data FROM v_comment WHERE id = $1",
            result['data']['comment_id']
        )
        return CreateCommentSuccess(comment=Comment.from_dict(comment_data))
    else:
        return CreateCommentError(message=result['message'])
```

This sync pattern approach has several advantages:

1. **NO triggers on tb_ tables** - No hidden side effects from triggers on base tables
2. **Explicit control** - You know exactly when projections are updated
3. **Better debugging** - Easier to trace when and why projections change
4. **Selective updates** - Only sync what's actually affected
5. **Transaction safety** - Sync happens within the mutation transaction
6. **Performance predictable** - No surprise trigger overhead on writes
7. **Cache invalidation automatic** - Triggers on tv_ tables handle cache invalidation

### Batch Sync Pattern

For high-volume scenarios, batch sync multiple entities:

```sql
-- Batch sync multiple posts
CREATE OR REPLACE FUNCTION sync_post_stats_batch(
    p_post_ids INTEGER[]
) RETURNS void AS $$
BEGIN
    -- Batch update multiple posts at once
    INSERT INTO tv_post_stats (fk_post, data, version, updated_at)
    SELECT
        p.id AS fk_post,
        jsonb_build_object(
            '__typename', 'PostStatistics',
            'post_id', p.pk_post,
            'comment_count', COALESCE(stats.comment_count, 0),
            'view_count', p.view_count,
            'engagement_score', (
                COALESCE(stats.comment_count, 0) * 10 +
                COALESCE(p.view_count, 0) * 1
            ),
            'last_activity', GREATEST(
                p.created_at,
                COALESCE(stats.last_comment_at, p.created_at)
            )
        ) AS data,
        1 AS version,
        NOW() AS updated_at
    FROM tb_post p
    LEFT JOIN (
        -- Calculate stats for all posts in one query
        SELECT
            c.fk_post,
            COUNT(*) AS comment_count,
            MAX(c.created_at) AS last_comment_at
        FROM tb_comment c
        WHERE c.fk_post = ANY(p_post_ids)
        GROUP BY c.fk_post
    ) stats ON stats.fk_post = p.id
    WHERE p.id = ANY(p_post_ids)
    ON CONFLICT (fk_post) DO UPDATE SET
        data = EXCLUDED.data,
        version = tv_post_stats.version + 1,
        updated_at = EXCLUDED.updated_at;
END;
$$ LANGUAGE plpgsql;
```

## Performance Optimization Patterns

### Query Result Caching

Cache expensive query results:

```sql
-- Query result cache
CREATE TABLE tb_query_cache (
    cache_key VARCHAR(255) PRIMARY KEY,
    query_hash VARCHAR(64) NOT NULL,
    result_data JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    hit_count INT DEFAULT 0,
    last_accessed TIMESTAMPTZ DEFAULT NOW()
);

-- Cache management function
CREATE OR REPLACE FUNCTION cached_query(
    p_cache_key VARCHAR(255),
    p_query TEXT,
    p_ttl INTERVAL DEFAULT '1 hour'
) RETURNS JSONB AS $$
DECLARE
    v_result JSONB;
    v_query_hash VARCHAR(64);
BEGIN
    -- Generate query hash
    v_query_hash := encode(digest(p_query, 'sha256'), 'hex');

    -- Check cache
    SELECT result_data INTO v_result
    FROM tb_query_cache
    WHERE cache_key = p_cache_key
        AND query_hash = v_query_hash
        AND expires_at > NOW();

    IF v_result IS NOT NULL THEN
        -- Update hit count
        UPDATE tb_query_cache
        SET
            hit_count = hit_count + 1,
            last_accessed = NOW()
        WHERE cache_key = p_cache_key;

        RETURN v_result;
    END IF;

    -- Execute query
    EXECUTE p_query INTO v_result;

    -- Store in cache
    INSERT INTO tb_query_cache (
        cache_key,
        query_hash,
        result_data,
        expires_at
    ) VALUES (
        p_cache_key,
        v_query_hash,
        v_result,
        NOW() + p_ttl
    )
    ON CONFLICT (cache_key) DO UPDATE SET
        query_hash = EXCLUDED.query_hash,
        result_data = EXCLUDED.result_data,
        expires_at = EXCLUDED.expires_at,
        created_at = NOW(),
        hit_count = 0;

    -- Clean expired entries
    DELETE FROM tb_query_cache
    WHERE expires_at < NOW() - INTERVAL '1 day';

    RETURN v_result;
END;
$$ LANGUAGE plpgsql;

-- Usage example
SELECT cached_query(
    'top_posts_2024',
    'SELECT jsonb_agg(data) FROM v_post WHERE published_at > ''2024-01-01'' ORDER BY view_count DESC LIMIT 10',
    '6 hours'::interval
);
```

### Pagination Optimization

Efficient pagination for large datasets:

```sql
-- Cursor-based pagination
CREATE OR REPLACE FUNCTION paginate_cursor(
    p_table_name TEXT,
    p_cursor_field TEXT,
    p_cursor_value ANYELEMENT DEFAULT NULL,
    p_limit INT DEFAULT 20,
    p_direction TEXT DEFAULT 'next'
) RETURNS TABLE (
    data JSONB,
    next_cursor TEXT,
    has_more BOOLEAN
) AS $$
DECLARE
    v_query TEXT;
    v_result JSONB;
    v_count INT;
BEGIN
    -- Build query based on direction
    IF p_direction = 'next' THEN
        v_query := format(
            'SELECT jsonb_agg(sub.data)
             FROM (
                SELECT data, %I as cursor_field
                FROM %I
                WHERE %I > $1
                ORDER BY %I
                LIMIT %s
             ) sub',
            p_cursor_field, p_table_name,
            p_cursor_field, p_cursor_field, p_limit + 1
        );
    ELSE
        v_query := format(
            'SELECT jsonb_agg(sub.data)
             FROM (
                SELECT data, %I as cursor_field
                FROM %I
                WHERE %I < $1
                ORDER BY %I DESC
                LIMIT %s
             ) sub',
            p_cursor_field, p_table_name,
            p_cursor_field, p_cursor_field, p_limit + 1
        );
    END IF;

    -- Execute query
    EXECUTE v_query INTO v_result USING p_cursor_value;

    -- Check if there are more results
    v_count := jsonb_array_length(COALESCE(v_result, '[]'::jsonb));

    RETURN QUERY SELECT
        CASE
            WHEN v_count > p_limit
            THEN v_result #- array[v_count - 1]::text[]
            ELSE v_result
        END AS data,
        CASE
            WHEN v_count > p_limit
            THEN (v_result->>(v_count - 2))::jsonb->>'id'
            ELSE NULL
        END AS next_cursor,
        v_count > p_limit AS has_more;
END;
$$ LANGUAGE plpgsql;
```

## Trade-offs and Decisions

### View Complexity vs Performance

| Approach | Pros | Cons | Use When |
|----------|------|------|----------|
| Simple Views | Fast, real-time data | Limited composition | Simple queries, low complexity |
| Layered Views | Reusable, maintainable | Multiple joins | Moderate complexity, good indexes |
| Materialized Views | Very fast reads | Stale data, storage | Complex aggregations, read-heavy |
| Table Views (tv_) | Fast, incremental updates | Manual sync management | Frequently accessed aggregations |
| Sync Functions | Predictable, debuggable | More code to maintain | High-volume, precise control |

### Multi-tenancy Approaches

| Pattern | Isolation | Performance | Complexity | Use When |
|---------|-----------|-------------|------------|----------|
| Row-Level Security | Good | Excellent | Low | SaaS, shared infrastructure |
| Schema Separation | Excellent | Good | Medium | Enterprise, compliance needs |
| Database per Tenant | Perfect | Variable | High | High security requirements |

### Sync vs Trigger Patterns

| Pattern | Pros | Cons | Use When |
|---------|------|------|----------|
| Explicit Sync | Predictable, debuggable, transactional | More code, must remember to call | High-volume, need control |
| Triggers | Automatic, no extra code | Hard to debug, performance unpredictable | Simple cases, low volume |

## Migration Strategies

### From Traditional ORM

```sql
-- Step 1: Create views alongside ORM models
CREATE OR REPLACE VIEW v_user AS
SELECT /* view definition */ FROM tb_users;

-- Step 2: Gradually move queries to views
-- Step 3: Add functions for mutations with sync
CREATE OR REPLACE FUNCTION fn_create_user(input_data JSONB)
RETURNS mutation_result AS $$
    -- mutation logic
    PERFORM sync_user_projections(NEW.id);
$$ LANGUAGE plpgsql;

-- Step 4: Remove ORM dependencies
```

### Adding to Existing Database

```sql
-- Non-invasive approach
-- 1. Create views over existing tables
CREATE OR REPLACE VIEW v_existing_table AS
SELECT
    id,
    jsonb_build_object(/* map columns */) AS data
FROM existing_table;

-- 2. Add projection tables gradually
CREATE TABLE tv_existing_table_stats AS
SELECT /* aggregated data */ FROM existing_table;

-- 3. Add sync functions for new features
-- 4. Migrate table by table
```

## Best Practices Summary

1. **Use explicit sync over triggers** - Better debugging and performance predictability
2. **Always version your views** - Never break existing APIs
3. **Use layered composition** - Build complex from simple
4. **Index strategically** - Focus on filter and join columns
5. **Batch sync when possible** - More efficient for high volume
6. **Handle nulls gracefully** - Use COALESCE in aggregations
7. **Document sync dependencies** - Track what affects what projections
8. **Monitor sync performance** - Track slow sync functions
9. **Plan for scale** - Consider 10x growth in design

## Next Steps

- Learn about [LLM-Native Architecture](./llm-native-architecture.md) for AI integration
- Review [Domain-Driven Database Design](./domain-driven-database.md) for DDD patterns
- Explore the [Blog API Tutorial](../tutorials/blog-api.md) for practical examples
