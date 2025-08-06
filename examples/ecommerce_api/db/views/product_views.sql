-- Product Views for E-commerce API
-- Optimized for GraphQL queries with FraiseQL

-- Product search view with full-text search capabilities
CREATE OR REPLACE VIEW product_search AS
SELECT
    p.id,
    p.sku,
    p.name,
    p.slug,
    p.description,
    p.short_description,
    p.category_id,
    c.name as category_name,
    c.slug as category_slug,
    p.brand,
    p.tags,
    p.is_active,
    p.is_featured,
    p.created_at,
    p.updated_at,
    -- Price range from variants
    MIN(pv.price) as min_price,
    MAX(pv.price) as max_price,
    -- Availability
    COALESCE(SUM(i.quantity - i.reserved_quantity), 0) > 0 as in_stock,
    COALESCE(SUM(i.quantity - i.reserved_quantity), 0) as total_inventory,
    -- Review stats
    COUNT(DISTINCT r.id) as review_count,
    AVG(r.rating)::DECIMAL(3,2) as average_rating,
    -- Primary image
    (SELECT url FROM product_images WHERE product_id = p.id AND is_primary = true LIMIT 1) as primary_image_url,
    -- Search vector
    to_tsvector('english', p.name || ' ' || COALESCE(p.description, '') || ' ' || COALESCE(p.brand, '')) as search_vector
FROM products p
LEFT JOIN categories c ON p.category_id = c.id
LEFT JOIN product_variants pv ON pv.product_id = p.id AND pv.is_active = true
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN reviews r ON r.product_id = p.id AND r.status = 'approved'
GROUP BY p.id, c.name, c.slug;

-- Product detail view with all related data
CREATE OR REPLACE VIEW product_detail AS
SELECT
    p.*,
    -- Category info
    json_build_object(
        'id', c.id,
        'name', c.name,
        'slug', c.slug,
        'parent_id', c.parent_id
    ) as category,
    -- All images
    COALESCE(
        json_agg(DISTINCT
            json_build_object(
                'id', pi.id,
                'url', pi.url,
                'alt_text', pi.alt_text,
                'position', pi.position,
                'is_primary', pi.is_primary
            ) ORDER BY pi.position
        ) FILTER (WHERE pi.id IS NOT NULL),
        '[]'::json
    ) as images,
    -- Variants with inventory
    COALESCE(
        json_agg(DISTINCT
            json_build_object(
                'id', pv.id,
                'sku', pv.sku,
                'name', pv.name,
                'price', pv.price,
                'compare_at_price', pv.compare_at_price,
                'attributes', pv.attributes,
                'inventory', json_build_object(
                    'quantity', i.quantity,
                    'reserved', i.reserved_quantity,
                    'available', i.quantity - i.reserved_quantity
                )
            )
        ) FILTER (WHERE pv.id IS NOT NULL),
        '[]'::json
    ) as variants,
    -- Review summary
    json_build_object(
        'count', COUNT(DISTINCT r.id),
        'average', AVG(r.rating)::DECIMAL(3,2),
        'distribution', json_build_object(
            '5', COUNT(r.id) FILTER (WHERE r.rating = 5),
            '4', COUNT(r.id) FILTER (WHERE r.rating = 4),
            '3', COUNT(r.id) FILTER (WHERE r.rating = 3),
            '2', COUNT(r.id) FILTER (WHERE r.rating = 2),
            '1', COUNT(r.id) FILTER (WHERE r.rating = 1)
        )
    ) as review_summary
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
    ch.*,
    -- Product count
    COUNT(DISTINCT p.id) as product_count,
    -- Subcategories
    COALESCE(
        json_agg(
            json_build_object(
                'id', sub.id,
                'name', sub.name,
                'slug', sub.slug,
                'product_count', (
                    SELECT COUNT(*) FROM products
                    WHERE category_id = sub.id AND is_active = true
                )
            )
        ) FILTER (WHERE sub.id IS NOT NULL),
        '[]'::json
    ) as subcategories
FROM category_hierarchy ch
LEFT JOIN products p ON p.category_id = ch.id AND p.is_active = true
LEFT JOIN categories sub ON sub.parent_id = ch.id AND sub.is_active = true
GROUP BY ch.id, ch.name, ch.slug, ch.description, ch.parent_id,
         ch.image_url, ch.is_active, ch.level, ch.path, ch.full_path;

-- Featured products view
CREATE OR REPLACE VIEW featured_products AS
SELECT
    p.id,
    p.name,
    p.slug,
    p.short_description,
    MIN(pv.price) as price,
    MAX(pv.compare_at_price) as compare_at_price,
    pi.url as image_url,
    c.name as category_name,
    COUNT(DISTINCT r.id) as review_count,
    AVG(r.rating)::DECIMAL(3,2) as average_rating
FROM products p
JOIN categories c ON p.category_id = c.id
LEFT JOIN product_variants pv ON pv.product_id = p.id AND pv.is_active = true
LEFT JOIN product_images pi ON pi.product_id = p.id AND pi.is_primary = true
LEFT JOIN reviews r ON r.product_id = p.id AND r.status = 'approved'
WHERE p.is_active = true AND p.is_featured = true
GROUP BY p.id, p.name, p.slug, p.short_description, pi.url, c.name
ORDER BY p.created_at DESC;

-- Best sellers view (based on order history)
CREATE OR REPLACE VIEW best_sellers AS
SELECT
    p.id,
    p.name,
    p.slug,
    p.short_description,
    MIN(pv.price) as price,
    pi.url as image_url,
    c.name as category_name,
    SUM(oi.quantity) as units_sold,
    COUNT(DISTINCT oi.order_id) as order_count
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

-- Recently viewed products (would be populated by application)
CREATE TABLE IF NOT EXISTS recently_viewed_products (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    customer_id UUID REFERENCES customers(id) ON DELETE CASCADE,
    session_id VARCHAR(255),
    product_id UUID NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    viewed_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    CONSTRAINT viewer CHECK (customer_id IS NOT NULL OR session_id IS NOT NULL)
);

CREATE INDEX idx_recently_viewed_customer ON recently_viewed_products(customer_id, viewed_at DESC);
CREATE INDEX idx_recently_viewed_session ON recently_viewed_products(session_id, viewed_at DESC);

-- Related products view (based on category and tags)
CREATE OR REPLACE VIEW related_products AS
SELECT DISTINCT
    p1.id as source_product_id,
    p2.id as related_product_id,
    p2.name,
    p2.slug,
    p2.short_description,
    MIN(pv.price) as price,
    pi.url as image_url,
    -- Relevance score
    CASE
        WHEN p1.category_id = p2.category_id THEN 3
        ELSE 0
    END +
    CASE
        WHEN p1.brand = p2.brand AND p1.brand IS NOT NULL THEN 2
        ELSE 0
    END +
    COALESCE(array_length(p1.tags & p2.tags, 1), 0) as relevance_score
FROM products p1
JOIN products p2 ON p1.id != p2.id AND p2.is_active = true
LEFT JOIN product_variants pv ON pv.product_id = p2.id AND pv.is_active = true
LEFT JOIN product_images pi ON pi.product_id = p2.id AND pi.is_primary = true
WHERE p1.is_active = true
  AND (
    p1.category_id = p2.category_id OR
    p1.brand = p2.brand OR
    p1.tags && p2.tags
  )
GROUP BY p1.id, p2.id, p2.name, p2.slug, p2.short_description, pi.url,
         p1.category_id, p2.category_id, p1.brand, p2.brand, p1.tags, p2.tags;
