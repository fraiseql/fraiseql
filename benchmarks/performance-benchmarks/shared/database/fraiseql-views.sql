-- FraiseQL JSONB Views for GraphQL Resolvers
-- Each view must have an 'id' column and a 'data' JSONB column

SET search_path TO benchmark, public;

-- Drop existing views if they exist
DROP VIEW IF EXISTS v_users CASCADE;
DROP VIEW IF EXISTS v_products CASCADE;
DROP VIEW IF EXISTS v_orders CASCADE;
DROP VIEW IF EXISTS v_categories CASCADE;

-- View for 'users' field with aggregated data
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id::text,
        'email', u.email,
        'username', u.username,
        'fullName', u.full_name,
        'createdAt', u.created_at,
        'isActive', u.is_active,
        'orderCount', COALESCE((
            SELECT COUNT(*)::int
            FROM orders o
            WHERE o.user_id = u.id
        ), 0),
        'totalSpent', COALESCE((
            SELECT SUM(o.total_amount)::float
            FROM orders o
            WHERE o.user_id = u.id
        ), 0.0),
        'reviewCount', COALESCE((
            SELECT COUNT(*)::int
            FROM reviews r
            WHERE r.user_id = u.id
        ), 0),
        'averageRating', (
            SELECT AVG(r.rating)::float
            FROM reviews r
            WHERE r.user_id = u.id
        )
    ) as data
FROM users u;

-- View for 'products' field with aggregated data
CREATE VIEW v_products AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id::text,
        'name', p.name,
        'sku', p.sku,
        'description', p.description,
        'price', p.price,
        'stockQuantity', p.stock_quantity,
        'categoryId', p.category_id::text,
        'createdAt', p.created_at,
        'updatedAt', p.updated_at,
        'reviewCount', COALESCE((
            SELECT COUNT(*)::int
            FROM reviews r
            WHERE r.product_id = p.id
        ), 0),
        'averageRating', (
            SELECT AVG(r.rating)::float
            FROM reviews r
            WHERE r.product_id = p.id
        ),
        'category', CASE WHEN p.category_id IS NOT NULL THEN (
            SELECT jsonb_build_object(
                'id', c.id::text,
                'name', c.name,
                'slug', c.slug,
                'description', c.description,
                'parentId', c.parent_id::text
            )
            FROM categories c
            WHERE c.id = p.category_id
        ) ELSE NULL END,
        'reviews', COALESCE((
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', r.id::text,
                    'rating', r.rating,
                    'title', r.title,
                    'comment', r.comment,
                    'createdAt', r.created_at,
                    'user', jsonb_build_object(
                        'id', u.id::text,
                        'username', u.username,
                        'fullName', u.full_name
                    )
                )
                ORDER BY r.created_at DESC
            )
            FROM reviews r
            JOIN users u ON u.id = r.user_id
            WHERE r.product_id = p.id
        ), '[]'::jsonb)
    ) as data
FROM products p;

-- View for 'orders' field with order items
CREATE VIEW v_orders AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id::text,
        'orderNumber', o.order_number,
        'userId', o.user_id::text,
        'status', o.status,
        'totalAmount', o.total_amount,
        'createdAt', o.created_at,
        'updatedAt', o.updated_at,
        'items', COALESCE((
            SELECT jsonb_agg(
                jsonb_build_object(
                    'id', oi.id::text,
                    'productId', oi.product_id::text,
                    'productName', p.name,
                    'quantity', oi.quantity,
                    'unitPrice', oi.unit_price,
                    'totalPrice', oi.total_price
                )
                ORDER BY oi.id
            )
            FROM order_items oi
            JOIN products p ON p.id = oi.product_id
            WHERE oi.order_id = o.id
        ), '[]'::jsonb)
    ) as data
FROM orders o;

-- View for 'categories' field
CREATE VIEW v_categories AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id::text,
        'name', c.name,
        'slug', c.slug,
        'description', c.description,
        'parentId', c.parent_id::text
    ) as data
FROM categories c;
