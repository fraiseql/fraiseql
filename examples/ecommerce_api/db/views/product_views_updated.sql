-- Product Views for E-commerce API (Updated for FraiseQL v0.1.0a14+)
-- Uses JSONB data column pattern

-- Product search view with full-text search capabilities
CREATE OR REPLACE VIEW product_search AS
SELECT
    p.id,                    -- For filtering
    p.category_id,           -- For category filtering
    p.is_active,             -- For active products filter
    p.is_featured,           -- For featured filter
    MIN(pv.price) as min_price,  -- For price range queries
    MAX(pv.price) as max_price,  -- For price range queries
    COALESCE(SUM(i.quantity - i.reserved_quantity), 0) > 0 as in_stock,  -- For stock filter
    jsonb_build_object(
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'slug', p.slug,
        'description', p.description,
        'short_description', p.short_description,
        'category_id', p.category_id,
        'category_name', c.name,
        'category_slug', c.slug,
        'brand', p.brand,
        'tags', p.tags,
        'is_active', p.is_active,
        'is_featured', p.is_featured,
        'created_at', p.created_at,
        'updated_at', p.updated_at,
        'min_price', MIN(pv.price),
        'max_price', MAX(pv.price),
        'in_stock', COALESCE(SUM(i.quantity - i.reserved_quantity), 0) > 0,
        'total_inventory', COALESCE(SUM(i.quantity - i.reserved_quantity), 0),
        'review_count', COUNT(DISTINCT r.id),
        'average_rating', AVG(r.rating)::DECIMAL(3,2),
        'primary_image_url', (SELECT url FROM product_images WHERE product_id = p.id AND is_primary = true LIMIT 1)
    ) as data
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN product_variants pv ON pv.product_id = p.id AND pv.is_active = true
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN reviews r ON r.product_id = p.id AND r.status = 'approved'
GROUP BY p.id, c.name, c.slug;

-- Product detail view with all related data
CREATE OR REPLACE VIEW product_detail AS
SELECT
    p.id,                    -- For filtering
    p.category_id,           -- For joins
    p.slug,                  -- For slug lookups
    jsonb_build_object(
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'slug', p.slug,
        'description', p.description,
        'short_description', p.short_description,
        'brand', p.brand,
        'tags', p.tags,
        'is_active', p.is_active,
        'is_featured', p.is_featured,
        'created_at', p.created_at,
        'updated_at', p.updated_at,
        'category', jsonb_build_object(
            'id', c.id,
            'name', c.name,
            'slug', c.slug,
            'parent_id', c.parent_id
        ),
        'images', COALESCE(
            jsonb_agg(DISTINCT
                jsonb_build_object(
                    'id', pi.id,
                    'url', pi.url,
                    'alt_text', pi.alt_text,
                    'position', pi.position,
                    'is_primary', pi.is_primary
                ) ORDER BY pi.position
            ) FILTER (WHERE pi.id IS NOT NULL),
            '[]'::jsonb
        ),
        'variants', COALESCE(
            jsonb_agg(DISTINCT
                jsonb_build_object(
                    'id', pv.id,
                    'sku', pv.sku,
                    'name', pv.name,
                    'price', pv.price,
                    'compare_at_price', pv.compare_at_price,
                    'attributes', pv.attributes,
                    'inventory', jsonb_build_object(
                        'quantity', i.quantity,
                        'reserved', i.reserved_quantity,
                        'available', i.quantity - i.reserved_quantity
                    )
                )
            ) FILTER (WHERE pv.id IS NOT NULL),
            '[]'::jsonb
        ),
        'review_summary', jsonb_build_object(
            'count', COUNT(DISTINCT r.id),
            'average', AVG(r.rating)::DECIMAL(3,2),
            'distribution', jsonb_build_object(
                '5', COUNT(r.id) FILTER (WHERE r.rating = 5),
                '4', COUNT(r.id) FILTER (WHERE r.rating = 4),
                '3', COUNT(r.id) FILTER (WHERE r.rating = 3),
                '2', COUNT(r.id) FILTER (WHERE r.rating = 2),
                '1', COUNT(r.id) FILTER (WHERE r.rating = 1)
            )
        )
    ) as data
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN product_images pi ON pi.product_id = p.id
LEFT JOIN product_variants pv ON pv.product_id = p.id
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN reviews r ON r.product_id = p.id AND r.status = 'approved'
GROUP BY p.id, c.id, c.name, c.slug, c.parent_id;

-- Category tree view for navigation
CREATE OR REPLACE VIEW category_tree AS
WITH RECURSIVE category_hierarchy AS (
    -- Base case: root categories
    SELECT
        id,
        name,
        slug,
        description,
        parent_id,
        image_url,
        is_active,
        0 as level,
        ARRAY[id] as path,
        name::TEXT as full_path
    FROM categories
    WHERE parent_id IS NULL

    UNION ALL

    -- Recursive case
    SELECT
        c.id,
        c.name,
        c.slug,
        c.description,
        c.parent_id,
        c.image_url,
        c.is_active,
        ch.level + 1,
        ch.path || c.id,
        ch.full_path || ' > ' || c.name
    FROM categories c
    JOIN category_hierarchy ch ON c.parent_id = ch.id
)
SELECT
    ch.id,                   -- For filtering
    ch.parent_id,            -- For hierarchy queries
    ch.is_active,            -- For active filter
    ch.level,                -- For level-based queries
    jsonb_build_object(
        'id', ch.id,
        'name', ch.name,
        'slug', ch.slug,
        'description', ch.description,
        'parent_id', ch.parent_id,
        'image_url', ch.image_url,
        'is_active', ch.is_active,
        'level', ch.level,
        'path', ch.path,
        'full_path', ch.full_path,
        'product_count', COUNT(DISTINCT p.id),
        'subcategories', COALESCE(
            jsonb_agg(
                jsonb_build_object(
                    'id', sub.id,
                    'name', sub.name,
                    'slug', sub.slug,
                    'product_count', (
                        SELECT COUNT(*) FROM products
                        WHERE category_id = sub.id AND is_active = true
                    )
                )
            ) FILTER (WHERE sub.id IS NOT NULL),
            '[]'::jsonb
        )
    ) as data
FROM category_hierarchy ch
LEFT JOIN products p ON p.category_id = ch.id AND p.is_active = true
LEFT JOIN categories sub ON sub.parent_id = ch.id AND sub.is_active = true
GROUP BY ch.id, ch.name, ch.slug, ch.description, ch.parent_id,
         ch.image_url, ch.is_active, ch.level, ch.path, ch.full_path;

-- Featured products view
CREATE OR REPLACE VIEW featured_products AS
SELECT
    p.id,                    -- For filtering
    p.category_id,           -- For category queries
    p.created_at,            -- For ordering
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'slug', p.slug,
        'short_description', p.short_description,
        'price', MIN(pv.price),
        'compare_at_price', MAX(pv.compare_at_price),
        'image_url', pi.url,
        'category_name', c.name,
        'review_count', COUNT(DISTINCT r.id),
        'average_rating', AVG(r.rating)::DECIMAL(3,2)
    ) as data
FROM products p
JOIN categories c ON p.category_id = c.id
LEFT JOIN product_variants pv ON pv.product_id = p.id AND pv.is_active = true
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
LEFT JOIN reviews r ON r.product_id = p.id AND r.status = 'approved'
WHERE p.is_active = true AND p.is_featured = true
GROUP BY p.id, p.name, p.slug, p.short_description, pi.url, c.name, p.created_at
ORDER BY p.created_at DESC;

-- Best sellers view (based on order history)
CREATE OR REPLACE VIEW best_sellers AS
SELECT
    p.id,                    -- For filtering
    p.category_id,           -- For category queries
    SUM(oi.quantity) as units_sold,  -- For ordering
    jsonb_build_object(
        'id', p.id,
        'name', p.name,
        'slug', p.slug,
        'short_description', p.short_description,
        'price', MIN(pv.price),
        'image_url', pi.url,
        'category_name', c.name,
        'units_sold', SUM(oi.quantity),
        'order_count', COUNT(DISTINCT oi.order_id)
    ) as data
FROM products p
JOIN product_variants pv ON pv.product_id = p.id
JOIN order_items oi ON oi.variant_id = pv.id
JOIN orders o ON o.id = oi.order_id AND o.status IN ('completed', 'shipped')
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
LEFT JOIN categories c ON p.category_id = c.id
WHERE p.is_active = true
  AND o.created_at >= CURRENT_DATE - INTERVAL '30 days'
GROUP BY p.id, p.name, p.slug, p.short_description, pi.url, c.name
ORDER BY units_sold DESC
LIMIT 20;
