-- FraiseQL JSONB Views for GraphQL Resolvers
-- Following the naming convention: v_<entity_plural>
-- Each view must have an 'id' column and a 'data' JSONB column

SET search_path TO benchmark, public;

-- Drop existing views if they exist
DROP VIEW IF EXISTS v_users CASCADE;
DROP VIEW IF EXISTS v_products CASCADE;
DROP VIEW IF EXISTS v_orders CASCADE;
DROP VIEW IF EXISTS v_categories CASCADE;

-- View for 'users' resolver
-- This view will be used when querying: { users { ... } }
CREATE VIEW v_users AS
SELECT
    u.id,
    jsonb_build_object(
        'id', u.id,
        'email', u.email,
        'username', u.username,
        'fullName', u.full_name,
        'createdAt', u.created_at,
        'isActive', u.is_active,
        'orders', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', o.id,
                    'orderNumber', o.order_number,
                    'status', o.status,
                    'totalAmount', o.total_amount,
                    'createdAt', o.created_at,
                    'itemCount', (SELECT COUNT(*) FROM order_items WHERE order_id = o.id),
                    'orderItems', (
                        SELECT jsonb_agg(
                            jsonb_build_object(
                                'id', oi.id,
                                'quantity', oi.quantity,
                                'unitPrice', oi.unit_price,
                                'totalPrice', oi.total_price,
                                'product', (
                                    SELECT jsonb_build_object(
                                        'id', p.id,
                                        'name', p.name,
                                        'price', p.price
                                    )
                                    FROM products p
                                    WHERE p.id = oi.product_id
                                )
                            )
                        )
                        FROM order_items oi
                        WHERE oi.order_id = o.id
                    )
                )
                ORDER BY o.created_at DESC
            )
            FROM orders o
            WHERE o.user_id = u.id),
            '[]'::jsonb
        )
    ) as data
FROM users u;

-- View for 'products' resolver
CREATE VIEW v_products AS
SELECT
    p.id,
    jsonb_build_object(
        'id', p.id,
        'sku', p.sku,
        'name', p.name,
        'description', p.description,
        'price', p.price,
        'stockQuantity', p.stock_quantity,
        'categoryId', p.category_id,
        'isActive', p.is_active,
        'createdAt', p.created_at,
        'category', CASE
            WHEN p.category_id IS NOT NULL THEN (
                SELECT jsonb_build_object(
                    'id', c.id,
                    'name', c.name,
                    'slug', c.slug,
                    'description', c.description
                )
                FROM categories c
                WHERE c.id = p.category_id
            )
            ELSE NULL
        END,
        'reviews', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', r.id,
                    'rating', r.rating,
                    'title', r.title,
                    'comment', r.comment,
                    'createdAt', r.created_at,
                    'isVerifiedPurchase', r.is_verified_purchase,
                    'helpfulCount', r.helpful_count,
                    'user', (
                        SELECT jsonb_build_object(
                            'id', u.id,
                            'username', u.username,
                            'fullName', u.full_name
                        )
                        FROM users u
                        WHERE u.id = r.user_id
                    )
                )
                ORDER BY r.created_at DESC
            )
            FROM reviews r
            WHERE r.product_id = p.id),
            '[]'::jsonb
        ),
        'averageRating', (
            SELECT AVG(rating)::float
            FROM reviews
            WHERE product_id = p.id
        ),
        'reviewCount', (
            SELECT COUNT(*)
            FROM reviews
            WHERE product_id = p.id
        )
    ) as data
FROM products p;

-- View for 'orders' resolver
CREATE VIEW v_orders AS
SELECT
    o.id,
    jsonb_build_object(
        'id', o.id,
        'orderNumber', o.order_number,
        'userId', o.user_id,
        'status', o.status,
        'totalAmount', o.total_amount,
        'createdAt', o.created_at,
        'updatedAt', o.updated_at,
        'shippedAt', o.shipped_at,
        'deliveredAt', o.delivered_at,
        'user', (
            SELECT jsonb_build_object(
                'id', u.id,
                'email', u.email,
                'username', u.username,
                'fullName', u.full_name,
                'isActive', u.is_active
            )
            FROM users u
            WHERE u.id = o.user_id
        ),
        'orderItems', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', oi.id,
                    'quantity', oi.quantity,
                    'unitPrice', oi.unit_price,
                    'totalPrice', oi.total_price,
                    'product', (
                        SELECT jsonb_build_object(
                            'id', p.id,
                            'sku', p.sku,
                            'name', p.name,
                            'price', p.price,
                            'stockQuantity', p.stock_quantity,
                            'category', CASE
                                WHEN p.category_id IS NOT NULL THEN (
                                    SELECT jsonb_build_object(
                                        'id', c.id,
                                        'name', c.name
                                    )
                                    FROM categories c
                                    WHERE c.id = p.category_id
                                )
                                ELSE NULL
                            END
                        )
                        FROM products p
                        WHERE p.id = oi.product_id
                    )
                )
                ORDER BY oi.created_at
            )
            FROM order_items oi
            WHERE oi.order_id = o.id),
            '[]'::jsonb
        ),
        'itemCount', (
            SELECT COUNT(*)
            FROM order_items
            WHERE order_id = o.id
        )
    ) as data
FROM orders o;

-- View for 'categories' resolver
CREATE VIEW v_categories AS
SELECT
    c.id,
    jsonb_build_object(
        'id', c.id,
        'name', c.name,
        'slug', c.slug,
        'description', c.description,
        'parentId', c.parent_id,
        'createdAt', c.created_at,
        'productCount', (
            SELECT COUNT(*)
            FROM products
            WHERE category_id = c.id
        ),
        'products', COALESCE(
            (SELECT jsonb_agg(
                jsonb_build_object(
                    'id', p.id,
                    'name', p.name,
                    'price', p.price,
                    'stockQuantity', p.stock_quantity
                )
                ORDER BY p.name
            )
            FROM products p
            WHERE p.category_id = c.id),
            '[]'::jsonb
        )
    ) as data
FROM categories c;

-- Grant permissions
GRANT SELECT ON v_users TO benchmark;
GRANT SELECT ON v_products TO benchmark;
GRANT SELECT ON v_orders TO benchmark;
GRANT SELECT ON v_categories TO benchmark;
